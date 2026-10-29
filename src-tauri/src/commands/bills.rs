//! Manual settlement of a bill occurrence.
//!
//! The calendar derives what it can from the statement. This is the one place
//! the user can tell it something the statement does not contain: that a bill
//! was paid in cash, or by someone else, or that a specific transaction is the
//! one that paid it.

use rusqlite::{params, Connection};
use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// The one check for a `YYYY-MM` month, used by everything that takes one. It
/// is strict on purpose: the month reaches SQL as a `LIKE` prefix, so a looser
/// check elsewhere would decide what that pattern matches.
pub fn validate_month(month: &str) -> AppResult<()> {
    let ok = month.len() == 7
        && month.as_bytes()[4] == b'-'
        && month[0..4].bytes().all(|b| b.is_ascii_digit())
        && month[5..7].bytes().all(|b| b.is_ascii_digit())
        && (1..=12).contains(&month[5..7].parse::<u32>().unwrap_or(0));
    if !ok {
        return Err(AppError::Invalid(format!(
            "month must be 'YYYY-MM' (got: '{month}')"
        )));
    }
    Ok(())
}

pub fn settle(
    conn: &Connection,
    rule_id: i64,
    due_month: &str,
    transaction_id: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO bill_settlements (rule_id, due_month, transaction_id)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(rule_id, due_month)
         DO UPDATE SET transaction_id = excluded.transaction_id,
                       settled_at = datetime('now')",
        params![rule_id, due_month, transaction_id],
    )?;
    Ok(())
}

pub fn unsettle(conn: &Connection, rule_id: i64, due_month: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM bill_settlements WHERE rule_id = ?1 AND due_month = ?2",
        params![rule_id, due_month],
    )?;
    Ok(())
}

/// One transaction already spoken for, and by which occurrence.
///
/// The search dialog needs this to stop the same payment settling two bills:
/// with a broad search over every transaction, picking one twice is easy to do
/// by accident, and the derivation exclusion is global — that money would leave
/// every rule's calculation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct BillLink {
    pub transaction_id: i64,
    /// What to call the bill in "already pays X of <month>": the rule's display
    /// name when it has one, otherwise its first snippet.
    pub rule_label: String,
    pub due_month: String,
}

pub fn links(conn: &Connection) -> AppResult<Vec<BillLink>> {
    let mut stmt = conn.prepare(
        "SELECT s.transaction_id,
                COALESCE(r.display_name,
                         (SELECT p.pattern FROM rule_patterns p
                           WHERE p.rule_id = r.id ORDER BY p.id LIMIT 1),
                         ''),
                s.due_month
           FROM bill_settlements s
           JOIN rules r ON r.id = s.rule_id
          WHERE s.transaction_id IS NOT NULL
          ORDER BY s.due_month DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(BillLink {
            transaction_id: row.get(0)?,
            rule_label: row.get(1)?,
            due_month: row.get(2)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

/// Every settlement that points at a transaction. One row per bill per month,
/// so the whole set is small enough to hand over unpaginated.
#[tauri::command]
#[specta::specta]
pub fn bill_links(db: State<'_, Db>) -> AppResult<Vec<BillLink>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    links(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn settle_bill(
    db: State<'_, Db>,
    rule_id: i64,
    due_month: String,
    transaction_id: Option<i64>,
) -> AppResult<()> {
    validate_month(&due_month)?;
    let conn = db.conn.lock().expect("db mutex poisoned");
    settle(&conn, rule_id, &due_month, transaction_id)
}

#[tauri::command]
#[specta::specta]
pub fn unsettle_bill(db: State<'_, Db>, rule_id: i64, due_month: String) -> AppResult<()> {
    validate_month(&due_month)?;
    let conn = db.conn.lock().expect("db mutex poisoned");
    unsettle(&conn, rule_id, &due_month)
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

    fn a_rule(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Home', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day) VALUES (?1, 0, 6)",
            [cat],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn settled_rows(conn: &Connection) -> Vec<(String, Option<i64>)> {
        conn.prepare("SELECT due_month, transaction_id FROM bill_settlements ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn settling_without_a_transaction_records_payment_outside_the_statement() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);

        settle(&conn, rule, "2026-08", None).unwrap();

        assert_eq!(settled_rows(&conn), vec![("2026-08".to_string(), None)]);
    }

    /// Settling twice is a click, not an error: the second one replaces what the
    /// first said instead of failing on the UNIQUE.
    #[test]
    fn settling_again_replaces_the_previous_answer() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);
        conn.execute("INSERT INTO accounts (name) VALUES ('c')", []).unwrap();
        let account = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description)
             VALUES (?1, '2026-07-15', '-182.40', 'ENERGIA')",
            [account],
        )
        .unwrap();
        let tx = conn.last_insert_rowid();

        settle(&conn, rule, "2026-08", None).unwrap();
        settle(&conn, rule, "2026-08", Some(tx)).unwrap();

        assert_eq!(settled_rows(&conn), vec![("2026-08".to_string(), Some(tx))]);
    }

    #[test]
    fn unsettling_removes_the_row() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);
        settle(&conn, rule, "2026-08", None).unwrap();

        unsettle(&conn, rule, "2026-08").unwrap();

        assert!(settled_rows(&conn).is_empty());
    }

    /// Undoing a bill that was never settled is a no-op, not a failure: the user
    /// asked for a state that already holds.
    #[test]
    fn unsettling_what_was_never_settled_is_not_an_error() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);

        assert!(unsettle(&conn, rule, "2026-08").is_ok());
    }

    fn account_and_payment(conn: &Connection, date: &str) -> i64 {
        conn.execute("INSERT OR IGNORE INTO accounts (id, name) VALUES (1, 'c')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description)
             VALUES (1, ?1, '-182.40', 'ENERGIA')",
            params![date],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn links_are_empty_when_nothing_is_settled() {
        let conn = fresh_conn();
        a_rule(&conn);

        assert!(links(&conn).unwrap().is_empty());
    }

    /// A cash settlement points at no transaction, so it cannot block one.
    #[test]
    fn a_settlement_without_a_transaction_is_not_a_link() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);
        settle(&conn, rule, "2026-08", None).unwrap();

        assert!(links(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_linked_transaction_reports_its_occurrence() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);
        let tx = account_and_payment(&conn, "2026-07-15");
        conn.execute(
            "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, 'ENERGIA')",
            [rule],
        )
        .unwrap();
        settle(&conn, rule, "2026-08", Some(tx)).unwrap();

        let found = links(&conn).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].transaction_id, tx);
        assert_eq!(found[0].due_month, "2026-08");
        assert_eq!(found[0].rule_label, "ENERGIA", "falls back to the first snippet");
    }

    /// The label is what the user named the bill, when they named it.
    #[test]
    fn a_display_name_wins_over_the_snippet() {
        let conn = fresh_conn();
        let rule = a_rule(&conn);
        let tx = account_and_payment(&conn, "2026-07-15");
        conn.execute(
            "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, 'ENERGIA')",
            [rule],
        )
        .unwrap();
        conn.execute(
            "UPDATE rules SET display_name = 'Conta de luz' WHERE id = ?1",
            [rule],
        )
        .unwrap();
        settle(&conn, rule, "2026-08", Some(tx)).unwrap();

        assert_eq!(links(&conn).unwrap()[0].rule_label, "Conta de luz");
    }

    #[test]
    fn a_month_must_look_like_a_month() {
        assert!(validate_month("2026-08").is_ok());
        assert!(validate_month("2026-8").is_err());
        assert!(validate_month("2026-08-01").is_err());
        assert!(validate_month("agosto").is_err());
    }
}
