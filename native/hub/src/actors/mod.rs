pub(crate) mod download_actor;

#[derive(Debug, thiserror::Error)]
pub enum CreateActorsError {
    #[error("failed to resolve application data directory")]
    ResolveDataDirectory(#[from] fluxdown_engine::data_dir::DataDirError),
    #[error("download actor failed")]
    DownloadActor(#[from] download_actor::ActorError),
}

pub async fn create_actors(
    shutdown: tokio_util::sync::CancellationToken,
) -> Result<(), CreateActorsError> {
    // Determine the data directory using the shared resolver.
    //
    // Linux:   $XDG_DATA_HOME/fluxdown  (~/.local/share/fluxdown)
    // macOS:   ~/Library/Application Support/fluxdown
    // Windows portable (marker file present): exe directory
    // Windows installed: %LOCALAPPDATA%\FluxDown
    let db_dir = fluxdown_engine::data_dir::resolve_data_dir(None)?;
    download_actor::run(db_dir, shutdown).await?;
    Ok(())
}

// 先停下载，再排空进度，最后持久化活动。保留 Engine 的其余字段，
// 让 engine.lock 在最后一次写入完成之前始终由当前宿主持有。
async fn shutdown_engine(
    mut engine: fluxdown_engine::Engine,
    progress_task: Option<tokio::task::JoinHandle<()>>,
) {
    use std::time::Duration;

    if let Err(error) =
        tokio::time::timeout(Duration::from_secs(10), engine.manager.shutdown()).await
    {
        crate::logger::report_error("hub", "stop downloads timed out", &error);
    }
    let journal = engine.activity_journal();
    // 释放最后一个 manager progress sender，reporter 才能读到 EOF。
    drop(engine.manager);
    if let Some(mut progress_task) = progress_task {
        match tokio::time::timeout(Duration::from_secs(10), &mut progress_task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => crate::logger::report_error("hub", "drain task progress", &error),
            Err(error) => {
                progress_task.abort();
                let _ = progress_task.await;
                crate::logger::report_error("hub", "drain task progress timed out", &error);
            }
        }
    }
    if let Err(error) = journal.flush().await {
        tracing::error!(%error, "Flutter task activity journal final flush failed");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fluxdown_engine::bt_downloader::BtConfig;
    use fluxdown_engine::db::Db;
    use fluxdown_engine::download_manager::progress_reporter;
    use fluxdown_engine::downloader::ProgressUpdate;
    use fluxdown_engine::proxy_config::ProxyConfig;
    use fluxdown_engine::task_activity::TaskActivityQuery;
    use fluxdown_engine::{Engine, EngineConfig, NoopSelection, NoopSink};

    #[test]
    fn shutdown_persists_queued_terminal_activity_before_runtime_exits() {
        let dir = std::env::temp_dir().join(format!("fluxdown-hub-stop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create test directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let engine = Engine::new(
                EngineConfig {
                    max_concurrent: 1,
                    speed_limit_bps: 0,
                    upload_limit_bps: 0,
                    default_save_dir: dir.to_string_lossy().into_owned(),
                    app_data_dir: dir.to_string_lossy().into_owned(),
                    bt_config: BtConfig::default(),
                    proxy_config: ProxyConfig::default(),
                    user_agent: String::new(),
                    data_dir_override: Some(dir.clone()),
                    database_url: None,
                },
                Arc::new(NoopSink),
                Arc::new(NoopSelection),
            )
            .await
            .expect("engine");
            engine
                .db
                .insert_task(
                    "tail",
                    "http://example.test/file",
                    "file",
                    "/tmp",
                    1,
                    0,
                    "",
                    "main",
                    "",
                    2,
                )
                .await
                .expect("insert task");
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            let reporter = tokio::spawn(progress_reporter(
                rx,
                engine.db.clone(),
                engine.activity_sink.clone(),
            ));
            tx.try_send(ProgressUpdate {
                task_id: "tail".into(),
                status: 4,
                error_message: "connection closed during shutdown".into(),
                ..Default::default()
            })
            .expect("queue terminal frame without yielding");
            drop(tx);
            super::shutdown_engine(engine, Some(reporter)).await;
        });
        // 先销毁原 runtime，防止查询的 await 偶然替漏掉的 flush 排空队列。
        drop(runtime);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("reopen runtime");
        runtime.block_on(async {
            let (db, _lease) = Db::open_exclusive(&dir)
                .await
                .expect("released writer lease");
            let page = db
                .query_task_activity(&TaskActivityQuery {
                    task_id: "tail".into(),
                    ..Default::default()
                })
                .await
                .expect("read activity after restart");
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.entries[0].status, Some(4));
            assert_eq!(page.entries[0].message, "connection closed during shutdown");
        });
        drop(runtime);
        std::fs::remove_dir_all(dir).expect("remove test directory");
    }
}
