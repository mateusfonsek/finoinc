//! Tauri surface for tax-id enrichment. All logic lives in [`crate::enrich`];
//! this only adapts to the shape the frontend already consumes.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::db::Db;
use crate::enrich;
use crate::error::AppResult;
use crate::locale::{LocalePack, LocaleState};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TaxIdResolution {
    pub tax_id: String,
    pub legal_name: Option<String>,
    pub trade_name: Option<String>,
    pub activity_code: Option<String>,
    pub activity_label: Option<String>,
    pub suggested_category_id: Option<i64>,
}

impl TaxIdResolution {
    /// Tax id seen, nothing found — what a locale without a provider returns.
    fn bare(tax_id: &str) -> Self {
        Self {
            tax_id: tax_id.to_string(),
            legal_name: None,
            trade_name: None,
            activity_code: None,
            activity_label: None,
            suggested_category_id: None,
        }
    }
}

/// Does not check the enabled flag — callers decide. `resolve_tax_id` is a
/// direct user action; the import path gates before reaching here.
pub fn resolve_tax_id_with_conn(
    conn: &std::sync::Mutex<rusqlite::Connection>,
    tax_id: &str,
    pack: &LocalePack,
) -> AppResult<TaxIdResolution> {
    let Some(e) = enrich::lookup(conn, tax_id, pack)? else {
        return Ok(TaxIdResolution::bare(tax_id));
    };
    Ok(TaxIdResolution {
        tax_id: tax_id.to_string(),
        legal_name: e.company.legal_name,
        trade_name: e.company.trade_name,
        activity_code: e.company.activity_code,
        activity_label: e.company.activity_label,
        suggested_category_id: e.suggested_category_id,
    })
}

#[tauri::command]
#[specta::specta]
pub fn resolve_tax_id(
    db: State<'_, Db>,
    locale: State<'_, LocaleState>,
    tax_id: String,
) -> AppResult<TaxIdResolution> {
    // The database is locked only around its own queries, never across the
    // provider's network call.
    let pack = locale.pack.lock().expect("locale mutex poisoned");
    resolve_tax_id_with_conn(&db.conn, &tax_id, &pack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::enrich::category_for_activity;
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

    fn category_named(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM categories WHERE name = ?1",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn category_keyed(conn: &Connection, key: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM categories WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// Each CNAE mapping categorizes without asking, so pin the main ones.
    #[test]
    fn cnae_map_routes_known_activities() {
        let conn = fresh_conn();
        let p = pack();
        let cases = [
            ("4711301", category_named(&conn, "Mercado")),
            ("5611201", category_named(&conn, "Restaurante")),
            ("4789099", category_named(&conn, "Compras")),
            ("8531700", category_keyed(&conn, "education")),
            ("7500100", category_keyed(&conn, "pets")),
        ];
        for (cnae, expected) in cases {
            assert_eq!(
                category_for_activity(&conn, cnae, &p).unwrap(),
                Some(expected),
                "CNAE {cnae}"
            );
        }
    }

    #[test]
    fn cnae_map_ignores_unmapped_activity() {
        let conn = fresh_conn();
        // 0111 = grain farming, not in the map.
        assert_eq!(
            category_for_activity(&conn, "0111301", &pack()).unwrap(),
            None
        );
    }

    /// No provider yields an empty resolution, not an error.
    #[test]
    fn no_provider_yields_bare_resolution() {
        let conn = fresh_conn();
        let mut p = pack();
        p.manifest.tax_id.provider = String::new();
        let r = resolve_tax_id_with_conn(&std::sync::Mutex::new(conn), "33.967.103/0001-84", &p).unwrap();
        assert_eq!(r.tax_id, "33.967.103/0001-84");
        assert!(r.legal_name.is_none());
        assert!(r.suggested_category_id.is_none());
    }
}
