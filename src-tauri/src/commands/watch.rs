//! Automatic import — discovering `.ofx` statements in user-picked folders.
//!
//! No filesystem watcher on purpose: the user sends the file from their phone
//! *and then goes to look at the Mac*, so window focus is the natural trigger.
//! That avoids the `notify` dependency, avoids interpreting FS events, and
//! avoids the known problem of FSEvents not firing reliably inside iCloud
//! Drive.

use std::path::Path;
use std::time::{Duration, SystemTime};

use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use tauri::State;

use sha2::{Digest, Sha256};

use crate::commands::app_settings::get_setting;
use crate::db::Db;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Type)]
pub struct WatchedFolder {
    pub id: i64,
    pub path: String,
    /// Short display name ("Finoinc", "Downloads"), derived from the last path
    /// component when the folder was added.
    pub label: String,
    /// `false` when the folder is gone or unmounted — the row shows an error
    /// state but the other folders keep working.
    pub exists: bool,
    pub imported_count: i64,
    pub last_imported_at: Option<String>,
}

/// Display name from a path. Falls back to "/" only for degenerate paths.
fn label_for(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Resolves symlinks and normalizes (`/tmp` → `/private/tmp` on macOS) so the
/// table's UNIQUE actually rejects the same folder added twice.
///
/// Error messages below are developer-facing and in English, like the rest of
/// the commands: they reach the UI via `e.message`, and the UI is what turns
/// them into user text (`t("watch.error_*")`). Localizing them here would leak
/// one language into every future locale pack.
fn canonical(path: &str) -> AppResult<String> {
    let p = std::fs::canonicalize(path)
        .map_err(|e| AppError::Path(format!("failed to resolve path '{path}': {e}")))?;
    if !p.is_dir() {
        return Err(AppError::Invalid(format!("'{path}' is not a directory")));
    }
    Ok(p.to_string_lossy().to_string())
}

fn row_to_folder(row: &rusqlite::Row) -> rusqlite::Result<WatchedFolder> {
    let path: String = row.get(1)?;
    let exists = Path::new(&path).is_dir();
    Ok(WatchedFolder {
        id: row.get(0)?,
        path,
        label: row.get(2)?,
        exists,
        imported_count: row.get(3)?,
        last_imported_at: row.get(4)?,
    })
}

/// The per-folder imported count uses `LIKE '<path>/%'` over each file's last
/// known path. It is evidence for the user ("3 statements imported"), not
/// accounting: a file moved after import shifts folders in the count, which is
/// fine.
const SELECT_FOLDERS: &str = "
    SELECT w.id,
           w.path,
           w.label,
           (SELECT COUNT(*) FROM seen_files s
             WHERE s.status = 'imported' AND s.path LIKE w.path || '/%'),
           (SELECT MAX(s.resolved_at) FROM seen_files s
             WHERE s.status = 'imported' AND s.path LIKE w.path || '/%')
      FROM watched_folders w
     ORDER BY w.added_at";

pub fn list_folders(conn: &Connection) -> AppResult<Vec<WatchedFolder>> {
    let mut stmt = conn.prepare(SELECT_FOLDERS)?;
    let rows = stmt.query_map([], row_to_folder)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

fn folder_by_id(conn: &Connection, id: i64) -> AppResult<WatchedFolder> {
    let all = {
        let mut stmt = conn.prepare(SELECT_FOLDERS)?;
        let rows = stmt.query_map([], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    all.into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| AppError::Invalid(format!("watched folder {id} not found")))
}

pub fn add_folder(conn: &Connection, path: &str) -> AppResult<WatchedFolder> {
    let canon = canonical(path)?;
    let label = label_for(&canon);
    conn.execute(
        "INSERT INTO watched_folders (path, label) VALUES (?1, ?2)
         ON CONFLICT(path) DO NOTHING",
        params![canon, label],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM watched_folders WHERE path = ?1",
        params![canon],
        |r| r.get(0),
    )?;
    folder_by_id(conn, id)
}

/// Used by "Locate…" on an errored row: repoints the folder while keeping the
/// record (and therefore the history) instead of forcing remove-and-re-add.
pub fn update_folder_path(conn: &Connection, id: i64, path: &str) -> AppResult<WatchedFolder> {
    let canon = canonical(path)?;
    let label = label_for(&canon);
    let changed = conn.execute(
        "UPDATE watched_folders SET path = ?2, label = ?3 WHERE id = ?1",
        params![id, canon, label],
    )?;
    if changed == 0 {
        return Err(AppError::Invalid(format!("watched folder {id} not found")));
    }
    folder_by_id(conn, id)
}

pub fn remove_folder(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM watched_folders WHERE id = ?1", params![id])?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn list_watched_folders(db: State<'_, Db>) -> AppResult<Vec<WatchedFolder>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    list_folders(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn add_watched_folder(db: State<'_, Db>, path: String) -> AppResult<WatchedFolder> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    add_folder(&conn, &path)
}

#[tauri::command]
#[specta::specta]
pub fn update_watched_folder_path(
    db: State<'_, Db>,
    id: i64,
    path: String,
) -> AppResult<WatchedFolder> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    update_folder_path(&conn, id, &path)
}

#[tauri::command]
#[specta::specta]
pub fn remove_watched_folder(db: State<'_, Db>, id: i64) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    remove_folder(&conn, id)
}

/// Creates a folder if missing. Exists only for the "iCloud Drive › Finoinc"
/// preset, the feature's only disk write, and even then only after explicit
/// confirmation in the UI.
///
/// Three lines here avoid pulling in the whole `tauri-plugin-fs`, which would
/// open far more file access than needed.
#[tauri::command]
#[specta::specta]
pub fn ensure_dir(path: String) -> AppResult<()> {
    std::fs::create_dir_all(&path)?;
    Ok(())
}

/// Lets the UI ask "create the folder?" only when it really is missing.
#[tauri::command]
#[specta::specta]
pub fn dir_exists(path: String) -> bool {
    Path::new(&path).is_dir()
}

/// `app_settings` key. Absent means off.
pub const WATCH_ENABLED_KEY: &str = "watch_enabled";

/// How many `.ofx` the last scan saw as not-yet-downloaded iCloud
/// placeholders. Settings shows this as information, never as an error.
pub const ICLOUD_PENDING_KEY: &str = "watch_icloud_pending";

/// When the last scan finished (UTC, SQLite `datetime('now')` format).
/// Settings shows the real time as evidence the feature is alive.
pub const LAST_SCAN_KEY: &str = "watch_last_scan_at";

/// A file with an mtime newer than this may still be being written (AirDrop
/// landing, download in progress). Defer to the next cycle instead of reading
/// half of it.
const SETTLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Type)]
pub struct DiscoveredFile {
    pub id: i64,
    pub content_hash: String,
    pub path: String,
    pub file_name: String,
    pub size: i64,
    pub status: String,
    pub seen_at: String,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: String,
    pub file_name: String,
    pub size: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    pub candidates: Vec<Candidate>,
    /// How many `.ofx` are in iCloud but not downloaded yet. Surfaces as
    /// information ("waiting for iCloud download"), never as an error.
    pub icloud_pending: usize,
}

