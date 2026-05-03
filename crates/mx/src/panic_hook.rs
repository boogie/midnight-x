//! Install a panic hook that restores the terminal and logs the panic before
//! the process dies, so the user gets a usable shell instead of a wedged
//! raw-mode TTY.

use std::panic;

use mx_tui::renderer;

/// Install the hook. Idempotent.
pub fn install() {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = renderer::release();
        eprintln!("\nmx crashed: {info}");
        if let Ok(log) = std::env::var("MX_LOG_PATH") {
            eprintln!("see log: {log}");
        }
        tracing::error!(target: "mx", "panic: {info}");
        prev(info);
    }));
}
