//! Saved CSV formats — how to read a bank's CSV layout — and the account each
//! format imports into. The column mapping itself is opaque JSON (`spec`): the
//! frontend owns parsing, the database only keeps it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CsvFormatRow {
    pub id: String,
    pub name: String,
    pub bank: Option<String>,
    pub kind: String,
    pub signature: String,
    pub spec: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NewCsvFormat {
    pub id: String,
    pub name: String,
    pub bank: Option<String>,
    pub kind: String,
    pub signature: String,
    pub spec: String,
}

const COLS: &str = "id, name, bank, kind, signature, spec, created_at";

fn row_to_format(row: &rusqlite::Row<'_>) -> rusqlite::Result<CsvFormatRow> {
    Ok(CsvFormatRow {
        id: row.get(0)?,
        name: row.get(1)?,
        bank: row.get(2)?,
        kind: row.get(3)?,
        signature: row.get(4)?,
        spec: row.get(5)?,
        created_at: row.get(6)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<CsvFormatRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM csv_formats ORDER BY name"))?;
    let rows = stmt
        .query_map([], row_to_format)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Saves a format. A second mapping for the same header layout replaces the
/// first (keeping its id), since only one format can claim a signature.
pub fn save(conn: &Connection, f: &NewCsvFormat) -> AppResult<CsvFormatRow> {
    if !matches!(f.kind.as_str(), "checking" | "credit_card") {
        return Err(AppError::Invalid(format!(
            "invalid kind '{}' (expected 'checking' or 'credit_card')",
            f.kind
        )));
    }
    conn.execute(
        "INSERT INTO csv_formats (id, name, bank, kind, signature, spec)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(signature) DO UPDATE SET
           name = excluded.name, bank = excluded.bank,
           kind = excluded.kind, spec = excluded.spec",
        params![f.id, f.name, f.bank, f.kind, f.signature, f.spec],
    )?;
    conn.query_row(
        &format!("SELECT {COLS} FROM csv_formats WHERE signature = ?1"),
        params![f.signature],
        row_to_format,
    )
    .map_err(AppError::from)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM csv_format_accounts WHERE format_id = ?1",
        params![id],
    )?;
    conn.execute("DELETE FROM csv_formats WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_account(conn: &Connection, format_id: &str) -> AppResult<Option<i64>> {
    conn.query_row(
        "SELECT account_id FROM csv_format_accounts WHERE format_id = ?1",
        params![format_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

pub fn set_account(conn: &Connection, format_id: &str, account_id: i64) -> AppResult<()> {
    conn.execute(
        "INSERT INTO csv_format_accounts (format_id, account_id) VALUES (?1, ?2)
         ON CONFLICT(format_id) DO UPDATE SET account_id = excluded.account_id",
        params![format_id, account_id],
    )?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn list_csv_formats(db: State<'_, Db>) -> AppResult<Vec<CsvFormatRow>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    list(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn save_csv_format(db: State<'_, Db>, format: NewCsvFormat) -> AppResult<CsvFormatRow> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    save(&conn, &format)
}

#[tauri::command]
#[specta::specta]
pub fn delete_csv_format(db: State<'_, Db>, id: String) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    delete(&conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn get_format_account(db: State<'_, Db>, format_id: String) -> AppResult<Option<i64>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    get_account(&conn, &format_id)
}

#[tauri::command]
#[specta::specta]
pub fn set_format_account(db: State<'_, Db>, format_id: String, account_id: i64) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    set_account(&conn, &format_id, account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn
    }

    fn fmt(id: &str, signature: &str) -> NewCsvFormat {
        NewCsvFormat {
            id: id.into(),
            name: "Inter".into(),
            bank: Some("inter".into()),
            kind: "checking".into(),
            signature: signature.into(),
            spec: r#"{"columns":{"date":0}}"#.into(),
        }
    }

    fn account(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid, kind) VALUES ('Inter', NULL, NULL, 'checking')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn migration_0020_creates_tables() {
        let conn = fresh_conn();
        for t in ["csv_formats", "csv_format_accounts"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{t}");
        }
    }

    #[test]
    fn save_then_list_roundtrip() {
        let conn = fresh_conn();
        let saved = save(&conn, &fmt("a", "data|valor")).unwrap();
        assert_eq!(saved.id, "a");
        assert_eq!(saved.spec, r#"{"columns":{"date":0}}"#);
        assert_eq!(list(&conn).unwrap(), vec![saved]);
    }

    #[test]
    fn save_same_signature_upserts() {
        let conn = fresh_conn();
        save(&conn, &fmt("a", "data|valor")).unwrap();
        let mut again = fmt("b", "data|valor");
        again.name = "Inter PJ".into();
        let saved = save(&conn, &again).unwrap();
        assert_eq!(saved.id, "a");
        assert_eq!(saved.name, "Inter PJ");
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn invalid_kind_rejected() {
        let conn = fresh_conn();
        let mut f = fmt("a", "x");
        f.kind = "savings".into();
        assert!(save(&conn, &f).is_err());
    }

    #[test]
    fn delete_clears_remembered_account() {
        let conn = fresh_conn();
        save(&conn, &fmt("a", "x")).unwrap();
        let acc = account(&conn);
        set_account(&conn, "a", acc).unwrap();
        delete(&conn, "a").unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert_eq!(get_account(&conn, "a").unwrap(), None);
    }

    #[test]
    fn remembered_account_cascades_on_delete() {
        let conn = fresh_conn();
        let acc = account(&conn);
        set_account(&conn, "nubank-card", acc).unwrap();
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![acc])
            .unwrap();
        assert_eq!(get_account(&conn, "nubank-card").unwrap(), None);
    }

    #[test]
    fn set_format_account_upserts() {
        let conn = fresh_conn();
        let a = account(&conn);
        let b = account(&conn);
        set_account(&conn, "nubank-card", a).unwrap();
        set_account(&conn, "nubank-card", b).unwrap();
        assert_eq!(get_account(&conn, "nubank-card").unwrap(), Some(b));
    }
}
