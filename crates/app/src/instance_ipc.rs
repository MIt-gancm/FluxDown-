//! 单实例激活通道：次实例把链接 / 激活请求交给主实例后退出。
//!
//! Unix 用 `<instance_dir>/activate.sock`，Windows 用命名管道
//! `\\.\pipe\fluxdown-desktop-<instance_dir FNV hash>`。消息为一行 JSON，主实例
//! 仅在已接收消息后确认；绑定端点先于 UI / agent 初始化，因此并发启动不会丢失唤起。

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

const ACKNOWLEDGEMENT: &[u8] = b"ok\n";
const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 次实例 → 主实例的一次请求。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateMessage {
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub activate: bool,
}

/// 已排入 UI 事件泵的激活请求；仅在 UI 实际处理后确认给次实例。
pub(crate) struct ActivationRequest {
    message: ActivateMessage,
    acknowledgement: oneshot::Sender<()>,
}

impl ActivationRequest {
    fn new(message: ActivateMessage, acknowledgement: oneshot::Sender<()>) -> Self {
        Self {
            message,
            acknowledgement,
        }
    }

    pub(crate) fn into_parts(self) -> (ActivateMessage, oneshot::Sender<()>) {
        (self.message, self.acknowledgement)
    }
}
/// 通道端点（socket 路径或管道名）。
#[derive(Debug, Clone)]
pub struct Endpoint(String);

impl Endpoint {
    #[must_use]
    pub fn for_instance_dir(instance_dir: &Path) -> Self {
        #[cfg(unix)]
        {
            Self(instance_dir.join("activate.sock").display().to_string())
        }
        #[cfg(windows)]
        {
            let hash = instance_hash(instance_dir);
            Self(format!(r"\\.\pipe\fluxdown-desktop-{hash:016x}"))
        }
    }
}

#[cfg(windows)]
fn instance_hash(instance_dir: &Path) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in instance_dir.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// 不能把请求交给主实例的原因。
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("failed to encode FluxDown desktop activation request")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to reach the FluxDown desktop activation endpoint before sending")]
    Io(#[source] io::Error),
    #[error("FluxDown desktop activation may already have been delivered")]
    DeliveryUncertain(#[source] io::Error),
}

impl SendError {
    /// 仅当尚未写出请求时才允许重试，以免捕获请求被重复提交。
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::NotConnected
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::UnexpectedEof
                )
        )
    }
}

/// 已绑定的主实例激活端点。端点在完整 UI 初始化前建立，以便内核排队并发启动的请求。
pub struct Listener {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(unix)]
    listener: std::os::unix::net::UnixListener,
    #[cfg(windows)]
    name: String,
    #[cfg(windows)]
    server: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl Listener {
    /// 在成为主实例后立即绑定端点。旧 Unix socket 只能来自已经退出的主实例。
    pub fn bind(endpoint: Endpoint) -> Result<Self, io::Error> {
        #[cfg(unix)]
        {
            let path = PathBuf::from(&endpoint.0);
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != io::ErrorKind::NotFound
            {
                return Err(error);
            }
            let listener = std::os::unix::net::UnixListener::bind(&path)?;
            listener.set_nonblocking(true)?;
            Ok(Self { path, listener })
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;

            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint.0)?;
            Ok(Self {
                name: endpoint.0,
                server,
            })
        }
    }

    /// 主实例：将已确认的请求送入 UI 事件泵。
    pub async fn listen(mut self, tx: mpsc::Sender<ActivationRequest>) {
        #[cfg(unix)]
        self.listen_unix(tx).await;
        #[cfg(windows)]
        self.listen_windows(tx).await;
    }

    #[cfg(unix)]
    async fn listen_unix(&mut self, tx: mpsc::Sender<ActivationRequest>) {
        let Ok(listener) = self.listener.try_clone() else {
            eprintln!("FluxDown desktop activation socket unavailable");
            return;
        };
        let Ok(listener) = tokio::net::UnixListener::from_std(listener) else {
            eprintln!("FluxDown desktop activation socket unavailable");
            return;
        };
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                receive_and_ack_unix(stream, tx).await;
            });
        }
    }

    #[cfg(windows)]
    async fn listen_windows(&mut self, tx: mpsc::Sender<ActivationRequest>) {
        use tokio::net::windows::named_pipe::ServerOptions;

        loop {
            if self.server.connect().await.is_err() {
                continue;
            }
            let connected = std::mem::replace(
                &mut self.server,
                match ServerOptions::new().create(&self.name) {
                    Ok(next) => next,
                    Err(_) => return,
                },
            );
            let tx = tx.clone();
            tokio::spawn(async move {
                receive_and_ack_windows(connected, tx).await;
            });
        }
    }
}

