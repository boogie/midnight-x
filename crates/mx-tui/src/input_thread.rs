//! Spawn a background thread that polls `crossterm::event::read()` and forwards
//! translated `mx_core::Event`s onto the supplied `mpsc::Sender`. The thread
//! exits when `should_run.load() == false`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event as ct;

use mx_core::event::Event;

use crate::input_xlate::translate;

pub struct InputThread {
    pub handle: JoinHandle<()>,
    pub should_run: Arc<AtomicBool>,
}

/// Spawn the input thread. Returns a handle the caller uses to stop it.
///
/// # Panics
///
/// Panics if the OS refuses to spawn a thread (extremely unlikely).
#[must_use]
pub fn spawn(tx: Sender<Event>) -> InputThread {
    let should_run = Arc::new(AtomicBool::new(true));
    let sr = Arc::clone(&should_run);
    let handle = thread::Builder::new()
        .name("mx-input".into())
        .spawn(move || {
            while sr.load(Ordering::Relaxed) {
                // Poll so the thread can notice should_run flipping.
                match ct::poll(Duration::from_millis(100)) {
                    Ok(true) => match ct::read() {
                        Ok(ct::Event::Resize(w, h)) => {
                            let _ = tx.send(Event::Resize { cols: w, rows: h });
                        }
                        Ok(ev) => {
                            if let Some(input) = translate(ev) {
                                let _ = tx.send(Event::Input(input));
                            }
                        }
                        Err(_) => break,
                    },
                    Ok(false) => { /* timed out, loop and check should_run */ }
                    Err(_) => break,
                }
            }
        })
        .expect("input thread cannot fail to spawn");
    InputThread { handle, should_run }
}

impl InputThread {
    /// Stop the thread. Joins on the handle.
    pub fn stop(self) {
        self.should_run.store(false, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}
