use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::types::Value;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::locale::LocalePack;

pub mod migrations;

pub struct Db {
    pub conn: Mutex<Connection>,
    pub path: PathBuf,
    /// `true` when this process created a brand-new database. The caller seeds
    /// a fresh DB from the active locale pack; existing DBs are left untouched.
    pub fresh: bool,
}

const DB_FILE: &str = "finoinc.db";
/// Data dir and database name from before the app was renamed to Finoinc.
const LEGACY_DIR: &str = "app.finan";
const LEGACY_DB_FILE: &str = "finan.db";

/// Copies the pre-rename data dir into the current one on first launch, so an
/// existing user keeps their data after the bundle identifier changed. The
/// legacy dir is left untouched as a backup. No-op once the new DB exists.
pub fn adopt_legacy_data(app: &AppHandle) -> AppResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Path(e.to_string()))?;
    copy_legacy_dir(&dir.with_file_name(LEGACY_DIR), &dir)
}

fn copy_legacy_dir(legacy: &Path, dir: &Path) -> AppResult<()> {
    if dir.join(DB_FILE).exists() || !legacy.join(LEGACY_DB_FILE).is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    for entry in std::fs::read_dir(legacy)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_file() || name == ".DS_Store" || name == LEGACY_DB_FILE {
            continue;
        }
        let target = match name.strip_prefix(LEGACY_DB_FILE) {
            Some(suffix) => format!("{DB_FILE}{suffix}"),
            None => name,
        };
        std::fs::copy(entry.path(), dir.join(target))?;
    }
    // The main DB goes last, through a rename: its presence is what marks the
    // copy as done, so an interrupted copy is redone on the next launch.
    let tmp = dir.join(format!("{DB_FILE}.partial"));
    std::fs::copy(legacy.join(LEGACY_DB_FILE), &tmp)?;
    std::fs::rename(&tmp, dir.join(DB_FILE))?;
    Ok(())
}

pub fn init(app: &AppHandle) -> AppResult<Db> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Path(e.to_string()))?;

    std::fs::create_dir_all(&dir)?;
    let path = dir.join(DB_FILE);

    let conn = Connection::open(&path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let fresh = migrations::apply(&conn)?;

    Ok(Db {
        conn: Mutex::new(conn),
        path,
        fresh,
    })
}

/// Seed a **fresh** database from the active locale pack. Categories are matched
/// by their stable `key`: a row with a matching key is updated in place, and a
/// key with no matching row is inserted. Migrations seed no rows of their own
/// (a fresh DB's `categories` table starts empty), so every call currently
/// takes the insert path — but matching by key first keeps the function safe
/// to call again against a DB that already has some category rows, instead of
/// hitting a UNIQUE violation. Seed rules are inserted mapped by category key,
/// skipping patterns already present. Safe to call only on a fresh DB (never
/// overwrites user edits).
pub fn seed_from_pack(conn: &Connection, pack: &LocalePack) -> AppResult<()> {
    for c in &pack.categories {
        let updated = conn.execute(
            "UPDATE categories
                SET name = ?2, color_token = ?3, kind = ?4, is_investment = ?5
              WHERE key = ?1",
            params![c.key, c.name, c.color_token, c.kind, c.is_investment],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO categories (key, name, color_token, kind, is_investment)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![c.key, c.name, c.color_token, c.kind, c.is_investment],
            )?;
        }
    }

    for r in &pack.rules.seed_rules {
        // A seeded rule starts with a single snippet; more are added from the
        // Rules screen.
        let inserted = conn.execute(
            "INSERT INTO rules (category_id, priority, due_day, display_name)
             SELECT c.id, ?1, NULL, ?2 FROM categories c
             WHERE c.key = ?3
               AND NOT EXISTS (
                   SELECT 1 FROM rule_patterns WHERE pattern = ?4
               )",
            params![r.priority, r.display_name, r.category, r.pattern],
        )?;
        if inserted > 0 {
            conn.execute(
                "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, ?2)",
                params![conn.last_insert_rowid(), r.pattern],
            )?;
        }
    }

    Ok(())
}

/// True when the database is still exactly what `seeded_from` put there:
/// nothing imported, no bill marked paid, and every category and rule as the
/// pack seeded it. Anything the user set up before their first import — a
/// renamed category, a rule with a due day — makes it not pristine, so a
/// language switch never reseeds over it.
pub fn is_pristine(conn: &Connection, seeded_from: &LocalePack) -> AppResult<bool> {
    for table in ["accounts", "transactions", "bill_settlements"] {
        let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        if rows > 0 {
            return Ok(false);
        }
    }
    let expected = Connection::open_in_memory()?;
    migrations::apply(&expected)?;
    seed_from_pack(&expected, seeded_from)?;
    Ok(seed_snapshot(conn)? == seed_snapshot(&expected)?)
}