fn is_ofx(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".ofx")
}

/// iCloud placeholder: an undownloaded `Nubank.ofx` appears on disk as
/// `.Nubank.ofx.icloud`. Returns the real name when it is an `.ofx` stub.
fn icloud_stub_target(name: &str) -> Option<String> {
    let inner = name.strip_prefix('.')?.strip_suffix(".icloud")?;
    if is_ofx(inner) {
        Some(inner.to_string())
    } else {
        None
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Scans **one** folder, no recursion. `now` is injected so tests can simulate
/// elapsed time without sleeping.
pub fn scan_dir(dir: &Path, now: SystemTime) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // Folder gone, unmounted or unreadable: stay silent. The UI shows the
        // error state via `WatchedFolder::exists`.
        Err(_) => return outcome,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        if let Some(target) = icloud_stub_target(&name) {
            // Try to materialize: on macOS, *opening* the real path triggers
            // the on-demand download — no need to read the bytes (a large
            // statement would be loaded into memory just to be dropped here).
            // If it still is not there, count it and move on.
            let real = dir.join(&target);
            if std::fs::File::open(&real).is_err() {
                outcome.icloud_pending += 1;
            }
            continue;
        }

        if !is_ofx(&name) {
            continue;
        }

        // `DirEntry::metadata()` does NOT follow symlinks, on purpose: a link
        // inside the watched folder pointing outside it is not a regular file
        // here, falls into `_ => continue` and is ignored. That keeps the scan
        // scoped to exactly the folder the user picked, which is the feature's
        // privacy promise.
        let meta = match entry.metadata() {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };

        // Written just now? Probably still arriving.
        if let Ok(modified) = meta.modified() {
            match now.duration_since(modified) {
                Ok(age) if age < SETTLE => continue,
                // mtime in the future (skewed clock) — treat as recent.
                Err(_) => continue,
                _ => {}
            }
        }

        let path = entry.path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        outcome.candidates.push(Candidate {
            path: path.to_string_lossy().to_string(),
            file_name: name,
            size: meta.len() as i64,
            content_hash: hash_bytes(&bytes),
        });
    }

    outcome
}

