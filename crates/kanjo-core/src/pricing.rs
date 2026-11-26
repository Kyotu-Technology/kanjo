use crate::model::TokenCounts;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../../../assets/pricing.json");

/// Price of one model, USD per token. Field names follow LiteLLM so entries
/// from its price file can be pasted into a Kanjo price file unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_token_cost: Option<f64>,
    /// 5-minute cache write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost: Option<f64>,
    /// 1-hour cache write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost_above_1hr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTable {
    pub version: String,
    pub source: String,
    #[serde(default = "usd")]
    pub currency: String,
    pub models: BTreeMap<String, ModelPrice>,
}

fn usd() -> String {
    "USD".into()
}

impl PriceTable {
    /// The table compiled into the binary. Kanjo never downloads prices.
    pub fn embedded() -> &'static PriceTable {
        static TABLE: OnceLock<PriceTable> = OnceLock::new();
        TABLE.get_or_init(|| PriceTable::from_json(EMBEDDED).expect("embedded price table is valid"))
    }

    pub fn from_json(text: &str) -> Result<PriceTable> {
        let table: PriceTable = serde_json::from_str(text)?;
        if table.currency != "USD" {
            return Err(Error::Invalid(format!("unsupported price currency {}", table.currency)));
        }
        for (name, price) in &table.models {
            let values = [
                Some(price.input_cost_per_token),
                Some(price.output_cost_per_token),
                price.cache_read_input_token_cost,
                price.cache_creation_input_token_cost,
                price.cache_creation_input_token_cost_above_1hr,
            ];
            if values.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) {
                return Err(Error::Invalid(format!("invalid price for model {name}")));
            }
        }
        Ok(table)
    }

    /// Exact match first, then the name without a `[1m]`-style suffix, then
    /// without a trailing `-YYYYMMDD` date. Unknown models stay unknown: they
    /// are reported as unpriced, never billed at zero or at a guessed rate.
    pub fn lookup(&self, model: &str) -> Option<&ModelPrice> {
        if let Some(p) = self.models.get(model) {
            return Some(p);
        }
        let base = model.split('[').next().unwrap_or(model);
        if let Some(p) = self.models.get(base) {
            return Some(p);
        }
        let undated = strip_date(base);
        if undated != base {
            return self.models.get(undated);
        }
        None
    }

    /// API-equivalent list price of one response, or `None` for an unpriced model.
    pub fn cost(&self, model: &str, t: &TokenCounts) -> Option<f64> {
        let p = self.lookup(model)?;
        let cache_read = p.cache_read_input_token_cost.unwrap_or(p.input_cost_per_token * 0.1);
        let write_5m = p.cache_creation_input_token_cost.unwrap_or(p.input_cost_per_token * 1.25);
        let write_1h = p.cache_creation_input_token_cost_above_1hr.unwrap_or(p.input_cost_per_token * 2.0);
        Some(
            t.input as f64 * p.input_cost_per_token
                + t.output as f64 * p.output_cost_per_token
                + t.cache_read as f64 * cache_read
                + t.cache_write_5m as f64 * write_5m
                + t.cache_write_1h as f64 * write_1h,
        )
    }
}

/// The price table in force: the imported one marked active, otherwise the embedded one.
pub fn active(conn: &rusqlite::Connection) -> Result<PriceTable> {
    use rusqlite::OptionalExtension;
    let data: Option<String> = conn.query_row("SELECT data FROM price_tables WHERE active = 1 LIMIT 1", [], |r| r.get(0)).optional()?;
    match data {
        Some(text) => PriceTable::from_json(&text),
        None => Ok(PriceTable::embedded().clone()),
    }
}

/// Cheap identity of the table in force: version and import time of the active
/// imported table, `None` for the embedded one. Import and reset change it, so
/// a long-running process can notice a table changed by another process.
pub fn active_stamp(conn: &rusqlite::Connection) -> Result<Option<(String, i64)>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .prepare_cached("SELECT version, imported_at FROM price_tables WHERE active = 1 LIMIT 1")?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

fn strip_date(model: &str) -> &str {
    match model.rsplit_once('-') {
        Some((head, tail)) if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_table_prices_known_models() {
        let table = PriceTable::embedded();
        assert!(table.models.len() > 10);
        let t = TokenCounts {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
        };
        let cost = table.cost("claude-opus-5-5", &t).unwrap();
        assert!((cost - (4.0 + 20.0 + 0.2 + 5.0 + 8.0)).abs() < 1e-6, "{cost}");
    }

    #[test]
    fn unknown_models_are_unpriced_not_free() {
        let table = PriceTable::embedded();
        assert!(table.cost("some-future-model", &TokenCounts::default()).is_none());
        assert!(table.lookup("claude-haiku-4-5-20251001").is_some());
        assert!(table.lookup("claude-opus-5-5[1m]").is_some());
        assert!(table.lookup("opus").is_none());
    }

    #[test]
    fn rejects_negative_prices() {
        let bad = r#"{"version":"x","source":"t","models":{"m":{"input_cost_per_token":-1.0,"output_cost_per_token":0.0}}}"#;
        assert!(PriceTable::from_json(bad).is_err());
    }
}
