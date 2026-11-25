//! The price table in force: info, signed import and reset to the embedded table.

use kanjo_core::api::PricingInfo;
use kanjo_core::pricing::PriceTable;
use kanjo_core::store::write;
use kanjo_core::{Error, Result, Store};
use rusqlite::{params, OptionalExtension};

/// The active table (imported or embedded) and the models it cannot price.
pub fn info(store: &Store) -> Result<PricingInfo> {
    store.read(|c| {
        let active: Option<(String, Option<String>)> = c
            .query_row("SELECT data, signed_by FROM price_tables WHERE active = 1 LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let (table, signed) = match active {
            Some((data, signed_by)) => (PriceTable::from_json(&data)?, signed_by.is_some()),
            None => (PriceTable::embedded().clone(), false),
        };
        let mut stmt = c.prepare(
            "SELECT model FROM requests WHERE cost_usd IS NULL UNION SELECT model FROM usage_hourly WHERE unpriced_requests > 0 ORDER BY 1",
        )?;
        let unpriced_models = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(PricingInfo { version: table.version, source: table.source, models: table.models.len() as i64, signed, unpriced_models })
    })
}

/// Imports a price file, makes it active and reprices every stored request in
/// the same transaction. With `signature` (the text of a `.minisig` file)
/// the file must verify against one of `trusted_keys` (minisign public keys,
/// bare base64 or the two-line `.pub` format). Without one the file is
/// accepted only when `allow_unsigned`, and is recorded as unsigned.
pub fn import(store: &Store, json: &[u8], signature: Option<&str>, trusted_keys: &[String], allow_unsigned: bool) -> Result<PricingInfo> {
    let text = std::str::from_utf8(json).map_err(|_| Error::Invalid("the price file is not UTF-8".into()))?;
    let table = PriceTable::from_json(text)?;
    if table.models.is_empty() {
        return Err(Error::Invalid("the price file lists no models".into()));
    }
    let signed_by = match signature {
        Some(sig) => Some(verify(json, sig, trusted_keys)?),
        None if allow_unsigned => None,
        None => {
            return Err(Error::Invalid(
                "the price file is not signed; import it with a signature or allow unsigned tables explicitly".into(),
            ))
        }
    };
    let now = kanjo_core::time::now_ms();
    store.write(|tx| {
        tx.execute("UPDATE price_tables SET active = 0 WHERE active != 0", [])?;
        tx.execute(
            "INSERT INTO price_tables (version, source, imported_at, signed_by, data, active) VALUES (?1, ?2, ?3, ?4, ?5, 1)
             ON CONFLICT (version) DO UPDATE SET source = excluded.source, imported_at = excluded.imported_at,
               signed_by = excluded.signed_by, data = excluded.data, active = 1",
            params![table.version, table.source, now, signed_by, text],
        )?;
        write::reprice_all(tx, &table)?;
        Ok(())
    })?;
    info(store)
}

/// Returns to the embedded table and reprices. Imported tables stay stored, inactive.
pub fn reset(store: &Store) -> Result<PricingInfo> {
    store.write(|tx| {
        tx.execute("UPDATE price_tables SET active = 0 WHERE active != 0", [])?;
        write::reprice_all(tx, PriceTable::embedded())?;
        Ok(())
    })?;
    info(store)
}

/// Returns the base64 public key that verified `data`.
fn verify(data: &[u8], signature: &str, trusted_keys: &[String]) -> Result<String> {
    let sig = minisign_verify::Signature::decode(signature).map_err(|e| Error::Invalid(format!("unreadable signature: {e}")))?;
    if trusted_keys.is_empty() {
        return Err(Error::Invalid("a signature was given but no trusted price keys are configured".into()));
    }
    for raw in trusted_keys {
        let Some(b64) = key_line(raw) else { continue };
        let Ok(pk) = minisign_verify::PublicKey::from_base64(b64) else {
            tracing::warn!("ignoring an unreadable trusted price key");
            continue;
        };
        if pk.verify(data, &sig, false).is_ok() {
            return Ok(b64.to_string());
        }
    }
    Err(Error::Invalid("the price file signature does not match any trusted key".into()))
}

