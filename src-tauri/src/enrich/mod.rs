//! Enriching transactions from a tax id found in their description.
//!
//! Three things come from the locale pack, which is what keeps this generic:
//! the id format (`manifest.taxId.regex`), the lookup service
//! (`manifest.taxId.provider`) and the activity→category map
//! (`rules.cnae_map`). A pack without a provider makes this whole module a
//! no-op, with no country check anywhere.

pub mod job;
pub mod provider;
pub mod providers;

#[cfg(test)]
pub mod test_support;

use rusqlite::params;

use crate::error::AppResult;
use crate::locale::LocalePack;
use provider::Company;

/// `app_settings` key. Absent means off — no external call without opt-in.
pub const ENABLED_KEY: &str = "enrich_enabled";

pub fn is_enabled(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![ENABLED_KEY],
        |r| r.get::<_, String>(0),
    )
    .map(|v| v == "1")
    .unwrap_or(false)
}

/// Can the active pack enrich at all? This — not a language code — is what the
/// UI should ask before offering the setting.
pub fn is_available(pack: &LocalePack) -> bool {
    pack.tax_id_re.is_some() && provider::for_name(&pack.manifest.tax_id.provider).is_some()
}

/// Available *and* opted in.
pub fn is_active(conn: &rusqlite::Connection, pack: &LocalePack) -> bool {
    is_available(pack) && is_enabled(conn)
}

/// First tax id in the description, in the active locale's format.
pub fn extract_tax_id(pack: &LocalePack, description: &str) -> Option<String> {
    let re = pack.tax_id_re.as_ref()?;
    re.find(description).map(|m| m.as_str().to_string())
}

/// Digits only — what providers expect.
pub fn digits_of(tax_id: &str) -> String {
    tax_id.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Category for an activity code, longest matching prefix wins ("4711" beats
/// "47"). `None` when nothing matches or the category is gone.
pub fn category_for_activity(
    conn: &rusqlite::Connection,
    activity_code: &str,
    pack: &LocalePack,
) -> AppResult<Option<i64>> {
    let mut best: Option<&crate::locale::CnaeEntry> = None;
    for entry in &pack.rules.cnae_map {
        if activity_code.starts_with(&entry.prefix)
            && best.is_none_or(|b| entry.prefix.len() > b.prefix.len())
        {
            best = Some(entry);
        }
    }
    let Some(entry) = best else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT id FROM categories WHERE key = ?1",
            params![entry.category],
            |r| r.get(0),
        )
        .ok())
}

/// A lookup result, already crossed with local categories.
#[derive(Debug, Clone, Default)]
pub struct Enrichment {
    pub company: Company,
    pub suggested_category_id: Option<i64>,
}

/// `Ok(None)` — not an error — when the active locale has no provider. That's
/// a normal condition, not a failure.
pub fn lookup(
    conn: &rusqlite::Connection,
    tax_id: &str,
    pack: &LocalePack,
) -> AppResult<Option<Enrichment>> {
    let Some(p) = provider::for_name(&pack.manifest.tax_id.provider) else {
        return Ok(None);
    };
    Ok(Some(lookup_with(conn, tax_id, pack, p.as_ref())?))
}

/// Lookup with the provider handed in — the testable shape, no network.
///
/// Separate from [`lookup`] because resolving the provider from the pack inside
/// it made it impossible to exercise the enrichment loop without calling the
/// real external service.
pub fn lookup_with(
    conn: &rusqlite::Connection,
    tax_id: &str,
    pack: &LocalePack,
    provider: &dyn provider::TaxIdProvider,
) -> AppResult<Enrichment> {
    let company = provider.lookup(&digits_of(tax_id))?;
    let suggested = match company.activity_code.as_deref() {
        Some(code) => category_for_activity(conn, code, pack)?,
        None => None,
    };
    Ok(Enrichment {
        company,
        suggested_category_id: suggested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    fn pack() -> LocalePack {
        LocalePack::embedded_pt_br()
    }

    use crate::enrich::test_support::FakeProvider;

    #[test]
    fn lookup_with_uses_the_injected_provider() {
        let conn = fresh_conn();
        let p = pack();
        // 4711301 = grocery, mapped in the pt-BR pack.
        let fake = FakeProvider::new(&[("33967103000184", "4711301")]);

        let e = lookup_with(&conn, "33.967.103/0001-84", &p, &fake).unwrap();

        assert_eq!(fake.call_count(), 1, "must query the injected provider");
        assert!(
            e.suggested_category_id.is_some(),
            "a mapped activity code suggests a category"
        );
        assert_eq!(
            e.company.legal_name.as_deref(),
            Some("EMPRESA 33967103000184")
        );
    }

    #[test]
    fn lookup_with_maps_unknown_activity_to_no_category() {
        let conn = fresh_conn();
        let p = pack();
        // 0111301 = grain farming, outside the cnae_map.
        let fake = FakeProvider::new(&[("33967103000184", "0111301")]);

        let e = lookup_with(&conn, "33.967.103/0001-84", &p, &fake).unwrap();

        assert!(e.suggested_category_id.is_none());
    }

    #[test]
    fn locale_pack_is_clonable() {
        let p = pack();
        let c = p.clone();
        assert_eq!(c.manifest.tax_id.provider, p.manifest.tax_id.provider);
    }

    #[test]
    fn extracts_tax_id_by_locale_format() {
        let desc = "Transferência enviada pelo Pix - DEMERGE - 33.967.103/0001-84 - banco";
        assert_eq!(
            extract_tax_id(&pack(), desc),
            Some("33.967.103/0001-84".into())
        );
        assert_eq!(extract_tax_id(&pack(), "Compra no débito - MISTER SUSHI"), None);
    }

    #[test]
    fn digits_only() {
        assert_eq!(digits_of("33.967.103/0001-84"), "33967103000184");
    }

    #[test]
    fn longest_activity_prefix_wins() {
        let conn = fresh_conn();
        let p = pack();
        let market = category_for_activity(&conn, "4711302", &p).unwrap();
        let restaurant = category_for_activity(&conn, "5611203", &p).unwrap();
        assert!(market.is_some());
        assert!(restaurant.is_some());
        assert_ne!(market, restaurant);
    }

    #[test]
    fn unknown_activity_has_no_category() {
        let conn = fresh_conn();
        assert_eq!(category_for_activity(&conn, "9999999", &pack()).unwrap(), None);
    }

    #[test]
    fn disabled_by_default_even_when_available() {
        let conn = fresh_conn();
        let p = pack();
        assert!(is_available(&p), "pt-BR declares regex + brasilapi");
        assert!(!is_enabled(&conn), "nobody opted in");
        assert!(!is_active(&conn, &p));
    }

    #[test]
    fn active_once_enabled() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, '1')",
            params![ENABLED_KEY],
        )
        .unwrap();
        assert!(is_active(&conn, &pack()));
    }

    #[test]
    fn no_provider_means_unavailable_and_lookup_is_noop() {
        let conn = fresh_conn();
        let mut p = pack();
        p.manifest.tax_id.provider = "none".into();
        assert!(!is_available(&p));
        assert!(lookup(&conn, "33.967.103/0001-84", &p).unwrap().is_none());
    }
}