const SELECT_FILES: &str =
    "SELECT id, content_hash, path, file_name, size, status, seen_at FROM seen_files";

fn row_to_file(row: &rusqlite::Row) -> rusqlite::Result<DiscoveredFile> {
    Ok(DiscoveredFile {
        id: row.get(0)?,
        content_hash: row.get(1)?,
        path: row.get(2)?,
        file_name: row.get(3)?,
        size: row.get(4)?,
        status: row.get(5)?,
        seen_at: row.get(6)?,
    })
}

pub fn pending_files(conn: &Connection) -> AppResult<Vec<DiscoveredFile>> {
    let sql = format!("{SELECT_FILES} WHERE status = 'pending' ORDER BY seen_at, id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_file)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

pub fn is_enabled(conn: &Connection) -> AppResult<bool> {
    Ok(get_setting(conn, WATCH_ENABLED_KEY)?.as_deref() == Some("1"))
}

/// Phase 1 (needs the DB): what the scan must know before touching disk.
/// `None` means the feature is off — and it is `None` *before* any filesystem
/// access, which is what guarantees it is inert when disabled.
pub fn scan_targets(conn: &Connection) -> AppResult<Option<Vec<String>>> {
    if !is_enabled(conn)? {
        return Ok(None);
    }
    Ok(Some(
        list_folders(conn)?.into_iter().map(|f| f.path).collect(),
    ))
}

/// Phase 2 (no DB): filesystem only. Separate precisely so it can run
/// **outside** the DB mutex — `read_dir` plus reading plus SHA-256 per file can
/// take seconds, and every command in the app waits on that same mutex.
pub fn scan_paths(paths: &[String], now: SystemTime) -> ScanOutcome {
    let mut merged = ScanOutcome::default();
    for path in paths {
        let outcome = scan_dir(Path::new(path), now);
        merged.icloud_pending += outcome.icloud_pending;
        merged.candidates.extend(outcome.candidates);
    }
    merged
}

/// Phase 3 (needs the DB): records what the scan found and returns **all**
/// pending files, not just this round's — what the UI needs for the badge and
/// the queue.
pub fn record_scan(conn: &Connection, outcome: &ScanOutcome) -> AppResult<Vec<DiscoveredFile>> {
    for c in &outcome.candidates {
        // Unknown hash → new file. Known hash → just update the path, since
        // the file may have been moved or renamed.
        conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status)
             VALUES (?1, ?2, ?3, ?4, 'pending')
             ON CONFLICT(content_hash) DO UPDATE SET
               path = excluded.path,
               file_name = excluded.file_name",
            params![c.content_hash, c.path, c.file_name, c.size],
        )?;
    }

    // The iCloud stub count goes to the KV store rather than the return value:
    // it is a scalar Settings reads occasionally, not something the discovery
    // store needs to carry on every scan.
    crate::commands::app_settings::set_setting(
        conn,
        ICLOUD_PENDING_KEY,
        &outcome.icloud_pending.to_string(),
    )?;

    // Scan timestamp from SQLite's clock (UTC), not the injected `now`, which
    // tests deliberately skew — Settings must show the real time.
    let scanned_at: String = conn.query_row("SELECT datetime('now')", [], |r| r.get(0))?;
    crate::commands::app_settings::set_setting(conn, LAST_SCAN_KEY, &scanned_at)?;

    pending_files(conn)
}

