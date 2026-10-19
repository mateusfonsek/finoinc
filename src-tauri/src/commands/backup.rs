use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\x00";

/// Returns the absolute path of the active SQLite DB file as a string.
#[tauri::command]
#[specta::specta]
pub fn db_path(db: State<'_, Db>) -> AppResult<String> {
    Ok(db.path.display().to_string())
}

/// Reads a file's raw bytes. Used by OFX drag-and-drop: Tauri's drop event
/// delivers only the path, not the contents.
#[tauri::command]
#[specta::specta]
pub fn read_file_bytes(path: String) -> AppResult<Vec<u8>> {
    Ok(fs::read(&path)?)
}

/// Copy the active DB file to `destination`. Issues a WAL checkpoint first
/// so the .db file is consistent on disk before copying.
#[tauri::command]
#[specta::specta]
pub fn export_backup(db: State<'_, Db>, destination: String) -> AppResult<String> {
    {
        let conn = db.conn.lock().expect("db mutex poisoned");
        let _ = conn.pragma_update(None, "wal_checkpoint", "FULL");
    }
    let dest = PathBuf::from(&destination);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&db.path, &dest)?;
    Ok(dest.display().to_string())
}

/// Validate that `source` is a SQLite 3 file (header magic + size > 16),
/// then overwrite the active DB. Frontend should prompt the user to restart
/// the app after this call.
#[tauri::command]
#[specta::specta]
pub fn restore_backup(db: State<'_, Db>, source: String) -> AppResult<()> {
    let src = PathBuf::from(&source);
    validate_sqlite_file(&src)?;

    {
        let conn = db.conn.lock().expect("db mutex poisoned");
        let _ = conn.pragma_update(None, "wal_checkpoint", "FULL");
    }

    fs::copy(&src, &db.path)?;

    // Remove sidecar -wal / -shm files so the restored DB is not shadowed.
    for ext in ["-wal", "-shm"] {
        let sidecar = db.path.with_extension(format!(
            "{}{}",
            db.path.extension().and_then(|e| e.to_str()).unwrap_or(""),
            ext
        ));
        let _ = fs::remove_file(sidecar);
    }
    Ok(())
}

fn validate_sqlite_file(path: &PathBuf) -> AppResult<()> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 16];
    let n = file.read(&mut header)?;
    if n < 16 || &header != SQLITE_HEADER {
        return Err(AppError::Invalid(format!(
            "{} is not a valid SQLite 3 file",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;
    use std::io::Write;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("finoinc-test-{}-{name}", std::process::id()));
        p
    }

    #[test]
    fn validate_accepts_real_sqlite_file() {
        let path = temp_path("ok.db");
        let _ = fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        drop(conn);

        validate_sqlite_file(&path).expect("valid sqlite should pass");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn validate_rejects_random_file() {
        let path = temp_path("nope.bin");
        let _ = fs::remove_file(&path);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"not a sqlite file at all").unwrap();
        drop(f);

        let r = validate_sqlite_file(&path);
        assert!(r.is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn restore_copies_file_contents() {
        let src = temp_path("src.db");
        let dst = temp_path("dst.db");
        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);

        let conn = Connection::open(&src).unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        drop(conn);

        validate_sqlite_file(&src).unwrap();
        fs::copy(&src, &dst).unwrap();

        assert!(validate_sqlite_file(&dst).is_ok());
        let dst_conn = Connection::open(&dst).unwrap();
        let count: i64 = dst_conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 13);

        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);
    }
}