/// Categories and rules without their row ids, which differ between two
/// databases seeded from the same pack.
fn seed_snapshot(conn: &Connection) -> AppResult<Vec<Vec<Value>>> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT key, name, color_token, kind, is_investment FROM categories ORDER BY key, name",
        "SELECT c.key, r.priority, r.due_day, r.display_name, r.pay_lead_months,
                (SELECT group_concat(pattern, char(31))
                   FROM (SELECT pattern FROM rule_patterns WHERE rule_id = r.id ORDER BY pattern))
           FROM rules r JOIN categories c ON c.id = r.category_id
          ORDER BY 1, 6, 2, 3, 4, 5",
    ] {
        let mut stmt = conn.prepare(sql)?;
        let width = stmt.column_count();
        let mut q = stmt.query([])?;
        while let Some(row) = q.next()? {
            rows.push((0..width).map(|i| row.get(i)).collect::<rusqlite::Result<_>>()?);
        }
    }
    Ok(rows)
}

/// Replace pack-seeded data wholesale. Safe ONLY on a pristine DB — the caller
/// checks. Renaming categories by key is not enough on its own: the previous
/// pack's seed rules would survive and the new pack's would never be inserted.
///
/// Runs as one transaction: the three deletes and `seed_from_pack`'s inserts
/// either all land or none do. Without this, a failure partway (a malformed
/// pack, a full disk) could leave the database with zero categories and zero
/// rules — worse than either the old or the new pack — with no way back
/// except another successful reseed.
pub fn reseed_from_pack(conn: &mut Connection, pack: &LocalePack) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM rule_patterns", [])?;
    tx.execute("DELETE FROM rules", [])?;
    tx.execute("DELETE FROM categories", [])?;
    seed_from_pack(&tx, pack)?;
    tx.commit()?;
    Ok(())
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

    fn tmpdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("finoinc-db-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copy_legacy_dir_renames_db_files_and_keeps_the_legacy_dir() {
        let root = tmpdir("legacy");
        let legacy = root.join("app.finan");
        std::fs::create_dir_all(&legacy).unwrap();
        for (name, body) in [("finan.db", "db"), ("finan.db-wal", "wal"), ("active_locale", "pt-BR")] {
            std::fs::write(legacy.join(name), body).unwrap();
        }
        let dir = root.join("app.finoinc");

        copy_legacy_dir(&legacy, &dir).unwrap();

        assert_eq!(std::fs::read_to_string(dir.join("finoinc.db")).unwrap(), "db");
        assert_eq!(std::fs::read_to_string(dir.join("finoinc.db-wal")).unwrap(), "wal");
        assert_eq!(std::fs::read_to_string(dir.join("active_locale")).unwrap(), "pt-BR");
        assert!(!dir.join("finoinc.db.partial").exists());
        assert!(legacy.join("finan.db").exists(), "legacy dir stays as a backup");
    }

    #[test]
    fn copy_legacy_dir_never_overwrites_an_existing_db() {
        let root = tmpdir("existing");
        let legacy = root.join("app.finan");
        let dir = root.join("app.finoinc");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(legacy.join("finan.db"), "old").unwrap();
        std::fs::write(dir.join("finoinc.db"), "current").unwrap();

        copy_legacy_dir(&legacy, &dir).unwrap();

        assert_eq!(std::fs::read_to_string(dir.join("finoinc.db")).unwrap(), "current");
    }

    #[test]
    fn copy_legacy_dir_is_a_no_op_without_legacy_data() {
        let root = tmpdir("none");
        let dir = root.join("app.finoinc");

        copy_legacy_dir(&root.join("app.finan"), &dir).unwrap();

        assert!(!dir.exists());
    }

    fn en_us_pack() -> LocalePack {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../locales/en-US");
        LocalePack::load_from_dir(&dir).expect("en-US pack must load")
    }

    #[test]
    fn is_pristine_true_on_fresh_seeded_db() {
        let conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();
        assert!(is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn is_pristine_false_after_inserting_an_account() {
        let conn = fresh_conn();
        conn.execute("INSERT INTO accounts (name) VALUES ('Checking')", [])
            .unwrap();
        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn is_pristine_false_after_inserting_a_transaction() {
        let conn = fresh_conn();
        conn.execute("INSERT INTO accounts (name) VALUES ('Checking')", [])
            .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description)
             VALUES (?1, '2026-01-01', '10.00', 'x')",
            params![account_id],
        )
        .unwrap();
        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn is_pristine_false_when_a_user_created_category_exists() {
        let conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();

        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Viagem', NULL, 'expense')",
            [],
        )
        .unwrap();

        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    /// Before any import a user can already have set things up; a language
    /// switch reseeding over that would silently wipe it.
    #[test]
    fn is_pristine_false_once_a_seeded_category_is_renamed() {
        let conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();
        conn.execute("UPDATE categories SET name = 'Feira' WHERE key = 'market'", [])
            .unwrap();
        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn is_pristine_false_once_a_seeded_rule_gets_a_due_day() {
        let conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();
        conn.execute("UPDATE rules SET due_day = 10 WHERE id = (SELECT MIN(id) FROM rules)", [])
            .unwrap();
        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn is_pristine_false_once_a_bill_is_marked_paid() {
        let conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();
        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month, transaction_id)
             VALUES ((SELECT MIN(id) FROM rules), '2026-10', NULL)",
            [],
        )
        .unwrap();
        assert!(!is_pristine(&conn, &LocalePack::embedded_pt_br()).unwrap());
    }

    #[test]
    fn reseed_from_pack_replaces_pt_br_with_en_us() {
        let mut conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();

        reseed_from_pack(&mut conn, &en_us_pack()).unwrap();

        let names: Vec<String> = conn
            .prepare("SELECT name FROM categories ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let en_names: Vec<String> = en_us_pack().categories.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names.len(), en_names.len());
        for name in &en_names {
            assert!(names.contains(name), "missing en-US category name {name:?}");
        }
        for name in &names {
            assert!(!name.contains("Mercado"), "a pt-BR category name survived: {name:?}");
        }

        let patterns: std::collections::BTreeSet<String> = conn
            .prepare("SELECT pattern FROM rule_patterns")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // Only the en-US seed patterns should be present: a pt-BR-only pattern
        // (unique to the old pack) surviving would mean the delete step failed,
        // and a missing en-US pattern would mean the reseed step failed.
        let en_patterns: std::collections::BTreeSet<String> = en_us_pack()
            .rules
            .seed_rules
            .iter()
            .map(|r| r.pattern.clone())
            .collect();
        assert_eq!(patterns, en_patterns);
    }

    #[test]
    fn reseed_from_pack_twice_is_stable() {
        let mut conn = fresh_conn();
        seed_from_pack(&conn, &LocalePack::embedded_pt_br()).unwrap();

        reseed_from_pack(&mut conn, &en_us_pack()).unwrap();
        reseed_from_pack(&mut conn, &en_us_pack()).unwrap();

        let category_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(category_count as usize, en_us_pack().categories.len());

        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rule_patterns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pattern_count as usize, en_us_pack().rules.seed_rules.len());
    }

    /// Proves the rollback, not just that a transaction wrapper exists: a
    /// pack with one category violating the `kind` CHECK constraint fails
    /// partway through `seed_from_pack`, after the deletes already ran (in
    /// the same, still-uncommitted transaction) and after several earlier
    /// categories in the broken pack were already (re)inserted. The original
    /// pt-BR data must still be there afterward, untouched.
    #[test]
    fn reseed_from_pack_rolls_back_a_failure_partway() {
        let mut conn = fresh_conn();
        let original = LocalePack::embedded_pt_br();
        seed_from_pack(&conn, &original).unwrap();

        let mut broken = original.clone();
        assert!(
            broken.categories.len() > 1,
            "the pt-BR pack needs at least two categories for this test to be meaningful"
        );
        broken.categories[1].kind = "bogus".to_string();

        let result = reseed_from_pack(&mut conn, &broken);
        assert!(
            result.is_err(),
            "a CHECK-constraint violation on `kind` must surface as an error"
        );

        let names: std::collections::BTreeSet<String> = conn
            .prepare("SELECT name FROM categories")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let original_names: std::collections::BTreeSet<String> =
            original.categories.iter().map(|c| c.name.clone()).collect();
        assert_eq!(
            names, original_names,
            "a failed reseed must leave the pre-reseed categories intact, not an empty table"
        );

        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rule_patterns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            pattern_count as usize,
            original.rules.seed_rules.len(),
            "a failed reseed must leave the pre-reseed rules intact, not an empty table"
        );
    }
}
