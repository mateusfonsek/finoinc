use rusqlite::Connection;

use crate::error::AppResult;

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../../migrations/0001_init.sql")),
    (
        "0002_rules",
        include_str!("../../migrations/0002_rules.sql"),
    ),
    (
        "0003_rules_due_day",
        include_str!("../../migrations/0003_rules_due_day.sql"),
    ),
    (
        "0004_rules_display_name",
        include_str!("../../migrations/0004_rules_display_name.sql"),
    ),
    (
        "0005_compras_category",
        include_str!("../../migrations/0005_compras_category.sql"),
    ),
    (
        "0006_transfer_category",
        include_str!("../../migrations/0006_transfer_category.sql"),
    ),
    (
        "0007_investments",
        include_str!("../../migrations/0007_investments.sql"),
    ),
    (
        "0008_delete_renda_category",
        include_str!("../../migrations/0008_delete_renda_category.sql"),
    ),
    (
        "0009_investment_green_color",
        include_str!("../../migrations/0009_investment_green_color.sql"),
    ),
    (
        "0010_drop_income_cnpj_rules",
        include_str!("../../migrations/0010_drop_income_cnpj_rules.sql"),
    ),
    (
        "0011_account_kind",
        include_str!("../../migrations/0011_account_kind.sql"),
    ),
    (
        "0012_cc_seed",
        include_str!("../../migrations/0012_cc_seed.sql"),
    ),
    (
        "0013_composite_fitid_unique",
        include_str!("../../migrations/0013_composite_fitid_unique.sql"),
    ),
    (
        "0014_category_keys",
        include_str!("../../migrations/0014_category_keys.sql"),
    ),
    (
        "0015_education_pets_categories",
        include_str!("../../migrations/0015_education_pets_categories.sql"),
    ),
    (
        "0016_watched_folders",
        include_str!("../../migrations/0016_watched_folders.sql"),
    ),
    (
        "0017_rule_patterns",
        include_str!("../../migrations/0017_rule_patterns.sql"),
    ),
    (
        "0018_bill_settlements",
        include_str!("../../migrations/0018_bill_settlements.sql"),
    ),
    (
        "0019_categorized_by_rule",
        include_str!("../../migrations/0019_categorized_by_rule.sql"),
    ),
];

