use rusqlite::params;
use tauri::State;

use crate::db::Db;
use crate::domain::category::{Category, CategoryWithCount, NewCategory, UpdateCategory};
use crate::error::{AppError, AppResult};

pub fn all(conn: &rusqlite::Connection) -> AppResult<Vec<Category>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color_token, kind, is_investment, created_at FROM categories ORDER BY kind, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Category {
            id: row.get(0)?,
            name: row.get(1)?,
            color_token: row.get(2)?,
            kind: row.get(3)?,
            is_investment: row.get::<_, i64>(4)? != 0,
            created_at: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub fn list_categories(db: State<'_, Db>) -> AppResult<Vec<Category>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    all(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn create_category(db: State<'_, Db>, input: NewCategory) -> AppResult<Category> {
    if !matches!(input.kind.as_str(), "expense" | "income" | "transfer") {
        return Err(AppError::Invalid(format!(
            "invalid kind '{}' (must be expense|income|transfer)",
            input.kind
        )));
    }
    let conn = db.conn.lock().expect("db mutex poisoned");
    conn.execute(
        "INSERT INTO categories (name, color_token, kind) VALUES (?1, ?2, ?3)",
        params![input.name, input.color_token, input.kind],
    )?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, name, color_token, kind, is_investment, created_at FROM categories WHERE id = ?1",
        params![id],
        |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                color_token: row.get(2)?,
                kind: row.get(3)?,
                is_investment: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
            })
        },
    )
    .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub fn list_categories_with_count(db: State<'_, Db>) -> AppResult<Vec<CategoryWithCount>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.color_token, c.kind, c.created_at,
                COUNT(t.id) AS tx_count
         FROM categories c
         LEFT JOIN transactions t ON t.category_id = c.id
         GROUP BY c.id
         ORDER BY c.kind, c.name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CategoryWithCount {
            id: row.get(0)?,
            name: row.get(1)?,
            color_token: row.get(2)?,
            kind: row.get(3)?,
            created_at: row.get(4)?,
            transaction_count: row.get::<_, i64>(5)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub fn update_category(
    db: State<'_, Db>,
    category_id: i64,
    input: UpdateCategory,
) -> AppResult<Category> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("name must not be empty".into()));
    }
    if !matches!(input.kind.as_str(), "expense" | "income" | "transfer") {
        return Err(AppError::Invalid(format!("invalid kind '{}'", input.kind)));
    }
    let conn = db.conn.lock().expect("db mutex poisoned");
    let changed = conn.execute(
        "UPDATE categories SET name = ?1, color_token = ?2, kind = ?3 WHERE id = ?4",
        params![input.name.trim(), input.color_token, input.kind, category_id],
    )?;
    if changed == 0 {
        return Err(AppError::Invalid(format!(
            "category {category_id} not found"
        )));
    }
    conn.query_row(
        "SELECT id, name, color_token, kind, is_investment, created_at FROM categories WHERE id = ?1",
        params![category_id],
        |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                color_token: row.get(2)?,
                kind: row.get(3)?,
                is_investment: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
            })
        },
    )
    .map_err(AppError::from)
}

/// Deletes a category and its rules. Transactions referencing it get
/// category_id=NULL.
#[tauri::command]
#[specta::specta]
pub fn delete_category(db: State<'_, Db>, category_id: i64) -> AppResult<()> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    delete(&mut conn, category_id)
}

fn delete(conn: &mut rusqlite::Connection, category_id: i64) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE transactions SET category_id = NULL, categorized_by_rule = NULL
          WHERE category_id = ?1",
        params![category_id],
    )?;
    // A rule cannot exist without its category; its snippets and bill
    // settlements go with it through ON DELETE CASCADE.
    tx.execute("DELETE FROM rules WHERE category_id = ?1", params![category_id])?;
    let deleted = tx.execute(
        "DELETE FROM categories WHERE id = ?1",
        params![category_id],
    )?;
    if deleted == 0 {
        return Err(AppError::Invalid(format!(
            "category {category_id} not found"
        )));
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::migrations;
    use rusqlite::{params, Connection};

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    #[test]
    fn seed_categories_listed() {
        let conn = fresh_conn();
        let mut stmt = conn
            .prepare("SELECT name, kind FROM categories ORDER BY name")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows.len(), 13);
        assert!(rows.iter().any(|(n, _)| n == "Mercado"));
        assert!(rows.iter().any(|(n, _)| n == "Compras"));
        assert!(rows.iter().any(|(n, k)| n == "Transferências" && k == "transfer"));
        assert!(rows.iter().any(|(n, k)| n == "Investimentos" && k == "transfer"));
        assert!(
            !rows.iter().any(|(n, _)| n == "Renda"),
            "migration 0008 removed Renda"
        );
    }

    #[test]
    fn create_category_inserts_row() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES (?1, ?2, ?3)",
            params!["Viagem", "--color-cat-outros", "expense"],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 14);
    }

    #[test]
    fn create_category_rejects_duplicate_name() {
        let conn = fresh_conn();
        let r = conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES (?1, ?2, ?3)",
            params!["Mercado", "--color-cat-mercado", "expense"],
        );
        assert!(r.is_err(), "UNIQUE constraint on name should reject");
    }

    /// Seeded categories all have rules, and `rules.category_id` is NOT NULL,
    /// so deleting one failed with a foreign-key error.
    #[test]
    fn a_category_with_rules_can_be_deleted() {
        let mut conn = fresh_conn();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        let market: i64 = conn
            .query_row("SELECT id FROM categories WHERE key = 'market'", [], |r| r.get(0))
            .unwrap();
        let rules_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM rules WHERE category_id = ?1", [market], |r| r.get(0))
            .unwrap();
        assert!(rules_before > 0, "precondition: the seed gives Mercado rules");

        super::delete(&mut conn, market).unwrap();

        let rules_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM rules WHERE category_id = ?1", [market], |r| r.get(0))
            .unwrap();
        assert_eq!(rules_after, 0);
    }

    #[test]
    fn delete_category_cascades_to_null_in_transactions() {
        let conn = fresh_conn();
        // create a custom category to delete
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Viagem', '--color-cat-outros', 'expense')",
            [],
        )
        .unwrap();
        let cat_id: i64 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid) VALUES ('t', NULL, 'A')",
            [],
        )
        .unwrap();
        let acc: i64 = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description, category_id, ofx_fitid)
             VALUES (?1, '2026-04-01', '-10', 'd', ?2, NULL)",
            params![acc, cat_id],
        )
        .unwrap();

        // Simulate the command's SQL
        conn.execute(
            "UPDATE transactions SET category_id = NULL WHERE category_id = ?1",
            params![cat_id],
        )
        .unwrap();
        conn.execute("DELETE FROM categories WHERE id = ?1", params![cat_id])
            .unwrap();

        let null_cat_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE category_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(null_cat_count, 1);
    }
}
