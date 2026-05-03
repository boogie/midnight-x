//! Worker executor — the only `std::thread::spawn` site for app workers.
//! Phase 2 ships only directory scanning; Phase 3 adds copy/move/delete.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use camino::Utf8PathBuf;

use mx_core::event::{Event, WorkerId, WorkerMsg};
use mx_core::state::{PanelSide, SortMode};

use crate::dir_scan::scan;

pub struct Executor {
    tx: Sender<Event>,
    next_id: AtomicU64,
    handles: HashMap<WorkerId, JoinHandle<()>>,
}

impl Executor {
    #[must_use]
    pub fn new(tx: Sender<Event>) -> Self {
        Self {
            tx,
            next_id: AtomicU64::new(1),
            handles: HashMap::new(),
        }
    }

    fn alloc_id(&self) -> WorkerId {
        WorkerId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Spawn a directory scan in a worker thread. Sends exactly one
    /// `WorkerMsg`: `DirScanned` on success, or `Failed` on error.
    ///
    /// # Panics
    ///
    /// Panics if the OS refuses to spawn a thread (extremely unlikely).
    pub fn start_dir_scan(
        &mut self,
        side: PanelSide,
        dir: Utf8PathBuf,
        show_hidden: bool,
        sort: SortMode,
    ) -> WorkerId {
        let id = self.alloc_id();
        let tx = self.tx.clone();
        let handle = thread::Builder::new()
            .name(format!("mx-fs-scan-{}", id.0))
            .spawn(move || {
                let msg = match scan(&dir, show_hidden, sort) {
                    Ok(entries) => WorkerMsg::DirScanned { side, entries },
                    Err(e) => WorkerMsg::Failed {
                        errors: vec![(dir.clone(), e)],
                    },
                };
                let _ = tx.send(Event::Worker(id, msg));
            })
            .expect("worker thread cannot fail to spawn");
        self.handles.insert(id, handle);
        id
    }

    /// Drop completed workers from the handle map. Called periodically by
    /// the main loop so the map doesn't grow unbounded.
    pub fn reap(&mut self) {
        self.handles.retain(|_, h| !h.is_finished());
    }

    /// Wait for all in-flight workers to finish. Used during shutdown.
    pub fn join_all(&mut self) {
        let handles: Vec<_> = self.handles.drain().collect();
        for (_, h) in handles {
            let _ = h.join();
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.handles.len()
    }
}

/// Shared facade some callers prefer. Phase 2 doesn't use it; Phase 3 may.
#[derive(Clone)]
pub struct SharedExecutor(pub Arc<std::sync::Mutex<Executor>>);

impl SharedExecutor {
    #[must_use]
    pub fn new(executor: Executor) -> Self {
        Self(Arc::new(std::sync::Mutex::new(executor)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn drain<F: Fn(&Event) -> bool>(rx: &mpsc::Receiver<Event>, want: F) -> Event {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let timeout = deadline.saturating_duration_since(std::time::Instant::now());
            let ev = rx
                .recv_timeout(timeout)
                .expect("expected an Event before deadline");
            if want(&ev) {
                return ev;
            }
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn dir_scan_emits_DirScanned_with_entries() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.txt"), b"x").unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(t.path().to_path_buf()).unwrap();

        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let id = ex.start_dir_scan(PanelSide::Left, dir.clone(), false, SortMode::ByName);

        let ev = drain(&rx, |e| {
            matches!(
                e,
                Event::Worker(
                    _,
                    WorkerMsg::DirScanned {
                        side: PanelSide::Left,
                        ..
                    }
                )
            )
        });
        match ev {
            Event::Worker(got, WorkerMsg::DirScanned { entries, .. }) => {
                assert_eq!(got, id);
                assert!(entries.iter().any(|e| e.name == "a.txt"));
            }
            _ => unreachable!(),
        }

        ex.join_all();
    }

    #[test]
    #[allow(non_snake_case)]
    fn dir_scan_emits_Failed_for_missing_path() {
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let _ = ex.start_dir_scan(
            PanelSide::Right,
            camino::Utf8PathBuf::from("/no-such-path-mx-exec-test"),
            false,
            SortMode::ByName,
        );
        let ev = drain(&rx, |e| {
            matches!(e, Event::Worker(_, WorkerMsg::Failed { .. }))
        });
        if let Event::Worker(_, WorkerMsg::Failed { errors }) = ev {
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].1, mx_core::errors::FsError::NotFound);
        }
        ex.join_all();
    }

    #[test]
    fn worker_ids_are_unique_and_monotonic() {
        let (tx, _rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let t = tempfile::tempdir().unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(t.path().to_path_buf()).unwrap();
        let a = ex.start_dir_scan(PanelSide::Left, dir.clone(), false, SortMode::ByName);
        let b = ex.start_dir_scan(PanelSide::Right, dir, false, SortMode::ByName);
        assert!(b.0 > a.0);
        ex.join_all();
    }
}
