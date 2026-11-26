use crate::error::ApiErr;
use crate::security::Limiter;
use crate::ServeOptions;
use kanjo_core::api::{IngestStatus, KeySource, LiveEvent};
use kanjo_core::config::{Config, EncryptionMode};
use kanjo_core::paths::Paths;
use kanjo_core::settings::Settings;
use kanjo_core::{Ctx, Error, Result, Store};
use kanjo_ingest::Ingestor;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use tokio::sync::{broadcast, watch, Notify};

const DEMO_SEED: u64 = 42;
const EVENT_BUFFER: usize = 256;

/// Everything a request handler needs. Cheap to clone; build it with
/// [`AppState::open`], then hand it to [`crate::router`].
#[derive(Clone)]
pub struct AppState {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub store: Arc<Store>,
    pub key_source: KeySource,
    /// In demo mode `claude_dir` points to a directory under `home` that does
    /// not exist, so no engine call can reach the real Claude directory.
    pub paths: Paths,
    pub config: RwLock<Config>,
    pub demo: bool,
    pub ingestor: Option<Arc<Ingestor>>,
    /// `None` when authentication is off (loopback with `auth = "none"`).
    pub token: Option<String>,
    pub session: Option<String>,
    pub tls: bool,
    pub port: AtomicU16,
    pub week_start: AtomicU8,
    pub events: broadcast::Sender<LiveEvent>,
    pub shutdown: watch::Sender<bool>,
    pub alert_kick: Notify,
    /// A transcript import (first start or rescan) is running.
    pub importing: AtomicBool,
    pub limiter: Mutex<Limiter>,
    pub login_codes: Mutex<Vec<(String, std::time::Instant)>>,
    /// Random per start; named in `server.json` and in the login codes the command line writes.
    pub instance: String,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    /// Opens the store (and, in demo mode, fills it with synthetic data and runs
    /// the alert rules once) without starting any background loop. The port is
    /// taken from the configuration; call [`AppState::set_port`] once the real
    /// port is known.
    pub async fn open(opts: &ServeOptions) -> Result<AppState> {
        let opts = opts.clone();
        tokio::task::spawn_blocking(move || AppState::open_blocking(&opts))
            .await
            .map_err(|e| Error::Other(format!("start-up task failed: {e}")))?
    }

    fn open_blocking(opts: &ServeOptions) -> Result<AppState> {
        let mut config = opts.config.clone();
        let (paths, store, key_source, ingestor) = if opts.demo {
            config.storage.encryption = EncryptionMode::Off;
            let paths = Paths::at(&opts.paths.home, opts.paths.home.join("demo-claude-dir"));
            let (store, key_source) = kanjo_ops::open_store_with(&paths, &config, None)?;
            let now = kanjo_core::time::now_ms();
            kanjo_ops::demo::populate(&store, now, opts.demo_days.unwrap_or(crate::DEMO_DAYS).max(1), DEMO_SEED)?;
            let settings = Settings::load(&store)?;
            let ctx = Ctx { week_start: settings.week_start.min(6), ..Ctx::system() };
            let created = kanjo_alerts::evaluate(&store, &ctx)?;
            tracing::info!(alerts = created.len(), "demo data ready");
            (paths, Arc::new(store), key_source, None)
        } else {
            let paths = opts.paths.clone();
            let (store, key_source) = kanjo_ops::open_store(&paths, &config)?;
            let store = Arc::new(store);
            let ingestor = Arc::new(Ingestor::new(store.clone(), paths.clone())?);
            (paths, store, key_source, Some(ingestor))
        };
        let token = if crate::auth_enabled(&config) { Some(kanjo_ops::token::load_or_create(&paths)?) } else { None };
        let session = token.as_deref().map(crate::security::session_value);
        let settings = Settings::load(&store)?;
        let week_start = settings.week_start.min(6);
        let tls = config.server.tls_cert.is_some() && config.server.tls_key.is_some();
        let port = config.server.port;
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (shutdown, _) = watch::channel(false);
        let state = AppState {
            inner: Arc::new(Inner {
                store,
                key_source,
                paths,
                config: RwLock::new(config),
                demo: opts.demo,
                ingestor,
                token,
                session,
                tls,
                port: AtomicU16::new(port),
                week_start: AtomicU8::new(week_start),
                events,
                shutdown,
                alert_kick: Notify::new(),
                importing: AtomicBool::new(false),
                limiter: Mutex::new(Limiter::default()),
                login_codes: Mutex::new(Vec::new()),
                instance: kanjo_setup::server_file::random_hex(16)?,
            }),
        };
        if let Err(e) = state.inner.apply_settings(&settings) {
            tracing::warn!(error = %e, "could not bring config.toml in line with the privacy settings");
        }
        Ok(state)
    }

    /// The port the Host and Origin checks expect.
    pub fn set_port(&self, port: u16) {
        self.inner.port.store(port, Ordering::Relaxed);
    }

    pub fn port(&self) -> u16 {
        self.inner.port.load(Ordering::Relaxed)
    }

    /// Sends an event to every open `/api/v1/events` stream.
    pub fn publish(&self, event: LiveEvent) {
        let _ = self.inner.events.send(event);
    }

    pub fn is_demo(&self) -> bool {
        self.inner.demo
    }

    /// Ends open event streams and asks the background loops to stop.
    pub(crate) fn begin_shutdown(&self) {
        self.inner.shutdown.send_replace(true);
    }

    /// Runs `f` on the blocking pool with a fresh `Ctx`.
    pub(crate) async fn db<T, F>(&self, f: F) -> std::result::Result<T, ApiErr>
    where
        T: Send + 'static,
        F: FnOnce(&Inner, Ctx) -> std::result::Result<T, ApiErr> + Send + 'static,
    {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let ctx = inner.ctx();
            f(&inner, ctx)
        })
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "blocking task failed");
            ApiErr::internal()
        })?
    }
}

impl Inner {
    pub fn ctx(&self) -> Ctx {
        Ctx { week_start: self.week_start.load(Ordering::Relaxed), ..Ctx::system() }
    }

    pub fn ingest_status(&self) -> IngestStatus {
        self.ingestor.as_ref().map(|i| i.status()).unwrap_or_default()
    }

    pub fn config(&self) -> Config {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn publish(&self, event: LiveEvent) {
        let _ = self.events.send(event);
    }

    /// Applies saved settings to the running server: the week start, the
    /// ingestor's title flag and the statusline's `capture_titles` in
    /// config.toml (the fast path never opens the database). Demo mode leaves
    /// config.toml alone.
    pub fn apply_settings(&self, settings: &Settings) -> Result<()> {
        self.week_start.store(settings.week_start.min(6), Ordering::Relaxed);
        let titles = settings.privacy.session_titles;
        if let Some(i) = &self.ingestor {
            i.set_store_titles(titles);
        }
        if self.demo {
            return Ok(());
        }
        let mut disk = Config::load(&self.paths)?;
        if disk.statusline.capture_titles != titles {
            disk.statusline.capture_titles = titles;
            disk.save(&self.paths)?;
        }
        self.config.write().unwrap_or_else(|e| e.into_inner()).statusline.capture_titles = titles;
        Ok(())
    }
}
