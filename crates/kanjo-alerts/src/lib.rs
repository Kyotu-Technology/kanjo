//! kanjo-alerts: alert rules, budgets, cache and anomaly insights, and local
//! notification delivery.
//!
//! Limit rules read through `kanjo_core::limits` (readings and the shared
//! forecast), so alerts agree with the dashboard. Every alert is stored with
//! the raw values behind it; titles and messages are rendered when the alert
//! is read, in the language asked for. All thresholds live in [`thresholds`].

mod budgets;
mod detect;
mod insights;
pub mod notify;
mod render;
mod rules;
mod stored;
pub mod thresholds;

pub use budgets::{budgets, create_budget, delete_budget, update_budget};
pub use insights::insights;
pub use render::render;

use kanjo_core::api::{Alert, AlertList, Query};
use kanjo_core::settings::{Language, Settings};
use kanjo_core::{Ctx, Result, Store};

/// Language code for rendering in the settings language; `Auto` renders English.
pub fn settings_lang(settings: &Settings) -> &'static str {
    match settings.language {
        Language::Pl => "pl",
        Language::En | Language::Auto => "en",
    }
}

/// Runs every rule enabled in `Settings.alerts` and stores the alerts that are new
/// (one per rule and window, guarded by the unique dedup key). Returns only the
/// newly created alerts, rendered in the settings language. Delivery is left to the
/// caller ([`notify::deliver`]).
pub fn evaluate(store: &Store, ctx: &Ctx) -> Result<Vec<Alert>> {
    let settings = Settings::load(store)?;
    let candidates = store.read(|c| rules::candidates(c, ctx, &settings))?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let created = store.write(|tx| stored::insert(tx, ctx.now, &candidates))?;
    let lang = settings_lang(&settings);
    Ok(created.into_iter().map(|a| a.render(lang)).collect())
}

/// Stored alerts, newest first. Honours `q.limit` (default 50), `q.offset`,
/// `q.q == "open"` (unacknowledged only) and `q.lang` (default: the settings language).
/// `total` counts the alerts matching the filter.
pub fn list(store: &Store, ctx: &Ctx, q: &Query) -> Result<AlertList> {
    let _ = ctx;
    let settings = Settings::load(store)?;
    let lang = q.lang.clone().unwrap_or_else(|| settings_lang(&settings).to_string());
    let page = store.read(|c| stored::page(c, q, settings.privacy.redact_paths))?;
    Ok(AlertList {
        items: page.items.into_iter().map(|a| a.render(&lang)).collect(),
        total: page.total,
        unacknowledged: page.unacknowledged,
    })
}

/// Acknowledges one alert, or every open alert when `id` is `None`. Returns the
/// number acknowledged; `NotFound` for an id that does not exist.
pub fn acknowledge(store: &Store, ctx: &Ctx, id: Option<i64>) -> Result<i64> {
    store.write(|tx| stored::acknowledge(tx, ctx.now, id))
}