/// Scans every watched folder and records the new files as `pending`.
///
/// `now` is injected (same reason as `scan_dir`): tests simulate elapsed time
/// without sleeping. In production the caller always passes
/// `SystemTime::now()` — if the scan invented its own "fast-forwarded" clock
/// here, the `SETTLE` guard in `scan_dir` would never defer anything, and a
/// file still being written (AirDrop, download) would be read and hashed half
/// complete.
///
/// The three phases composed over a single connection. The Tauri command does
/// **not** go through here — it must release the mutex between phases (see
/// `scan_paths`) — so this exists for tests: the whole pipeline driven by an
/// injected clock over an in-memory connection. `#[cfg(test)]` precisely so it
/// cannot become a second entry point in production.
#[cfg(test)]
pub fn scan_all(conn: &Connection, now: SystemTime) -> AppResult<Vec<DiscoveredFile>> {
    let Some(paths) = scan_targets(conn)? else {
        return Ok(Vec::new());
    };
    let outcome = scan_paths(&paths, now);
    record_scan(conn, &outcome)
}

pub fn mark(conn: &Connection, content_hash: &str, status: &str) -> AppResult<()> {
    if !matches!(status, "pending" | "imported" | "ignored" | "invalid") {
        return Err(AppError::Invalid(format!("invalid status '{status}'")));
    }
    conn.execute(
        "UPDATE seen_files
            SET status = ?2,
                resolved_at = CASE WHEN ?2 = 'pending' THEN NULL ELSE datetime('now') END
          WHERE content_hash = ?1",
        params![content_hash, status],
    )?;
    Ok(())
}

/// The scan runs on window focus — exactly when the user is coming back to
/// interact. So the DB mutex is taken in two short moments (read the folders /
/// write the result) and **released** during disk work: holding it end to end
/// would freeze the whole UI, which shares that same mutex.
#[tauri::command]
#[specta::specta]
pub fn scan_watched_folders(db: State<'_, Db>) -> AppResult<Vec<DiscoveredFile>> {
    let targets = {
        let conn = db.conn.lock().expect("db mutex poisoned");
        scan_targets(&conn)?
    };
    let Some(paths) = targets else {
        return Ok(Vec::new());
    };

    let outcome = scan_paths(&paths, SystemTime::now());

    let conn = db.conn.lock().expect("db mutex poisoned");
    record_scan(&conn, &outcome)
}

