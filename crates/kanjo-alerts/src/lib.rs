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
use render::Style;
use std::collections::HashMap;

/// Language code for texts rendered on the server in the settings language. `Auto`
/// follows the operating system's locale (LC_ALL, LC_MESSAGES, LANG): Polish for a
/// Polish locale, English otherwise.
pub fn settings_lang(settings: &Settings) -> &'static str {
    match settings.language {
        Language::Pl => "pl",
        Language::En => "en",
        Language::Auto => locale_lang(["LC_ALL", "LC_MESSAGES", "LANG"].map(|v| std::env::var(v).ok())),
    }
}

fn locale_lang(vars: [Option<String>; 3]) -> &'static str {
    match vars.into_iter().flatten().find(|v| !v.is_empty()) {
        Some(v) if v.to_ascii_lowercase().starts_with("pl") => "pl",
        _ => "en",
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
    let mut created = store.write(|tx| stored::insert(tx, ctx.now, &candidates))?;
    let redact = settings.privacy.redact_paths;
    store.read(|c| {
        let mut labels = HashMap::new();
        created.iter_mut().try_for_each(|a| stored::refresh_labels(c, &mut a.params, redact, &mut labels))
    })?;
    let style = Style { lang: settings_lang(&settings), currency: &settings.currency, now: Some(ctx.now) };
    Ok(created.into_iter().map(|a| a.render(&style)).collect())
}

/// Stored alerts, newest first. Honours `q.limit` (default 50), `q.offset`,
/// `q.q == "open"` (unacknowledged only) and `q.lang` (default: the settings language).
/// `total` counts the alerts matching the filter.
pub fn list(store: &Store, ctx: &Ctx, q: &Query) -> Result<AlertList> {
    let settings = Settings::load(store)?;
    let lang = q.lang.clone().unwrap_or_else(|| settings_lang(&settings).to_string());
    let page = store.read(|c| stored::page(c, q, settings.privacy.redact_paths))?;
    let style = Style { lang: &lang, currency: &settings.currency, now: Some(ctx.now) };
    Ok(AlertList {
        items: page.items.into_iter().map(|a| a.render(&style)).collect(),
        total: page.total,
        unacknowledged: page.unacknowledged,
    })
}

/// Acknowledges one alert, or every open alert when `id` is `None`. Returns the
/// number acknowledged; `NotFound` for an id that does not exist.
pub fn acknowledge(store: &Store, ctx: &Ctx, id: Option<i64>) -> Result<i64> {
    store.write(|tx| stored::acknowledge(tx, ctx.now, id))
}

#[cfg(test)]
mod tests {
    use super::locale_lang;

    #[test]
    fn auto_language_follows_the_os_locale() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(locale_lang([None, None, s("pl_PL.UTF-8")]), "pl");
        assert_eq!(locale_lang([s(""), s("pl_PL"), s("en_US.UTF-8")]), "pl");
        assert_eq!(locale_lang([s("C"), None, s("pl_PL.UTF-8")]), "en");
        assert_eq!(locale_lang([None, None, s("de_DE.UTF-8")]), "en");
        assert_eq!(locale_lang([None, None, None]), "en");
    }
}
