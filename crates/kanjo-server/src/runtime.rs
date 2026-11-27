//! Background work of a running server: the first import, the transcript
//! watcher, spool drain, config fingerprints, alert evaluation and retention.
//! Every database call runs on the blocking pool. Demo mode starts none of it.

use crate::state::{lock, AppState, Inner};
use kanjo_core::api::{IngestStatus, LiveEvent};
use kanjo_core::settings::Settings;
use kanjo_core::Result;
use kanjo_ingest::{ScanReport, WatchHandle};
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

const SPOOL_EVERY: Duration = Duration::from_secs(2);
const CONFIG_EVERY: Duration = Duration::from_secs(60);
const ALERTS_EVERY: Duration = Duration::from_secs(60);
const ALERT_DEBOUNCE: Duration = Duration::from_secs(3);
const RETENTION_EVERY: Duration = Duration::from_secs(24 * 3600);
const FALLBACK_SCAN_EVERY: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(5);

pub(crate) struct Runtime {
    tasks: Vec<JoinHandle<()>>,
    watcher: Arc<Mutex<Option<WatchHandle>>>,
}

pub(crate) fn start(state: &AppState) -> Runtime {
    let watcher = Arc::new(Mutex::new(None));
    let mut tasks = Vec::new();
    if state.inner.ingestor.is_some() {
        tasks.push(tokio::spawn(import_then_watch(state.clone(), watcher.clone())));
        tasks.push(tokio::spawn(periodic(state.clone(), SPOOL_EVERY, drain_spool)));
        tasks.push(tokio::spawn(periodic(state.clone(), CONFIG_EVERY, check_config)));
        tasks.push(tokio::spawn(periodic(state.clone(), RETENTION_EVERY, retention)));
        tasks.push(tokio::spawn(alert_loop(state.clone())));
    }
    Runtime { tasks, watcher }
}

impl Runtime {
    /// Waits briefly for the loops to notice the stop signal, then stops the watcher.
    pub(crate) async fn stop(self) {
        for task in self.tasks {
            let abort = task.abort_handle();
            if tokio::time::timeout(STOP_GRACE, task).await.is_err() {
                abort.abort();
            }
        }
        let handle = lock(&self.watcher).take();
        if let Some(h) = handle {
            let join = tokio::task::spawn_blocking(move || drop(h));
            if tokio::time::timeout(STOP_GRACE, join).await.is_err() {
                tracing::warn!("the transcript watcher did not stop in time");
            }
        }
    }
}

fn stopping(inner: &Inner) -> bool {
    *inner.shutdown.borrow()
}

/// Runs `step` now and then every `period` until shutdown.
async fn periodic<F, Fut>(state: AppState, period: Duration, step: F)
where
    F: Fn(AppState) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut stop = state.inner.shutdown.subscribe();
    let mut tick = tokio::time::interval(period);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        if *stop.borrow() {
            return;
        }
        tokio::select! {
            _ = tick.tick() => step(state.clone()).await,
            changed = stop.changed() => if changed.is_err() { return },
        }
    }
}

/// Runs `f` on the blocking pool and logs a failure under `what`.
async fn blocking<T, F>(state: &AppState, what: &'static str, f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce(&Arc<Inner>) -> Result<T> + Send + 'static,
{
    let inner = state.inner.clone();
    match tokio::task::spawn_blocking(move || f(&inner)).await {
        Ok(Ok(v)) => Some(v),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "{what} failed");
            None
        }
        Err(e) => {
            tracing::error!(error = %e, "{what} task failed");
            None
        }
    }
}

fn usage_event(inner: &Inner, report: ScanReport) {
    if report.inserted + report.updated == 0 {
        return;
    }
    inner.publish(LiveEvent::Usage { requests: report.inserted, cost_usd: report.cost_usd, sessions: report.sessions });
    inner.alert_kick.notify_one();
}

/// Re-imports all transcript history unless an import is running. Progress
/// goes to the event stream as `LiveEvent::Ingest`.
pub(crate) fn spawn_rescan(state: &AppState) -> Option<JoinHandle<()>> {
    if state.inner.ingestor.is_none() || state.inner.importing.swap(true, Ordering::SeqCst) {
        return None;
    }
    Some(tokio::spawn(import(state.clone(), true)))
}

/// Reads the titles of transcripts imported while titles were off (see
/// `Ingestor::refresh_titles`) and tells open pages to refetch.
pub(crate) fn spawn_title_refresh(state: &AppState) {
    let state = state.clone();
    tokio::spawn(async move {
        let sessions = blocking(&state, "session title refresh", |inner| match &inner.ingestor {
            Some(ing) => ing.refresh_titles(),
            None => Ok(Vec::new()),
        })
        .await
        .unwrap_or_default();
        if !sessions.is_empty() {
            state.publish(LiveEvent::Usage { requests: 0, cost_usd: 0.0, sessions });
        }
    });
}

