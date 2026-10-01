use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::downloader::DownloadError;

static LEASES: OnceLock<Mutex<HashMap<PathBuf, Weak<Lease>>>> = OnceLock::new();

fn leases() -> &'static Mutex<HashMap<PathBuf, Weak<Lease>>> {
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Lease {
    path: PathBuf,
    task_id: String,
    runtime_task: Option<tokio::task::Id>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut leases = leases().lock().unwrap_or_else(|e| e.into_inner());
        // A new lease can replace an expired weak entry before this drop obtains the mutex.
        if leases
            .get(&self.path)
            .is_some_and(|lease| std::ptr::eq(lease.as_ptr(), self))
        {
            leases.remove(&self.path);
        }
    }
}

// Nested download modes and detached workers share ownership until their last writer exits.
#[derive(Clone)]
pub(crate) struct TempFileGuard {
    _lease: Arc<Lease>,
}

impl TempFileGuard {
    pub(crate) async fn acquire(path: &Path, task_id: &str) -> Result<Self, DownloadError> {
        let key = normalized_path(path).await?;
        let mut leases = leases().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(lease) = leases.get(&key).and_then(Weak::upgrade) {
            // Drop the registry mutex before a strong reference: its destructor also locks it.
            drop(leases);
            // Inline mode changes may reenter; a separate spawned run is still another writer.
            if lease.task_id == task_id && lease.runtime_task == tokio::task::try_id() {
                return Ok(Self { _lease: lease });
            }
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "temporary file is already being written by task {}: {}; retry after that task stops",
                    lease.task_id,
                    path.display()
                ),
            )
            .into());
        }
        let lease = Arc::new(Lease {
            path: key.clone(),
            task_id: task_id.to_owned(),
            runtime_task: tokio::task::try_id(),
        });
        leases.insert(key, Arc::downgrade(&lease));
        Ok(Self { _lease: lease })
    }
}

async fn normalized_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "temporary file path has no file name",
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Canonicalizing the parent also works before the temporary file exists, and resolves
    // relative paths, dot components and directory aliases without changing its key at creation.
    let normalized = tokio::fs::canonicalize(parent).await?.join(file_name);
    let normalized = match tokio::fs::canonicalize(&normalized).await {
        Ok(path) => path,
        Err(e) if e.kind() == io::ErrorKind::NotFound => normalized,
        Err(e) => return Err(e),
    };
    #[cfg(target_os = "windows")]
    let normalized = PathBuf::from(normalized.to_string_lossy().to_lowercase());
    Ok(normalized)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::TempFileGuard;
    use crate::downloader::DownloadError;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::Barrier;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("fluxdown_temp_lease_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self) -> PathBuf {
            self.0.join("file.bin.fdownloading")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0)
                && error.kind() != ErrorKind::NotFound
            {
                // Drop can run during unwinding; report without causing a second panic.
                eprintln!("remove test directory {}: {error}", self.0.display());
            }
        }
    }

    fn assert_busy(result: Result<TempFileGuard, DownloadError>) {
        assert!(matches!(result, Err(DownloadError::Io(e)) if e.kind() == ErrorKind::WouldBlock));
    }

    #[tokio::test]
    async fn concurrent_tasks_allow_only_one_writer() {
        let dir = TestDir::new();
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for task_id in ["first", "second"] {
            let barrier = barrier.clone();
            let path = dir.file();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                let lease = TempFileGuard::acquire(&path, "task").await;
                let wrote = match &lease {
                    Ok(_) => {
                        tokio::fs::write(&path, task_id.as_bytes()).await.unwrap();
                        true
                    }
                    Err(DownloadError::Io(e)) if e.kind() == ErrorKind::WouldBlock => false,
                    Err(e) => panic!("unexpected error: {e}"),
                };
                barrier.wait().await;
                wrote
            }));
        }
        barrier.wait().await;
        barrier.wait().await;
        let first = handles.remove(0).await.unwrap();
        let second = handles.remove(0).await.unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read(dir.file()).unwrap(),
            if first {
                b"first".as_slice()
            } else {
                b"second".as_slice()
            }
        );
    }

    #[tokio::test]
    async fn nested_same_task_and_worker_clones_keep_ownership_until_last_drop() {
        let dir = TestDir::new();
        let outer = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        let nested = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        let worker = nested.clone();
        drop(outer);
        drop(nested);
        assert_busy(TempFileGuard::acquire(&dir.file(), "other").await);
        drop(worker);
        let retry = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        drop(retry);
        let other = TempFileGuard::acquire(&dir.file(), "other").await.unwrap();
        drop(other);
    }

    #[tokio::test]
    async fn normalized_alias_stays_exclusive_after_file_creation() {
        let dir = TestDir::new();
        std::fs::create_dir_all(dir.0.join("child")).unwrap();
        let alias = dir.0.join("child").join("..").join("file.bin.fdownloading");
        let lease = TempFileGuard::acquire(&alias, "first").await.unwrap();
        std::fs::write(dir.file(), b"preserved").unwrap();
        assert_busy(TempFileGuard::acquire(&dir.file(), "second").await);
        assert_eq!(std::fs::read(dir.file()).unwrap(), b"preserved");
        drop(lease);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_directory_and_file_share_writer_ownership() {
        let dir = TestDir::new();
        std::fs::create_dir_all(dir.0.join("real")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("alias")).unwrap();
        let path = dir.0.join("real").join("file.fdownloading");
        let alias = dir.0.join("alias").join("file.fdownloading");
        let lease = TempFileGuard::acquire(&alias, "first").await.unwrap();
        std::fs::write(&path, b"preserved").unwrap();
        assert_busy(TempFileGuard::acquire(&path, "second").await);
        std::os::unix::fs::symlink(&path, dir.0.join("linked.fdownloading")).unwrap();
        assert_busy(TempFileGuard::acquire(&dir.0.join("linked.fdownloading"), "second").await);
        drop(lease);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_path_case_does_not_create_a_second_writer() {
        let dir = TestDir::new();
        let path = dir.0.join("File.BIN.fdownloading");
        let lease = TempFileGuard::acquire(&path, "first").await.unwrap();
        assert_busy(TempFileGuard::acquire(&dir.file(), "second").await);
        std::fs::write(&path, b"preserved").unwrap();
        assert_busy(TempFileGuard::acquire(&dir.file(), "second").await);
        drop(lease);
    }

    #[tokio::test]
    async fn cancellation_releases_lease_for_resume() {
        let dir = TestDir::new();
        let path = dir.file();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let _lease = TempFileGuard::acquire(&path, "task").await.unwrap();
            tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        rx.await.unwrap();
        assert_busy(TempFileGuard::acquire(&dir.file(), "other").await);
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
        let resume = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        drop(resume);
        let other = TempFileGuard::acquire(&dir.file(), "other").await.unwrap();
        drop(other);
    }

    #[tokio::test]
    async fn panic_unwind_releases_lease_for_retry() {
        let dir = TestDir::new();
        let lease = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        let panic = std::panic::catch_unwind(move || {
            let _lease = lease;
            panic!("download panic");
        });
        assert!(panic.is_err());
        let retry = TempFileGuard::acquire(&dir.file(), "task").await.unwrap();
        drop(retry);
        let other = TempFileGuard::acquire(&dir.file(), "other").await.unwrap();
        drop(other);
    }
}
