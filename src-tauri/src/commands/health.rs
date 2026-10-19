use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::db::Db;
use crate::error::AppResult;

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct HealthInfo {
    pub version: String,
    pub db_path: String,
    pub category_count: u32,
}

#[tauri::command]
#[specta::specta]
pub fn health_check(db: State<'_, Db>) -> AppResult<HealthInfo> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let category_count: u32 =
        conn.query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))?;

    Ok(HealthInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_path: db.path.display().to_string(),
        category_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    #[test]
    fn health_info_struct_serializes() {
        let info = HealthInfo {
            version: "0.1.0".to_string(),
            db_path: "/tmp/finoinc.db".to_string(),
            category_count: 9,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\":\"0.1.0\""));
        assert!(json.contains("\"category_count\":9"));
    }

    #[test]
    fn category_count_matches_seed() {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 13);
    }
}