#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 次实例：确认主实例已处理请求后才返回成功。
pub fn send_to_primary(endpoint: &Endpoint, message: &ActivateMessage) -> Result<(), SendError> {
    let mut line = serde_json::to_string(message).map_err(SendError::Serialize)?;
    line.push('\n');
    send_line(endpoint, &line)
}

#[cfg(unix)]
fn send_line(endpoint: &Endpoint, line: &str) -> Result<(), SendError> {
    use std::io::{Read as _, Write as _};

    let mut stream = std::os::unix::net::UnixStream::connect(&endpoint.0).map_err(SendError::Io)?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(SendError::Io)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(SendError::Io)?;
    stream
        .write_all(line.as_bytes())
        .map_err(SendError::DeliveryUncertain)?;
    stream.flush().map_err(SendError::DeliveryUncertain)?;
    let mut acknowledgement = [0; ACKNOWLEDGEMENT.len()];
    stream
        .read_exact(&mut acknowledgement)
        .map_err(SendError::DeliveryUncertain)?;
    if acknowledgement == ACKNOWLEDGEMENT {
        Ok(())
    } else {
        Err(SendError::DeliveryUncertain(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected activation acknowledgement",
        )))
    }
}

#[cfg(windows)]
fn send_line(endpoint: &Endpoint, line: &str) -> Result<(), SendError> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::windows::named_pipe::ClientOptions;

    let endpoint = endpoint.0.clone();
    let line = line.to_owned();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(SendError::Io)?;
    runtime.block_on(async move {
        let mut client = ClientOptions::new()
            .open(&endpoint)
            .map_err(SendError::Io)?;
        tokio::time::timeout(IO_TIMEOUT, async move {
            client
                .write_all(line.as_bytes())
                .await
                .map_err(SendError::DeliveryUncertain)?;
            client.flush().await.map_err(SendError::DeliveryUncertain)?;
            let mut acknowledgement = [0; ACKNOWLEDGEMENT.len()];
            client
                .read_exact(&mut acknowledgement)
                .await
                .map_err(SendError::DeliveryUncertain)?;
            if acknowledgement == ACKNOWLEDGEMENT {
                Ok(())
            } else {
                Err(SendError::DeliveryUncertain(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected activation acknowledgement",
                )))
            }
        })
        .await
        .map_err(|_| {
            SendError::DeliveryUncertain(io::Error::new(
                io::ErrorKind::TimedOut,
                "activation acknowledgement timed out",
            ))
        })?
    })
}

#[cfg(unix)]
async fn receive_and_ack_unix(
    mut stream: tokio::net::UnixStream,
    tx: mpsc::Sender<ActivationRequest>,
) {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let mut line = String::new();
    let read = {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut line).await
    };
    if matches!(read, Ok(count) if count > 0)
        && let Ok(message) = serde_json::from_str::<ActivateMessage>(line.trim())
    {
        let (acknowledgement, confirmed) = oneshot::channel();
        if tx
            .send(ActivationRequest::new(message, acknowledgement))
            .await
            .is_ok()
            && confirmed.await.is_ok()
        {
            let _ = stream.write_all(ACKNOWLEDGEMENT).await;
            let _ = stream.flush().await;
        }
    }
}

