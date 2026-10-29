use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::backup::Progress;
use rusqlite::{Connection, DatabaseName};
use tauri::State;

use crate::db::{migrations, Db};
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

/// Write a consistent copy of the active DB to `destination`.
#[tauri::command]
#[specta::specta]
pub fn export_backup(db: State<'_, Db>, destination: String) -> AppResult<String> {
    let dest = PathBuf::from(&destination);
    let conn = db.conn.lock().expect("db mutex poisoned");
    export_to(&conn, &db.path, &dest)?;
    Ok(dest.display().to_string())
}

/// Replace the active DB's contents with the backup at `source`. Frontend
/// should prompt the user to restart the app after this call.
#[tauri::command]
#[specta::specta]
pub fn restore_backup(db: State<'_, Db>, source: String) -> AppResult<()> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    restore_from(&mut conn, &db.path, &PathBuf::from(&source))
}

fn export_to(conn: &Connection, db_path: &Path, dest: &Path) -> AppResult<()> {
    refuse_live_file(db_path, dest)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    // `VACUUM INTO` refuses a file that already exists, and the save dialog can
    // pick one: write beside it and rename, so a failure never leaves a
    // half-written backup in its place.
    let partial = dest.with_extension("partial");
    let _ = fs::remove_file(&partial);
    conn.execute("VACUUM INTO ?1", [partial.to_string_lossy()])?;
    fs::rename(&partial, dest)?;
    Ok(())
}

/// Goes through SQLite's backup API on the open connection instead of copying
/// over the file: the connection keeps its cache and handles across a file
/// copy, so a later write or checkpoint would mix the old database into the
/// restored one.
fn restore_from(conn: &mut Connection, db_path: &Path, src: &Path) -> AppResult<()> {
    refuse_live_file(db_path, src)?;
    validate_sqlite_file(src)?;
    conn.restore(DatabaseName::Main, src, None::<fn(Progress)>)?;
    // A backup from an older release has an older schema.
    migrations::apply(conn)?;
    Ok(())
}

/// Copying the database onto itself truncates it to zero bytes.
fn refuse_live_file(db_path: &Path, other: &Path) -> AppResult<()> {
    let same = match (fs::canonicalize(db_path), fs::canonicalize(other)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if same {
        return Err(AppError::Invalid(
            "the backup cannot be the app's own database file".into(),
        ));
    }
    Ok(())
}

fn validate_sqlite_file(path: &Path) -> AppResult<()> {
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

    /// A WAL-mode database with one account named `name`, like the live one.
    fn db_with_account(path: &Path, name: &str) -> Connection {
        for ext in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{ext}", path.display()));
        }
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES (?1, 'checking')",
            [name],
        )
        .unwrap();
        conn
    }

    fn account_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT name FROM accounts ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    }

    /// Overwriting the file under the open connection let a later write or
    /// checkpoint mix pages of the old database into the restored one.
    #[test]
    fn restore_replaces_the_live_database_and_later_writes_land_on_it() {
        let live_path = temp_path("live.db");
        let backup_path = temp_path("backup.db");
        let mut live = db_with_account(&live_path, "old");
        drop(db_with_account(&backup_path, "restored"));

        restore_from(&mut live, &live_path, &backup_path).unwrap();
        live.execute("INSERT INTO accounts (name, kind) VALUES ('after', 'checking')", [])
            .unwrap();
        assert_eq!(account_names(&live), ["restored", "after"]);
        drop(live);

        let reopened = Connection::open(&live_path).unwrap();
        assert_eq!(account_names(&reopened), ["restored", "after"]);
    }

    #[test]
    fn restore_brings_an_older_backup_up_to_the_current_schema() {
        let live_path = temp_path("live-schema.db");
        let backup_path = temp_path("old-schema.db");
        let mut live = db_with_account(&live_path, "live");
        {
            let _ = fs::remove_file(&backup_path);
            let old = Connection::open(&backup_path).unwrap();
            old.execute_batch(include_str!("../../migrations/0001_init.sql")).unwrap();
            old.execute_batch(
                "CREATE TABLE _migrations (name TEXT PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')));
                 INSERT INTO _migrations (name) VALUES ('0001_init');",
            )
            .unwrap();
        }

        restore_from(&mut live, &live_path, &backup_path).unwrap();
        let has_patterns: bool = live
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'rule_patterns'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(has_patterns, "migrations did not run on the restored backup");
    }

    /// Copying a file onto itself truncates it to zero bytes.
    #[test]
    fn restore_refuses_the_live_database_itself() {
        let live_path = temp_path("self-restore.db");
        let mut live = db_with_account(&live_path, "keep");
        assert!(restore_from(&mut live, &live_path, &live_path).is_err());
        assert_eq!(account_names(&live), ["keep"]);
    }

    #[test]
    fn export_refuses_to_overwrite_the_live_database() {
        let live_path = temp_path("self-export.db");
        let live = db_with_account(&live_path, "keep");
        assert!(export_to(&live, &live_path, &live_path).is_err());
        assert_eq!(account_names(&live), ["keep"]);
    }

    /// The save dialog lets the user pick an existing file to replace.
    #[test]
    fn export_replaces_an_existing_file_with_a_full_copy() {
        let live_path = temp_path("export-live.db");
        let dest = temp_path("export-dest.db");
        fs::write(&dest, b"previous backup").unwrap();
        let live = db_with_account(&live_path, "exported");

        export_to(&live, &live_path, &dest).unwrap();
        let copy = Connection::open(&dest).unwrap();
        assert_eq!(account_names(&copy), ["exported"]);
    }
}
