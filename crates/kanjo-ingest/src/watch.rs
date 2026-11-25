//! Live updates: a recursive notify watcher on `<claude>/projects`, debounced,
//! with a periodic safety scan for missed events.

use crate::{Ingestor, ScanReport};
use kanjo_core::{Error, Result};
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(100);
const QUIET: Duration = Duration::from_millis(300);
const MAX_DELAY: Duration = Duration::from_secs(2);
const SAFETY_SCAN: Duration = Duration::from_secs(60);
const SAFETY_SCAN_UNWATCHED: Duration = Duration::from_secs(5);

/// Keeps the watcher alive; dropping it stops the watcher and joins its thread.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

type Events = mpsc::Sender<notify::Result<Event>>;

fn attach(dir: &Path, tx: &Events) -> Result<RecommendedWatcher> {
    let tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |ev| {
        let _ = tx.send(ev);
    })
    .map_err(|e| Error::Other(format!("file watcher: {e}")))?;
    watcher.watch(dir, RecursiveMode::Recursive).map_err(|e| Error::Other(format!("file watcher: {e}")))?;
    Ok(watcher)
}

/// Our own reads raise access events; only writes, creations, removals and renames matter.
fn relevant(ev: &Event) -> bool {
    match ev.kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}

impl Ingestor {
    /// Starts watching `<claude>/projects`. Each debounced burst of changes
    /// runs an incremental [`Ingestor::scan`]; `on_change` gets the report
    /// when files were read. A scan also runs every minute regardless, and the
    /// watcher attaches itself once the directory appears.
    pub fn watch(self: &Arc<Self>, on_change: Box<dyn Fn(ScanReport) + Send + Sync>) -> Result<WatchHandle> {
        let dir = self.paths.claude_projects();
        let (tx, rx) = mpsc::channel();
        let mut watcher = if dir.is_dir() { Some(attach(&dir, &tx)?) } else { None };
        let stop = Arc::new(AtomicBool::new(false));
        let ingestor = Arc::clone(self);
        let flag = Arc::clone(&stop);
        ingestor.set_watcher_running(true);
        let thread = std::thread::Builder::new()
            .name("kanjo-watch".into())
            .spawn(move || {
                let mut first: Option<Instant> = None;
                let mut last = Instant::now();
                let mut last_scan = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    if watcher.is_none() && dir.is_dir() {
                        match attach(&dir, &tx) {
                            Ok(w) => {
                                watcher = Some(w);
                                first.get_or_insert_with(Instant::now);
                            }
                            Err(e) => tracing::debug!(error = %e, "watcher not attached yet"),
                        }
                    }
                    match rx.recv_timeout(TICK) {
                        Ok(Ok(ev)) if relevant(&ev) => {
                            last = Instant::now();
                            first.get_or_insert(last);
                        }
                        Ok(Ok(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "file watcher error; rescanning");
                            last = Instant::now();
                            first.get_or_insert(last);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    let now = Instant::now();
                    let burst_done = first.is_some_and(|f| now - last >= QUIET || now - f >= MAX_DELAY);
                    let safety = if watcher.is_some() { SAFETY_SCAN } else { SAFETY_SCAN_UNWATCHED };
                    if burst_done || now - last_scan >= safety {
                        first = None;
                        last_scan = now;
                        match ingestor.scan() {
                            Ok(report) if report.files > 0 => on_change(report),
                            Ok(_) => {}
                            Err(e) => tracing::warn!(error = %e, "incremental scan failed"),
                        }
                    }
                }
                drop(watcher);
                ingestor.set_watcher_running(false);
            })
            .map_err(Error::Io)?;
        Ok(WatchHandle { stop, thread: Some(thread) })
    }
}