fn key_line(raw: &str) -> Option<&str> {
    raw.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanjo_core::testkit::{self, TestStore};

    fn table_json(version: &str, factor: f64) -> String {
        let mut t = PriceTable::embedded().clone();
        t.version = version.into();
        t.source = "test".into();
        for p in t.models.values_mut() {
            p.input_cost_per_token *= factor;
            p.output_cost_per_token *= factor;
        }
        serde_json::to_string(&t).unwrap()
    }

    fn keypair() -> (String, minisign::SecretKey) {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        (kp.pk.to_base64(), kp.sk)
    }

    fn sign(sk: &minisign::SecretKey, data: &str) -> String {
        minisign::sign(None, sk, std::io::Cursor::new(data.as_bytes()), None, None).unwrap().into_string()
    }

    fn cost(t: &TestStore) -> f64 {
        t.store.read(|c| Ok(c.query_row("SELECT SUM(cost_usd) FROM sessions", [], |r| r.get(0))?)).unwrap()
    }

    fn seeded() -> TestStore {
        let t = TestStore::new();
        let p = t.project("p1");
        for (i, model) in ["claude-opus-5-5", "claude-sonnet-5-5", "mystery-model"].iter().enumerate() {
            let mut r = testkit::request(&format!("m{i}"), "s1", 1_795_000_000_000 + i as i64, model, 1000, 500);
            r.project_id = p;
            t.insert(&r);
        }
        t
    }

    #[test]
    fn signed_import_reprices_and_reset_restores() {
        let t = seeded();
        let base = cost(&t);
        let start = info(&t.store).unwrap();
        assert_eq!(start.version, PriceTable::embedded().version);
        assert!(!start.signed);
        assert_eq!(start.unpriced_models, vec!["mystery-model".to_string()]);

        let (pk, sk) = keypair();
        let (other_pk, _) = keypair();
        let json = table_json("2027-01-01", 2.0);
        let sig = sign(&sk, &json);
        let pub_file = format!("untrusted comment: minisign public key\n{pk}\n");
        let got = import(&t.store, json.as_bytes(), Some(&sig), &[other_pk.clone(), pub_file], false).unwrap();
        assert!(got.signed);
        assert_eq!(got.version, "2027-01-01");
        assert!((cost(&t) - base * 2.0).abs() < 1e-9);
        let (signed_by, version): (String, String) = t
            .store
            .read(|c| {
                Ok(c.query_row("SELECT signed_by, MAX(price_version) FROM price_tables, requests WHERE active = 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?)
            })
            .unwrap();
        assert_eq!((signed_by, version.as_str()), (pk, "2027-01-01"));

        let tampered = json.replace("2027-01-01", "2027-01-02");
        assert!(import(&t.store, tampered.as_bytes(), Some(&sig), std::slice::from_ref(&other_pk), true).is_err());
        assert!(import(&t.store, json.as_bytes(), Some(&sig), &[other_pk], true).is_err(), "wrong key fails even with allow_unsigned");
        assert!(import(&t.store, json.as_bytes(), Some(&sig), &[], true).is_err());

        let back = reset(&t.store).unwrap();
        assert_eq!(back.version, PriceTable::embedded().version);
        assert!((cost(&t) - base).abs() < 1e-9);
    }

    #[test]
    fn unsigned_refused_unless_allowed() {
        let t = seeded();
        let base = cost(&t);
        let json = table_json("local-1", 3.0);
        assert!(import(&t.store, json.as_bytes(), None, &[], false).is_err());
        assert!((cost(&t) - base).abs() < 1e-12);
        let got = import(&t.store, json.as_bytes(), None, &[], true).unwrap();
        assert!(!got.signed);
        assert!((cost(&t) - base * 3.0).abs() < 1e-9);
        let again = import(&t.store, table_json("local-1", 1.0).as_bytes(), None, &[], true).unwrap();
        assert_eq!(again.version, "local-1");
        assert!((cost(&t) - base).abs() < 1e-9);
        let active: i64 = t.store.read(|c| Ok(c.query_row("SELECT SUM(active) FROM price_tables", [], |r| r.get(0))?)).unwrap();
        assert_eq!(active, 1);
    }

    #[test]
    fn invalid_tables_are_rejected() {
        let t = seeded();
        assert!(import(&t.store, b"{not json", None, &[], true).is_err());
        assert!(import(&t.store, br#"{"version":"x","source":"t","models":{}}"#, None, &[], true).is_err());
        assert!(import(&t.store, &[0xff, 0xfe], None, &[], true).is_err());
    }
}