#[cfg(windows)]
async fn receive_and_ack_windows(
    mut stream: tokio::net::windows::named_pipe::NamedPipeServer,
    tx: mpsc::Sender<ActivationRequest>,
) {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let mut line = String::new();
    let read = {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut line).await
    };
    if matches!(read, Ok(count) if count > 0)
        && let Ok(message) = serde_json::from_str::<ActivateMessage>(line.trim())
    {
        let (acknowledgement, confirmed) = oneshot::channel();
        if tx
            .send(ActivationRequest::new(message, acknowledgement))
            .await
            .is_ok()
            && confirmed.await.is_ok()
        {
            let _ = stream.write_all(ACKNOWLEDGEMENT).await;
            let _ = stream.flush().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{ActivateMessage, Endpoint, Listener, SendError, send_to_primary};

    fn test_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::path::PathBuf::from("/tmp").join(format!("fd-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn message_round_trips_and_tolerates_missing_fields() {
        let message = ActivateMessage {
            urls: vec!["https://a/b".to_owned()],
            files: vec!["/tmp/x.torrent".into()],
            activate: true,
        };
        let json = serde_json::to_string(&message).expect("serialize");
        assert_eq!(
            serde_json::from_str::<ActivateMessage>(&json).expect("parse"),
            message
        );
        assert_eq!(
            serde_json::from_str::<ActivateMessage>("{}").expect("parse empty"),
            ActivateMessage::default()
        );
    }

    #[cfg(unix)]
    #[test]
    fn queued_request_is_confirmed_after_listener_task_starts() {
        let dir = test_dir("queued");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let endpoint = Endpoint::for_instance_dir(&dir);
        let listener = Listener::bind(endpoint.clone()).expect("bind listener");
        let sent = ActivateMessage {
            urls: vec!["magnet:?xt=abc".to_owned()],
            files: Vec::new(),
            activate: true,
        };
        let sender_endpoint = endpoint.clone();
        let sender_message = sent.clone();
        let sender = std::thread::spawn(move || send_to_primary(&sender_endpoint, &sender_message));

        std::thread::sleep(Duration::from_millis(25));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let task = runtime.spawn(listener.listen(tx));
        let received = runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(2), rx.recv()).await })
            .expect("timeout")
            .expect("message");
        let (received, acknowledgement) = received.into_parts();
        assert_eq!(received, sent);
        // Cold UI/font initialization can take longer than a single frame.
        std::thread::sleep(Duration::from_millis(400));
        acknowledgement.send(()).expect("confirm activation");
        sender
            .join()
            .expect("sender thread")
            .expect("acknowledged activation");
        task.abort();
        let _ = runtime.block_on(task);
        drop(runtime);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn missing_listener_is_retryable() {
        let dir = test_dir("missing");
        let endpoint = Endpoint::for_instance_dir(&dir);
        let error =
            send_to_primary(&endpoint, &ActivateMessage::default()).expect_err("no listener");
        assert!(matches!(error, SendError::Io(_)));
        assert!(error.is_retryable());
    }

    #[cfg(unix)]
    #[test]
    fn unacknowledged_delivery_is_not_retried() {
        use std::io::Read as _;

        let dir = test_dir("no-ack");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let endpoint = Endpoint::for_instance_dir(&dir);
        let listener = std::os::unix::net::UnixListener::bind(&endpoint.0).expect("bind listener");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 256];
            let _ = stream.read(&mut request);
            std::thread::sleep(Duration::from_millis(300));
        });
        let error = send_to_primary(&endpoint, &ActivateMessage::default())
            .expect_err("missing acknowledgement");
        assert!(matches!(error, SendError::DeliveryUncertain(_)));
        assert!(!error.is_retryable());
        server.join().expect("server thread");
        let _ = std::fs::remove_dir_all(dir);
    }
}
