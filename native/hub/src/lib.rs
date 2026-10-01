mod actors;
mod logger;
pub mod rinf_selection;
mod rinf_sink;
mod signal_bridge;
mod signals;
mod updater;

use actors::create_actors;
use rinf::{dart_shutdown, write_interface};

write_interface!();

// RUNTIME CONSTRAINT: This binary uses a single-threaded (`current_thread`) Tokio runtime.
// All tasks share the same OS thread, so blocking operations (blocking I/O, `std::thread::sleep`,
// `Mutex::lock` held across `.await`, etc.) will stall every other task on the runtime.
//
// Rules for contributors:
//   • Never call blocking APIs directly in `async fn` — wrap them in `tokio::task::spawn_blocking`.
//   • Never use `mpsc::Sender::blocking_send` inside a `tokio::spawn(async { … })` block;
//     use `.send(…).await` instead. `blocking_send` is only safe inside `spawn_blocking` closures.
//   • Never park the thread with `std::thread::sleep` or synchronous `Mutex` contention in async code.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = logger::init() {
        // 同进程二次 isolate：logger 已由上次 runtime 装好，继续拉 actor。
        if !error.is_already_initialized() {
            eprintln!(
                "FluxDown logger initialization failed: {}",
                logger::format_error_chain(&error)
            );
            return;
        }
    }
    let shutdown = tokio_util::sync::CancellationToken::new();
    let actor_shutdown = shutdown.clone();
    let mut actor_task = logger::spawn_logged("hub", "create actors", async move {
        create_actors(actor_shutdown).await?;
        Ok::<(), actors::CreateActorsError>(())
    });
    dart_shutdown().await;
    shutdown.cancel();
    match tokio::time::timeout(std::time::Duration::from_secs(30), &mut actor_task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => logger::report_error("hub", "stop actors", &error),
        Err(error) => logger::report_error("hub", "stop actors timed out", &error),
    }
}
