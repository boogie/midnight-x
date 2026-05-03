//! Worker executor — the only `std::thread::spawn` site for app workers.
//! Phase 2 ships only directory scanning; Phase 3 adds copy/move/delete.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use camino::Utf8PathBuf;

use mx_core::command::OverwritePolicy;
use mx_core::event::{Event, WorkerId, WorkerMsg};
use mx_core::state::{PanelSide, SortMode};

use crate::dir_scan::scan;

pub struct Executor {
    tx: Sender<Event>,
    next_id: AtomicU64,
    workers: HashMap<WorkerId, WorkerHandle>,
}

struct WorkerHandle {
    join: JoinHandle<()>,
    cancel: Arc<AtomicBool>,
    /// Sender used to deliver an `OverwritePolicy` resolution to a worker
    /// paused on a `Conflict`. `None` for ops that never raise conflicts.
    /// Populated for copy / move workers in later Phase 3 tasks.
    #[allow(dead_code)]
    resume: Option<Sender<OverwritePolicy>>,
}

impl Executor {
    #[must_use]
    pub fn new(tx: Sender<Event>) -> Self {
        Self {
            tx,
            next_id: AtomicU64::new(1),
            workers: HashMap::new(),
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
        let cancel = Arc::new(AtomicBool::new(false));
        let join = thread::Builder::new()
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
        self.workers.insert(
            id,
            WorkerHandle {
                join,
                cancel,
                resume: None,
            },
        );
        id
    }

    /// Spawn a recursive delete worker.
    ///
    /// # Panics
    ///
    /// Panics if the OS refuses to spawn a thread.
    pub fn start_delete(&mut self, paths: Vec<Utf8PathBuf>) -> WorkerId {
        let id = self.alloc_id();
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let join = thread::Builder::new()
            .name(format!("mx-fs-del-{}", id.0))
            .spawn(move || {
                let mut errors = Vec::new();
                for p in &paths {
                    if cancel_for_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = tx.send(Event::Worker(
                        id,
                        WorkerMsg::Progress {
                            bytes_done: 0,
                            bytes_total: 0,
                            current_path: p.clone(),
                        },
                    ));
                    let report = crate::delete::delete_tree(p, &cancel_for_thread);
                    errors.extend(report.errors);
                }
                let msg = if errors.is_empty() {
                    WorkerMsg::Done
                } else {
                    WorkerMsg::Failed { errors }
                };
                let _ = tx.send(Event::Worker(id, msg));
            })
            .expect("worker thread cannot fail to spawn");
        self.workers.insert(
            id,
            WorkerHandle {
                join,
                cancel,
                resume: None,
            },
        );
        id
    }

    /// Spawn a recursive copy worker.
    ///
    /// # Panics
    ///
    /// Panics if the OS refuses to spawn a thread.
    pub fn start_copy(&mut self, src: Vec<Utf8PathBuf>, dst: Utf8PathBuf) -> WorkerId {
        let id = self.alloc_id();
        let tx_main = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let (resume_tx, resume_rx) = std::sync::mpsc::channel::<OverwritePolicy>();
        let join = thread::Builder::new()
            .name(format!("mx-fs-copy-{}", id.0))
            .spawn(move || {
                run_copy_worker(id, src, dst, tx_main, cancel_for_thread, resume_rx);
            })
            .expect("worker thread cannot fail to spawn");
        self.workers.insert(
            id,
            WorkerHandle {
                join,
                cancel,
                resume: Some(resume_tx),
            },
        );
        id
    }

    /// Resolve a conflict that a copy/move worker is waiting on.
    pub fn resolve_conflict(&self, id: WorkerId, policy: OverwritePolicy) {
        if let Some(w) = self.workers.get(&id) {
            if let Some(ref tx) = w.resume {
                let _ = tx.send(policy);
            }
        }
    }

    /// Cancel a running worker.
    pub fn cancel(&self, id: WorkerId) {
        if let Some(w) = self.workers.get(&id) {
            w.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Drop completed workers from the handle map. Called periodically by
    /// the main loop so the map doesn't grow unbounded.
    pub fn reap(&mut self) {
        self.workers.retain(|_, w| !w.join.is_finished());
    }

    /// Wait for all in-flight workers to finish. Used during shutdown.
    pub fn join_all(&mut self) {
        let workers = std::mem::take(&mut self.workers);
        for (_, w) in workers {
            let _ = w.join.join();
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.workers.len()
    }
}

#[allow(clippy::needless_pass_by_value)] // values are owned by the spawned thread
fn run_copy_worker(
    id: WorkerId,
    src_list: Vec<Utf8PathBuf>,
    dst_dir: Utf8PathBuf,
    tx_main: Sender<Event>,
    cancel: Arc<AtomicBool>,
    resume_rx: std::sync::mpsc::Receiver<OverwritePolicy>,
) {
    use std::sync::Mutex;
    let policy_state: Arc<Mutex<Option<OverwritePolicy>>> = Arc::new(Mutex::new(None));

    let progress = {
        let tx = tx_main.clone();
        move |path: &camino::Utf8Path, done: u64, total: u64| {
            let _ = tx.send(Event::Worker(
                id,
                WorkerMsg::Progress {
                    bytes_done: done,
                    bytes_total: total,
                    current_path: path.to_path_buf(),
                },
            ));
        }
    };

    let resume_rx = std::sync::Mutex::new(resume_rx);
    let handler = {
        let tx = tx_main.clone();
        let policy_state = Arc::clone(&policy_state);
        move |src: &camino::Utf8Path,
              dst: &camino::Utf8Path|
              -> crate::copy::OverwriteAction {
            if let Some(p) = *policy_state.lock().expect("policy_state poisoned") {
                return policy_to_action(p);
            }
            let _ = tx.send(Event::Worker(
                id,
                WorkerMsg::Conflict {
                    src: src.to_path_buf(),
                    dst: dst.to_path_buf(),
                    kind: detect_conflict_kind(src, dst),
                },
            ));
            let rx = resume_rx.lock().expect("resume_rx poisoned");
            let decision = rx.recv().unwrap_or(OverwritePolicy::Cancel);
            if matches!(
                decision,
                OverwritePolicy::YesAll | OverwritePolicy::NoAll | OverwritePolicy::Cancel
            ) {
                *policy_state.lock().expect("policy_state poisoned") = Some(decision);
            }
            policy_to_action(decision)
        }
    };

    let mut all_errors = Vec::new();
    for src in &src_list {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let dst = dst_dir.join(src.file_name().unwrap_or(""));
        let report = crate::copy::copy_tree(src, &dst, &cancel, &progress, &handler);
        all_errors.extend(report.errors);
    }
    let msg = if all_errors.is_empty() {
        WorkerMsg::Done
    } else {
        WorkerMsg::Failed { errors: all_errors }
    };
    let _ = tx_main.send(Event::Worker(id, msg));
}

fn policy_to_action(p: OverwritePolicy) -> crate::copy::OverwriteAction {
    match p {
        OverwritePolicy::Yes | OverwritePolicy::YesAll => crate::copy::OverwriteAction::Overwrite,
        OverwritePolicy::No | OverwritePolicy::NoAll => crate::copy::OverwriteAction::Skip,
        OverwritePolicy::Cancel => crate::copy::OverwriteAction::Cancel,
    }
}

fn detect_conflict_kind(
    src: &camino::Utf8Path,
    dst: &camino::Utf8Path,
) -> mx_core::event::ConflictKind {
    use mx_core::event::ConflictKind;
    let s = std::fs::metadata(src).ok();
    let d = std::fs::metadata(dst).ok();
    let s_dir = s.as_ref().is_some_and(std::fs::Metadata::is_dir);
    let d_dir = d.as_ref().is_some_and(std::fs::Metadata::is_dir);
    match (s_dir, d_dir) {
        (false, false) => ConflictKind::FileOverFile,
        (false, true) => ConflictKind::FileOverDir,
        (true, false) => ConflictKind::DirOverFile,
        (true, true) => ConflictKind::DirOverDir,
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
    fn start_delete_removes_paths_and_emits_done() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        std::fs::write(t.path().join("b"), b"y").unwrap();
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let p_a = camino::Utf8PathBuf::from_path_buf(t.path().join("a")).unwrap();
        let p_b = camino::Utf8PathBuf::from_path_buf(t.path().join("b")).unwrap();
        let _id = ex.start_delete(vec![p_a, p_b]);
        let _ = drain(&rx, |e| matches!(e, Event::Worker(_, WorkerMsg::Done)));
        ex.join_all();
        assert!(!t.path().join("a").exists());
        assert!(!t.path().join("b").exists());
    }

    #[test]
    fn start_copy_succeeds_without_conflict() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(t.path().join("dst")).unwrap();
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let src = camino::Utf8PathBuf::from_path_buf(t.path().join("a.txt")).unwrap();
        let dst = camino::Utf8PathBuf::from_path_buf(t.path().join("dst")).unwrap();
        let _id = ex.start_copy(vec![src], dst);
        let _ = drain(&rx, |e| matches!(e, Event::Worker(_, WorkerMsg::Done)));
        ex.join_all();
        assert_eq!(std::fs::read(t.path().join("dst/a.txt")).unwrap(), b"hello");
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
