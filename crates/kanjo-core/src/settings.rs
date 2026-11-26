//! User-facing settings, stored in the database and edited from the UI.

use crate::{Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use ts_rs::TS;

pub fn get<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let raw: Option<String> = conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?;
    Ok(match raw {
        Some(text) => Some(serde_json::from_str(&text)?),
        None => None,
    })
}

pub fn set<T: Serialize>(tx: &Connection, key: &str, value: &T) -> Result<()> {
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, serde_json::to_string(value)?, crate::time::now_ms()],
    )?;
    Ok(())
}

const KEY: &str = "app";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct Settings {
    pub onboarding_done: bool,
    pub language: Language,
    pub theme: Theme,
    /// 0 = Monday ... 6 = Sunday.
    pub week_start: u8,
    pub plan: PlanSettings,
    pub currency: CurrencySettings,
    pub alerts: AlertSettings,
    pub privacy: PrivacySettings,
    pub retention: RetentionSettings,
}

impl Settings {
    pub fn load(store: &Store) -> Result<Settings> {
        store.read(|c| Ok(get(c, KEY)?.unwrap_or_default()))
    }

    pub fn save(&self, store: &Store) -> Result<()> {
        store.write(|tx| set(tx, KEY, self))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Follow the browser / OS locale.
    #[default]
    Auto,
    En,
    Pl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// Not chosen yet; plan comparisons are hidden.
    #[default]
    Unknown,
    Pro,
    /// Five times Pro's usage every five hours and every week.
    Max,
    /// Ten times Pro's usage every week and no five-hour limit once the account is
    /// verified. Formerly Max 20x.
    Ultra,
    /// Team plan, Standard seat.
    Team,
    /// Team plan, Premium seat.
    TeamPremium,
    Enterprise,
    /// Pay-as-you-go API or cloud provider billing.
    Api,
    Custom,
}

impl PlanKind {
    /// List price per seat and month in USD, as a starting value the user can
    /// override. `None` where there is no fixed price.
    pub fn default_monthly_usd(&self) -> Option<f64> {
        match self {
            PlanKind::Pro => Some(20.0),
            PlanKind::Max => Some(100.0),
            PlanKind::Ultra => Some(200.0),
            PlanKind::Team => Some(25.0),
            PlanKind::TeamPremium => Some(125.0),
            PlanKind::Unknown | PlanKind::Enterprise | PlanKind::Api | PlanKind::Custom => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct PlanSettings {
    #[serde(with = "plan_kind_compat")]
    #[ts(as = "PlanKind")]
    pub kind: PlanKind,
    /// What is actually paid per month, in USD. Falls back to the plan's list price.
    pub monthly_usd: Option<f64>,
}

/// Reads the names 0.1.0 saved before the rename: `max5x` is Max, `max20x` is Ultra.
mod plan_kind_compat {
    use super::PlanKind;
    use serde::de::IntoDeserializer;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(kind: &PlanKind, s: S) -> Result<S::Ok, S::Error> {
        kind.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PlanKind, D::Error> {
        let name = String::deserialize(d)?;
        let name = match name.as_str() {
            "max5x" => "max",
            "max20x" => "ultra",
            other => other,
        };
        PlanKind::deserialize(IntoDeserializer::<D::Error>::into_deserializer(name))
    }
}

impl PlanSettings {
    pub fn effective_monthly_usd(&self) -> Option<f64> {
        self.monthly_usd.or_else(|| self.kind.default_monthly_usd())
    }
}

/// Display currency. Prices are always computed in USD; a manual rate converts
/// them for display because Kanjo never fetches exchange rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct CurrencySettings {
    pub code: String,
    pub usd_rate: f64,
}

impl Default for CurrencySettings {
    fn default() -> Self {
        CurrencySettings { code: "USD".into(), usd_rate: 1.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct AlertSettings {
    /// Percent-of-limit thresholds for the 5-hour and 7-day windows.
    pub limit_thresholds: Vec<f64>,
    pub forecast: bool,
    pub budgets: bool,
    pub anomalies: bool,
    pub cache: bool,
    pub context: bool,
    /// Context usage (percent) that triggers the long-context warning.
    pub context_threshold: f64,
    pub desktop_notifications: bool,
    pub terminal_bell: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        AlertSettings {
            limit_thresholds: vec![80.0, 90.0, 95.0],
            forecast: true,
            budgets: true,
            anomalies: true,
            cache: true,
            context: true,
            context_threshold: 85.0,
            desktop_notifications: true,
            terminal_bell: false,
        }
    }
}

/// Privacy defaults are the restrictive ones; each relaxation is an explicit choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct PrivacySettings {
    /// Store session titles (they summarise conversation content).
    pub session_titles: bool,
    /// Show project aliases instead of names and paths in the UI and in exports.
    pub redact_paths: bool,
    /// Allow the read-only transcript viewer. Every view is written to the audit log.
    pub conversation_viewer: bool,
}

/// Days to keep each data class; 0 keeps it indefinitely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct RetentionSettings {
    /// Per-request rows. Hourly aggregates and session totals survive their removal.
    pub requests_days: u32,
    pub aggregates_days: u32,
    pub limit_samples_days: u32,
    pub alerts_days: u32,
    pub audit_days: u32,
    pub quarantine_days: u32,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        RetentionSettings {
            requests_days: 400,
            aggregates_days: 0,
            limit_samples_days: 400,
            alerts_days: 180,
            audit_days: 400,
            quarantine_days: 90,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_restrictive_defaults() {
        let t = crate::testkit::TestStore::new();
        let mut s = Settings::load(&t.store).unwrap();
        assert!(!s.privacy.conversation_viewer && !s.privacy.session_titles);
        s.plan.kind = PlanKind::Ultra;
        s.save(&t.store).unwrap();
        let again = Settings::load(&t.store).unwrap();
        assert_eq!(again, s);
        assert_eq!(again.plan.effective_monthly_usd(), Some(200.0));
    }

    #[test]
    fn plan_names_saved_by_0_1_0_still_load() {
        let t = crate::testkit::TestStore::new();
        for (old, kind, new) in [("max5x", PlanKind::Max, "max"), ("max20x", PlanKind::Ultra, "ultra")] {
            t.store.write(|tx| set(tx, KEY, &serde_json::json!({ "plan": { "kind": old, "monthly_usd": null } }))).unwrap();
            let s = Settings::load(&t.store).unwrap();
            assert_eq!(s.plan.kind, kind);
            assert_eq!(serde_json::to_value(&s.plan).unwrap()["kind"], new);
        }
    }
}