#[tauri::command]
#[specta::specta]
pub fn mark_file(db: State<'_, Db>, content_hash: String, status: String) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    mark(&conn, &content_hash, &status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    /// Unique temp folder per test, without external crates.
    fn tmpdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("finoinc-watch-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn add_folder_stores_canonical_path_and_label() {
        let conn = fresh_conn();
        let dir = tmpdir("add");
        let folder = add_folder(&conn, dir.to_str().unwrap()).unwrap();

        assert_eq!(folder.label, "finoinc-watch-test-add");
        assert!(folder.exists);
        assert_eq!(folder.imported_count, 0);
        assert_eq!(folder.last_imported_at, None);
        // canonicalize resolves /var to /private/var on macOS
        assert_eq!(folder.path, std::fs::canonicalize(&dir).unwrap().to_string_lossy());
    }

    #[test]
    fn add_folder_twice_is_idempotent() {
        let conn = fresh_conn();
        let dir = tmpdir("dup");
        let a = add_folder(&conn, dir.to_str().unwrap()).unwrap();
        let b = add_folder(&conn, dir.to_str().unwrap()).unwrap();

        assert_eq!(a.id, b.id, "re-adding returns the same row");
        assert_eq!(list_folders(&conn).unwrap().len(), 1);
    }

    #[test]
    fn add_folder_rejects_missing_path() {
        let conn = fresh_conn();
        let result = add_folder(&conn, "/path/that/really/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn add_folder_rejects_file() {
        let conn = fresh_conn();
        let dir = tmpdir("isfile");
        let file = dir.join("extrato.ofx");
        std::fs::write(&file, b"x").unwrap();
        let result = add_folder(&conn, file.to_str().unwrap());
        assert!(result.is_err(), "a file cannot be added as a folder");
    }

    #[test]
    fn missing_folder_reports_exists_false() {
        let conn = fresh_conn();
        let dir = tmpdir("gone");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert!(!folders[0].exists, "a folder that is gone must report exists=false");
    }

    #[test]
    fn update_folder_path_preserves_row_id() {
        let conn = fresh_conn();
        let old = tmpdir("old");
        let new = tmpdir("new");
        let created = add_folder(&conn, old.to_str().unwrap()).unwrap();

        let moved = update_folder_path(&conn, created.id, new.to_str().unwrap()).unwrap();
        assert_eq!(moved.id, created.id, "Localizar… preserva o registro");
        assert_eq!(moved.label, "finoinc-watch-test-new");
        assert_eq!(list_folders(&conn).unwrap().len(), 1);
    }

    #[test]
    fn remove_folder_deletes_row() {
        let conn = fresh_conn();
        let dir = tmpdir("rm");
        let folder = add_folder(&conn, dir.to_str().unwrap()).unwrap();
        remove_folder(&conn, folder.id).unwrap();
        assert!(list_folders(&conn).unwrap().is_empty());
    }

    #[test]
    fn imported_count_reflects_seen_files_in_folder() {
        let conn = fresh_conn();
        let dir = tmpdir("count");
        let folder = add_folder(&conn, dir.to_str().unwrap()).unwrap();

        conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status, resolved_at)
             VALUES ('h1', ?1, 'a.ofx', 10, 'imported', '2026-07-02')",
            params![format!("{}/a.ofx", folder.path)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status, resolved_at)
             VALUES ('h2', ?1, 'b.ofx', 10, 'ignored', '2026-07-03')",
            params![format!("{}/b.ofx", folder.path)],
        )
        .unwrap();

        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders[0].imported_count, 1, "only 'imported' counts");
        assert_eq!(folders[0].last_imported_at.as_deref(), Some("2026-07-02"));
    }

    #[test]
    fn ensure_dir_creates_and_is_idempotent() {
        let dir = tmpdir("ensure").join("finoinc");
        assert!(!dir.exists());

        ensure_dir(dir.to_string_lossy().to_string()).unwrap();
        assert!(dir.is_dir());

        // Running again on an existing folder must not fail.
        ensure_dir(dir.to_string_lossy().to_string()).unwrap();
        assert!(dir.is_dir());
    }

    #[test]
    fn dir_exists_distinguishes_directories_from_files() {
        let dir = tmpdir("dir-exists");
        assert!(dir_exists(dir.to_string_lossy().to_string()), "existing folder");

        let nonexistent = dir.join("nonexistent");
        assert!(!dir_exists(nonexistent.to_string_lossy().to_string()), "missing path");

        let file = dir.join("test.txt");
        std::fs::write(&file, b"test").unwrap();
        assert!(!dir_exists(file.to_string_lossy().to_string()), "a file, not a folder");
    }

    use std::time::{Duration, SystemTime};

    /// Writes a file with an mtime old enough not to be deferred.
    fn write_old(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    /// `now` shifted into the future simulates a file written long ago,
    /// without sleeping in the test.
    fn later() -> SystemTime {
        SystemTime::now() + Duration::from_secs(60)
    }

    #[test]
    fn scan_finds_ofx_and_ignores_other_extensions() {
        let dir = tmpdir("scan-ext");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        write_old(&dir, "foto.png", "binario");
        write_old(&dir, "notas.txt", "texto");

        let outcome = scan_dir(&dir, later());
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(outcome.candidates[0].file_name, "extrato.ofx");
    }

    #[test]
    fn scan_accepts_uppercase_extension() {
        let dir = tmpdir("scan-upper");
        write_old(&dir, "EXTRATO.OFX", "OFXDATA");

        let outcome = scan_dir(&dir, later());
        assert_eq!(outcome.candidates.len(), 1);
    }

    #[test]
    fn scan_hashes_content_so_renames_collide() {
        let dir = tmpdir("scan-hash");
        write_old(&dir, "a.ofx", "SAME CONTENT");
        write_old(&dir, "b.ofx", "SAME CONTENT");

        let outcome = scan_dir(&dir, later());
        assert_eq!(outcome.candidates.len(), 2);
        assert_eq!(
            outcome.candidates[0].content_hash, outcome.candidates[1].content_hash,
            "same content, same hash — dedup happens on INSERT"
        );
    }

    #[test]
    fn scan_defers_files_written_moments_ago() {
        let dir = tmpdir("scan-fresh");
        write_old(&dir, "chegando.ofx", "OFXDATA");

        // `now` = now: the file's mtime is milliseconds old.
        let outcome = scan_dir(&dir, SystemTime::now());
        assert!(
            outcome.candidates.is_empty(),
            "a file still being written (AirDrop/download) must be deferred"
        );
    }

    #[test]
    fn scan_all_defers_files_written_moments_ago() {
        // Same protection as the test above, but at the entry point the UI
        // actually calls: if `scan_all` invented its own fast-forwarded `now`
        // (as it once did), this guard would never fire and a file still in
        // transfer would be registered as pending.
        let conn = fresh_conn();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();
        let dir = tmpdir("scan-all-fresh");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        write_old(&dir, "chegando.ofx", "OFXDATA");

        // Real `now`, not `later()`: the file's mtime is milliseconds old.
        let found = scan_all(&conn, SystemTime::now()).unwrap();
        assert!(
            found.is_empty(),
            "a just-written file must not be registered as pending"
        );
    }

    #[test]
    fn scan_reports_icloud_stubs_without_treating_them_as_candidates() {
        let dir = tmpdir("scan-icloud");
        // iCloud placeholder: leading dot and an .icloud suffix.
        write_old(&dir, ".Nubank_2026-07.ofx.icloud", "stub");

        let outcome = scan_dir(&dir, later());
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.icloud_pending, 1, "a stub becomes information, not an error");
    }

    #[test]
    fn scan_ignores_icloud_stub_of_non_ofx() {
        let dir = tmpdir("scan-icloud-other");
        write_old(&dir, ".ferias.jpg.icloud", "stub");

        let outcome = scan_dir(&dir, later());
        assert_eq!(outcome.icloud_pending, 0);
    }

    #[test]
    fn scan_of_missing_dir_is_empty_not_panic() {
        let dir = tmpdir("scan-gone");
        std::fs::remove_dir_all(&dir).unwrap();

        let outcome = scan_dir(&dir, later());
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.icloud_pending, 0);
    }

    #[test]
    fn scan_all_returns_nothing_when_disabled() {
        let conn = fresh_conn();
        let dir = tmpdir("disabled");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        // watch_enabled ausente = desligado (default)

        assert!(scan_all(&conn, later()).unwrap().is_empty());
    }

    #[test]
    fn scan_all_registers_new_files_as_pending() {
        let conn = fresh_conn();
        let dir = tmpdir("scan-all");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let found = scan_all(&conn, later()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name, "extrato.ofx");
        assert_eq!(found[0].status, "pending");
    }

    #[test]
    fn scan_all_records_icloud_pending_count() {
        let conn = fresh_conn();
        let dir = tmpdir("icloud-count");
        write_old(&dir, ".Nubank_2026-07.ofx.icloud", "stub");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        scan_all(&conn, later()).unwrap();

        assert_eq!(
            crate::commands::app_settings::get_setting(&conn, ICLOUD_PENDING_KEY).unwrap(),
            Some("1".to_string())
        );
    }

    #[test]
    fn scan_all_records_last_scan_timestamp() {
        let conn = fresh_conn();
        let dir = tmpdir("last-scan");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        scan_all(&conn, later()).unwrap();

        let stamp = crate::commands::app_settings::get_setting(&conn, LAST_SCAN_KEY).unwrap();
        assert!(stamp.is_some(), "Settings shows when the last scan ran");
    }

    #[test]
    fn disabled_scan_writes_nothing() {
        // Disabled, the feature is inert: it touches neither disk nor the KV.
        let conn = fresh_conn();
        let dir = tmpdir("disabled-inert");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();

        assert!(scan_targets(&conn).unwrap().is_none());
        scan_all(&conn, later()).unwrap();

        assert_eq!(
            crate::commands::app_settings::get_setting(&conn, LAST_SCAN_KEY).unwrap(),
            None
        );
        assert_eq!(
            crate::commands::app_settings::get_setting(&conn, ICLOUD_PENDING_KEY).unwrap(),
            None
        );
    }

    #[test]
    fn scan_paths_merges_every_folder() {
        // The disk phase takes the whole folder list and returns a single
        // result — this is the phase that runs outside the DB mutex.
        let a = tmpdir("merge-a");
        let b = tmpdir("merge-b");
        write_old(&a, "a.ofx", "CONTEUDO A");
        write_old(&b, "b.ofx", "CONTEUDO B");
        write_old(&b, ".Nubank.ofx.icloud", "stub");

        let paths = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        let outcome = scan_paths(&paths, later());

        assert_eq!(outcome.candidates.len(), 2);
        assert_eq!(outcome.icloud_pending, 1);
    }

    #[test]
    fn scan_all_does_not_resurface_resolved_files() {
        let conn = fresh_conn();
        let dir = tmpdir("scan-resolved");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let found = scan_all(&conn, later()).unwrap();
        mark(&conn, &found[0].content_hash, "imported").unwrap();

        assert!(
            scan_all(&conn, later()).unwrap().is_empty(),
            "an already-resolved file does not come back"
        );
    }

    #[test]
    fn scan_all_dedups_same_content_under_another_name() {
        let conn = fresh_conn();
        let dir = tmpdir("scan-dedup");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let first = scan_all(&conn, later()).unwrap();
        mark(&conn, &first[0].content_hash, "imported").unwrap();

        // Same statement, downloaded again under a different name.
        write_old(&dir, "extrato (1).ofx", "OFXDATA");
        assert!(
            scan_all(&conn, later()).unwrap().is_empty(),
            "the content hash was already seen"
        );
    }

    #[test]
    fn scan_all_updates_path_when_file_moves() {
        let conn = fresh_conn();
        let a = tmpdir("move-a");
        let b = tmpdir("move-b");
        write_old(&a, "extrato.ofx", "OFXDATA");
        add_folder(&conn, a.to_str().unwrap()).unwrap();
        add_folder(&conn, b.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let first = scan_all(&conn, later()).unwrap();
        assert_eq!(first.len(), 1);

        std::fs::rename(a.join("extrato.ofx"), b.join("extrato.ofx")).unwrap();
        let second = scan_all(&conn, later()).unwrap();

        assert_eq!(second.len(), 1, "still a single pending file");
        assert!(
            second[0].path.contains("move-b"),
            "the path is updated to where the file is now"
        );
    }

    #[test]
    fn mark_sets_resolved_at() {
        let conn = fresh_conn();
        let dir = tmpdir("mark");
        write_old(&dir, "extrato.ofx", "OFXDATA");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let found = scan_all(&conn, later()).unwrap();
        mark(&conn, &found[0].content_hash, "ignored").unwrap();

        let (status, resolved): (String, Option<String>) = conn
            .query_row(
                "SELECT status, resolved_at FROM seen_files WHERE content_hash = ?1",
                params![found[0].content_hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "ignored");
        assert!(resolved.is_some());
    }

    #[test]
    fn mark_rejects_unknown_status() {
        let conn = fresh_conn();
        assert!(mark(&conn, "qualquer", "garbage").is_err());
    }

    #[test]
    fn pending_files_lists_only_pending() {
        let conn = fresh_conn();
        let dir = tmpdir("pending-only");
        write_old(&dir, "a.ofx", "CONTEUDO A");
        write_old(&dir, "b.ofx", "CONTEUDO B");
        add_folder(&conn, dir.to_str().unwrap()).unwrap();
        crate::commands::app_settings::set_setting(&conn, "watch_enabled", "1").unwrap();

        let found = scan_all(&conn, later()).unwrap();
        assert_eq!(found.len(), 2);
        mark(&conn, &found[0].content_hash, "invalid").unwrap();

        assert_eq!(pending_files(&conn).unwrap().len(), 1);
    }
}