async fn import(state: AppState, from_scratch: bool) {
    let done = blocking(&state, "transcript import", move |inner| {
        let Some(ing) = inner.ingestor.clone() else { return Ok(None) };
        if from_scratch {
            ing.rescan_from_scratch()?;
        }
        let before = ing.status().requests_stored;
        let progress_to = inner.clone();
        let progress = move |st: &IngestStatus| progress_to.publish(LiveEvent::Ingest(st.clone()));
        let st = ing.backfill(&progress)?;
        Ok(Some(st.requests_stored - before))
    })
    .await;
    state.inner.importing.store(false, Ordering::SeqCst);
    if let Some(Some(added)) = done {
        tracing::info!(requests = added, "transcript import finished");
        if added > 0 {
            state.publish(LiveEvent::Usage { requests: added, cost_usd: 0.0, sessions: Vec::new() });
        }
        state.inner.alert_kick.notify_one();
    }
}

async fn import_then_watch(state: AppState, slot: Arc<Mutex<Option<WatchHandle>>>) {
    if state.inner.importing.swap(true, Ordering::SeqCst) {
        return;
    }
    import(state.clone(), false).await;
    if stopping(&state.inner) {
        return;
    }
    let handle = blocking(&state, "transcript watcher", |inner| {
        let Some(ing) = inner.ingestor.clone() else { return Ok(None) };
        let to = inner.clone();
        ing.watch(Box::new(move |report| usage_event(&to, report))).map(Some)
    })
    .await;
    match handle {
        Some(Some(h)) => {
            if stopping(&state.inner) {
                let _ = tokio::task::spawn_blocking(move || drop(h)).await;
            } else {
                *lock(&slot) = Some(h);
                state.publish(LiveEvent::Ingest(state.inner.ingest_status()));
            }
        }
        _ => {
            tracing::warn!("no file watcher; scanning transcripts every {} s instead", FALLBACK_SCAN_EVERY.as_secs());
            periodic(state, FALLBACK_SCAN_EVERY, scan).await;
        }
    }
}

async fn scan(state: AppState) {
    if let Some(report) = blocking(&state, "transcript scan", |inner| match &inner.ingestor {
        Some(ing) => ing.scan().map(Some),
        None => Ok(None),
    })
    .await
    .flatten()
    {
        usage_event(&state.inner, report);
    }
}

async fn drain_spool(state: AppState) {
    blocking(&state, "spool drain", |inner| {
        let Some(ing) = &inner.ingestor else { return Ok(()) };
        let report = ing.drain_spool()?;
        for session_id in report.sessions {
            inner.publish(LiveEvent::Status { session_id });
        }
        if report.limits_changed {
            let limits = kanjo_analytics::limits(&inner.store, &inner.ctx())?;
            inner.publish(LiveEvent::Limits(limits));
            inner.alert_kick.notify_one();
        }
        Ok(())
    })
    .await;
}

async fn check_config(state: AppState) {
    blocking(&state, "config fingerprint check", |inner| {
        if let Some(ing) = &inner.ingestor {
            let changed = ing.check_config_files()?;
            if changed > 0 {
                tracing::debug!(changed, "prompt-shaping files changed");
            }
        }
        Ok(())
    })
    .await;
}

async fn retention(state: AppState) {
    blocking(&state, "retention", |inner| {
        let settings = Settings::load(&inner.store)?;
        let report = kanjo_ops::retention::apply(&inner.store, &settings.retention, kanjo_core::time::now_ms())?;
        let removed = report.requests
            + report.sessions
            + report.aggregates
            + report.limit_samples
            + report.alerts
            + report.audit_entries
            + report.quarantine;
        if removed > 0 {
            tracing::info!(?report, "retention removed expired rows");
            let detail = serde_json::json!({ "report": report, "settings": settings.retention, "user": kanjo_ops::audit::actor() });
            let now = kanjo_core::time::now_ms();
            inner.store.write(|tx| {
                kanjo_ops::audit::record_tx(tx, now, crate::handlers::ACTOR_SERVICE, "retention.apply", "retention", &detail)
            })?;
            inner.publish(LiveEvent::Usage { requests: 0, cost_usd: 0.0, sessions: Vec::new() });
        }
        Ok(())
    })
    .await;
}

/// Evaluates the alert rules every minute and shortly after new usage, a new
/// limit reading or a budget change (debounced); delivers and streams new alerts.
/// Skipped while an import runs, so rules never judge half-imported history;
/// the import kicks an evaluation when it ends.
async fn alert_loop(state: AppState) {
    let mut stop = state.inner.shutdown.subscribe();
    let mut tick = tokio::time::interval(ALERTS_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        if *stop.borrow() {
            return;
        }
        tokio::select! {
            _ = tick.tick() => {}
            _ = state.inner.alert_kick.notified() => {
                tokio::time::sleep(ALERT_DEBOUNCE).await;
                let _ = futures_util::FutureExt::now_or_never(state.inner.alert_kick.notified());
            }
            changed = stop.changed() => if changed.is_err() { return },
        }
        if stopping(&state.inner) {
            return;
        }
        if state.inner.importing.load(Ordering::SeqCst) {
            continue;
        }
        evaluate_alerts(&state).await;
    }
}

async fn evaluate_alerts(state: &AppState) {
    blocking(state, "alert evaluation", |inner| {
        let created = kanjo_alerts::evaluate(&inner.store, &inner.ctx())?;
        if created.is_empty() {
            return Ok(());
        }
        let settings = Settings::load(&inner.store)?;
        for alert in created {
            kanjo_alerts::notify::deliver(&alert, &settings.alerts);
            inner.publish(LiveEvent::Alert(alert));
        }
        Ok(())
    })
    .await;
}
