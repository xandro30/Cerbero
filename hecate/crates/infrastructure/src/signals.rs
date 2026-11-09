//! Signals through tokio (research R13): the CLI's Ctrl-C and the orderly stop of `serve`.

use std::io;

/// Calls `on_interrupt` once on the first Ctrl-C (SIGINT), from a background thread. From then on
/// SIGINT no longer ends the process by itself: the caller decides how to cancel.
pub fn on_interrupt(on_interrupt: impl FnOnce() + Send + 'static) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let signal = runtime.block_on(async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    })?;
    std::thread::spawn(move || {
        let mut signal = signal;
        runtime.block_on(async move {
            if signal.recv().await.is_some() {
                on_interrupt();
            }
        });
    });
    Ok(())
}

/// Waits for SIGINT or SIGTERM: the orderly stop of `serve`.
pub async fn stop_requested() -> io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }
    Ok(())
}