/// Applies pending migrations. Returns `true` when this call created a **brand
/// new** database (i.e. `0001_init` had not run before), so the caller can seed
/// it from the active locale pack without clobbering an existing user's data.
pub fn apply(conn: &Connection) -> AppResult<bool> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let already_initialized: bool = conn
        .query_row(
            "SELECT 1 FROM _migrations WHERE name = '0001_init'",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);
    let fresh_db = !already_initialized;

    for (name, sql) in MIGRATIONS {
        let already: bool = conn
            .query_row(
                "SELECT 1 FROM _migrations WHERE name = ?1",
                [name],
                |_row| Ok(true),
            )
            .unwrap_or(false);

        if !already {
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO _migrations (name) VALUES (?1)", [name])?;
        }
    }

    Ok(fresh_db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |row| row.get::<_, String>(0),
        )
        .is_ok()
    }

    /// Applies migrations only up to `up_to`, so a test can build an old-format
    /// DB and see what the next migration does to it.
    fn apply_through(conn: &Connection, up_to: &str) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                name TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        for (name, sql) in MIGRATIONS {
            conn.execute_batch(sql).unwrap();
            conn.execute("INSERT INTO _migrations (name) VALUES (?1)", [name])
                .unwrap();
            if *name == up_to {
                return;
            }
        }
        panic!("migration {up_to} not found");
    }

    /// 0017 moves `rules.pattern` into `rule_patterns`. Existing rules must
    /// survive as single-snippet rules.
    #[test]
    fn rule_patterns_migration_preserves_existing_rules() {
        let conn = Connection::open_in_memory().unwrap();
        apply_through(&conn, "0016_watched_folders");

        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('X', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();

        // Old format: pattern was a column on the rule itself.
        conn.execute(
            "INSERT INTO rules (pattern, category_id, priority, due_day) VALUES ('legado', ?1, 7, 5)",
            [cat],
        )
        .unwrap();
        let rule_id = conn.last_insert_rowid();

        apply(&conn).unwrap();

        let patterns: Vec<String> = conn
            .prepare("SELECT pattern FROM rule_patterns WHERE rule_id = ?1 ORDER BY id")
            .unwrap()
            .query_map([rule_id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(patterns, vec!["legado".to_string()]);

        // The rest of the rule is untouched...
        let (priority, due_day): (i32, Option<i32>) = conn
            .query_row(
                "SELECT priority, due_day FROM rules WHERE id = ?1",
                [rule_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((priority, due_day), (7, Some(5)));

        // ...and the old column is gone, so it cannot become a second source
        // of truth.
        assert!(conn
            .query_row("SELECT pattern FROM rules WHERE id = ?1", [rule_id], |r| r
                .get::<_, String>(0))
            .is_err());
    }

    /// Migrations create schema; the active locale pack creates data. Without this
    /// separation a fresh database in any language is born with Brazilian rows,
    /// because migrations run before the pack is even read.
    #[test]
    fn fresh_db_has_no_seed_data() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        for table in ["categories", "rules", "rule_patterns"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0, "{table} must be empty: seeding belongs to the pack");
        }
    }

    #[test]
    fn applies_init_migration() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        assert!(table_exists(&conn, "accounts"));
        assert!(table_exists(&conn, "categories"));
        assert!(table_exists(&conn, "transactions"));
        assert!(table_exists(&conn, "_migrations"));
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();
        apply(&conn).unwrap();

        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(applied as usize, MIGRATIONS.len(), "each migration runs once");
    }

    #[test]
    fn applies_rules_migration() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        assert!(table_exists(&conn, "rules"));
        let applied: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM _migrations ORDER BY name")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            applied,
            vec![
                "0001_init".to_string(),
                "0002_rules".to_string(),
                "0003_rules_due_day".to_string(),
                "0004_rules_display_name".to_string(),
                "0005_compras_category".to_string(),
                "0006_transfer_category".to_string(),
                "0007_investments".to_string(),
                "0008_delete_renda_category".to_string(),
                "0009_investment_green_color".to_string(),
                "0010_drop_income_cnpj_rules".to_string(),
                "0011_account_kind".to_string(),
                "0012_cc_seed".to_string(),
                "0013_composite_fitid_unique".to_string(),
                "0014_category_keys".to_string(),
                "0015_education_pets_categories".to_string(),
                "0016_watched_folders".to_string(),
                "0017_rule_patterns".to_string(),
                "0018_bill_settlements".to_string(),
                "0019_categorized_by_rule".to_string(),
            ]
        );
    }

    /// 0014 backfills `key` by the Portuguese name the earlier migrations seeded.
    /// A fresh database has no such rows any more, so the state has to be built.
    #[test]
    fn backfills_category_keys() {
        let conn = Connection::open_in_memory().unwrap();
        apply_through(&conn, "0013_composite_fitid_unique");

        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Mercado', '--color-cat-mercado', 'expense')",
            [],
        )
        .unwrap();

        apply(&conn).unwrap();

        let key: String = conn
            .query_row("SELECT key FROM categories WHERE name = 'Mercado'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(key, "market");
    }

    #[test]
    fn reports_fresh_db_only_on_first_apply() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(apply(&conn).unwrap(), "first apply means a fresh DB");
        assert!(!apply(&conn).unwrap(), "reapplying is not a fresh DB");
    }

    #[test]
    fn creates_watch_tables() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        assert!(table_exists(&conn, "app_settings"));
        assert!(table_exists(&conn, "watched_folders"));
        assert!(table_exists(&conn, "seen_files"));
    }

    #[test]
    fn seen_files_rejects_unknown_status() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        let result = conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status)
             VALUES ('abc', '/tmp/x.ofx', 'x.ofx', 10, 'garbage')",
            [],
        );
        assert!(result.is_err(), "CHECK should reject an invalid status");
    }

    #[test]
    fn seen_files_hash_is_unique() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status)
             VALUES ('abc', '/tmp/a.ofx', 'a.ofx', 10, 'pending')",
            [],
        )
        .unwrap();
        // Same content under another name/path: must collide.
        let result = conn.execute(
            "INSERT INTO seen_files (content_hash, path, file_name, size, status)
             VALUES ('abc', '/tmp/b.ofx', 'b.ofx', 10, 'pending')",
            [],
        );
        assert!(result.is_err(), "the same content under another name must collide");
    }

    #[test]
    fn watched_folders_path_is_unique() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        conn.execute(
            "INSERT INTO watched_folders (path, label) VALUES ('/tmp/finoinc', 'finoinc')",
            [],
        )
        .unwrap();
        let result = conn.execute(
            "INSERT INTO watched_folders (path, label) VALUES ('/tmp/finoinc', 'outro')",
            [],
        );
        assert!(result.is_err(), "the same folder cannot be added twice");
    }

    #[test]
    fn creates_bill_settlements_table() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        assert!(table_exists(&conn, "bill_settlements"));
    }

    #[test]
    fn rules_default_to_paying_in_the_month_they_are_due() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('X', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day) VALUES (?1, 0, 6)",
            [cat],
        )
        .unwrap();

        let lead: i64 = conn
            .query_row("SELECT pay_lead_months FROM rules", [], |r| r.get(0))
            .unwrap();
        assert_eq!(lead, 0, "an existing rule must keep behaving as it did");
    }

    /// One row per occurrence: settling the same bill twice must collide rather
    /// than leave two rows disagreeing about the same month.
    #[test]
    fn a_bill_is_settled_at_most_once_per_month() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('X', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day) VALUES (?1, 0, 6)",
            [cat],
        )
        .unwrap();
        let rule = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month) VALUES (?1, '2026-08')",
            [rule],
        )
        .unwrap();
        let again = conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month) VALUES (?1, '2026-08')",
            [rule],
        );

        assert!(again.is_err());
    }

    /// The settlement is the user's statement that the bill is paid. Losing the
    /// transaction must not silently unpay it.
    #[test]
    fn deleting_a_rule_drops_its_settlements() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('X', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day) VALUES (?1, 0, 6)",
            [cat],
        )
        .unwrap();
        let rule = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month) VALUES (?1, '2026-08')",
            [rule],
        )
        .unwrap();

        conn.execute("DELETE FROM rules WHERE id = ?1", [rule]).unwrap();

        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM bill_settlements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }
}
