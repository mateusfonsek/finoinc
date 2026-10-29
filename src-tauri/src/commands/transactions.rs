use rusqlite::params;
use rusqlite::Connection;
use tauri::State;

use crate::db::Db;
use crate::domain::transaction::{ExpenseRow, InsertResult, NewTransaction, Transaction, TxKey};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TransactionFilters {
    pub account_id: Option<i64>,
    pub month: Option<String>,
    pub category_id: Option<i64>,
    pub q: Option<String>,
    pub limit: Option<u32>,
}

pub fn list(conn: &Connection, f: &TransactionFilters) -> AppResult<Vec<Transaction>> {
    let mut where_clauses: Vec<String> = Vec::new();
    let mut bound: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = f.account_id {
        where_clauses.push(format!("account_id = ?{}", bound.len() + 1));
        bound.push(Box::new(id));
    }
    if let Some(month) = f.month.as_ref() {
        where_clauses.push(format!("date LIKE ?{}", bound.len() + 1));
        bound.push(Box::new(format!("{month}-%")));
    }
    if let Some(cid) = f.category_id {
        where_clauses.push(format!("category_id = ?{}", bound.len() + 1));
        bound.push(Box::new(cid));
    }
    if let Some(q) = f.q.as_ref() {
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            where_clauses.push(format!(
                "(LOWER(description) LIKE ?{n} OR LOWER(COALESCE(notes, '')) LIKE ?{n})",
                n = bound.len() + 1
            ));
            bound.push(Box::new(format!("%{}%", trimmed.to_lowercase())));
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };
    let limit_sql = match f.limit {
        Some(n) => format!(" LIMIT {n}"),
        None => String::new(),
    };
    let sql = format!(
        "SELECT id, account_id, date, amount, description, category_id, notes, ofx_fitid, imported_at
         FROM transactions{where_sql} ORDER BY date DESC, id DESC{limit_sql}",
    );

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Transaction {
            id: row.get(0)?,
            account_id: row.get(1)?,
            date: row.get(2)?,
            amount: row.get(3)?,
            description: row.get(4)?,
            category_id: row.get(5)?,
            notes: row.get(6)?,
            ofx_fitid: row.get(7)?,
            imported_at: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub fn list_transactions(
    db: State<'_, Db>,
    filters: Option<TransactionFilters>,
) -> AppResult<Vec<Transaction>> {
    let f = filters.unwrap_or_default();
    let conn = db.conn.lock().expect("db mutex poisoned");
    list(&conn, &f)
}

/// Insert a batch of new transactions. Returns counts of inserted vs skipped (duplicates).
/// Dedup happens via UNIQUE(account_id, ofx_fitid, date, amount) + INSERT OR IGNORE.
/// The triple matches genuinely identical transactions — the same FITID with a
/// different amount or date (typical Nubank case: original purchase plus
/// refund) is stored as a distinct tx.
#[tauri::command]
#[specta::specta]
pub fn insert_transactions(
    db: State<'_, Db>,
    account_id: i64,
    txs: Vec<NewTransaction>,
) -> AppResult<InsertResult> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");

    for (idx, tx) in txs.iter().enumerate() {
        tx.parse_amount().map_err(|e| {
            AppError::Invalid(format!("tx[{idx}]: invalid amount '{}': {e}", tx.amount))
        })?;
    }

    let tx_conn = conn.transaction()?;
    let mut inserted = 0u32;
    let mut skipped = 0u32;
    {
        let mut stmt = tx_conn.prepare(
            "INSERT OR IGNORE INTO transactions
                (account_id, date, amount, description, ofx_fitid)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for tx in &txs {
            let changes = stmt.execute(params![
                account_id,
                tx.date,
                tx.amount,
                tx.description,
                tx.ofx_fitid,
            ])?;
            if changes == 1 {
                inserted += 1;
            } else {
                skipped += 1;
            }
        }
    }
    tx_conn.commit()?;

    let auto_categorized =
        crate::commands::rules::apply_rules_internal(&mut conn, Some(account_id))?;

    Ok(InsertResult {
        inserted,
        skipped_duplicates: skipped,
        auto_categorized,
    })
}

/// The month's largest expenses (or all-time when `month` is None).
/// Filters: amount < 0, category `kind != 'transfer'` (excludes bill payments,
/// internal transfers and investment deposits). Ordered most to least expensive
/// (CAST AS REAL ASC, since amounts are negative).
#[tauri::command]
#[specta::specta]
pub fn top_expenses(
    db: State<'_, Db>,
    month: Option<String>,
    limit: Option<u32>,
) -> AppResult<Vec<ExpenseRow>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let pattern: Option<String> = month.as_ref().map(|m| format!("{m}-%"));
    let lim = limit.unwrap_or(8);

    let mut stmt = conn.prepare(
        "SELECT t.id, t.date, t.amount, t.description,
                t.category_id, c.name, c.color_token
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE (?1 IS NULL OR t.date LIKE ?1)
           AND CAST(t.amount AS REAL) < 0
           AND COALESCE(c.kind, '') != 'transfer'
         ORDER BY CAST(t.amount AS REAL) ASC
         LIMIT ?2",
    )?;
    let pat_ref: Option<&str> = pattern.as_deref();
    let rows = stmt.query_map(params![pat_ref, lim], |row| {
        Ok(ExpenseRow {
            id: row.get(0)?,
            date: row.get(1)?,
            amount: row.get(2)?,
            description: row.get(3)?,
            category_id: row.get(4)?,
            category_name: row.get(5)?,
            category_color_token: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

/// Given `(fitid, date, amount)` triples, returns the subset already present in
/// this account. The frontend uses the triple as the duplicate key because the
/// table's UNIQUE is composite — the same FITID with a different amount/date is a
/// distinct transaction (e.g. an original purchase vs its refund).
#[tauri::command]
#[specta::specta]
pub fn check_existing_tx_keys(
    db: State<'_, Db>,
    account_id: i64,
    keys: Vec<TxKey>,
) -> AppResult<Vec<TxKey>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn.lock().expect("db mutex poisoned");

    let mut stmt = conn.prepare(
        "SELECT 1 FROM transactions
         WHERE account_id = ?1 AND ofx_fitid = ?2 AND date = ?3 AND amount = ?4
         LIMIT 1",
    )?;

    let mut existing: Vec<TxKey> = Vec::new();
    for k in keys {
        let found: bool = stmt
            .query_row(
                params![account_id, k.ofx_fitid, k.date, k.amount],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if found {
            existing.push(k);
        }
    }

    Ok(existing)
}

pub fn set_category(
    conn: &Connection,
    transaction_id: i64,
    category_id: Option<i64>,
) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE transactions SET category_id = ?1 WHERE id = ?2",
        params![category_id, transaction_id],
    )?;
    if changed == 0 {
        return Err(AppError::Invalid(format!(
            "transaction {transaction_id} not found"
        )));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_transaction_category(
    db: State<'_, Db>,
    transaction_id: i64,
    category_id: Option<i64>,
) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    set_category(&conn, transaction_id, category_id)
}

#[tauri::command]
#[specta::specta]
pub fn update_transaction_notes(
    db: State<'_, Db>,
    transaction_id: i64,
    notes: Option<String>,
) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let changed = conn.execute(
        "UPDATE transactions SET notes = ?1 WHERE id = ?2",
        params![notes, transaction_id],
    )?;
    if changed == 0 {
        return Err(AppError::Invalid(format!(
            "transaction {transaction_id} not found"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::migrations;
    use crate::domain::transaction::NewTransaction;
    use rusqlite::{params, Connection};

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    fn insert_account(conn: &Connection, name: &str, acctid: Option<&str>) -> i64 {
        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid) VALUES (?1, NULL, ?2)",
            params![name, acctid],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn raw_insert_batch(
        conn: &mut Connection,
        account_id: i64,
        txs: &[NewTransaction],
    ) -> (u32, u32) {
        let tx_conn = conn.transaction().unwrap();
        let mut inserted = 0u32;
        let mut skipped = 0u32;
        {
            let mut stmt = tx_conn
                .prepare(
                    "INSERT OR IGNORE INTO transactions
                        (account_id, date, amount, description, ofx_fitid)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .unwrap();
            for tx in txs {
                let changes = stmt
                    .execute(params![
                        account_id,
                        tx.date,
                        tx.amount,
                        tx.description,
                        tx.ofx_fitid
                    ])
                    .unwrap();
                if changes == 1 {
                    inserted += 1;
                } else {
                    skipped += 1;
                }
            }
        }
        tx_conn.commit().unwrap();
        (inserted, skipped)
    }

    fn mk(fitid: &str, amount: &str) -> NewTransaction {
        NewTransaction {
            date: "2026-04-12".into(),
            amount: amount.into(),
            description: format!("desc-{fitid}"),
            ofx_fitid: Some(fitid.into()),
        }
    }

    #[test]
    fn insert_batch_inserts_all_when_no_dups() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10.00"), mk("F2", "-5.50"), mk("F3", "100.00")];
        let (ins, skip) = raw_insert_batch(&mut conn, acc, &txs);
        assert_eq!(ins, 3);
        assert_eq!(skip, 0);
    }

    #[test]
    fn insert_batch_skips_duplicates() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs1 = vec![mk("F1", "10.00"), mk("F2", "-5.50")];
        raw_insert_batch(&mut conn, acc, &txs1);

        let txs2 = vec![mk("F1", "10.00"), mk("F2", "-5.50"), mk("F3", "100.00")];
        let (ins, skip) = raw_insert_batch(&mut conn, acc, &txs2);
        assert_eq!(ins, 1);
        assert_eq!(skip, 2);
    }

    #[test]
    fn same_fitid_different_account_is_not_duplicate() {
        let mut conn = fresh_conn();
        let a1 = insert_account(&conn, "acc1", Some("A1"));
        let a2 = insert_account(&conn, "acc2", Some("A2"));
        let txs = vec![mk("SHARED_FITID", "10.00")];
        raw_insert_batch(&mut conn, a1, &txs);
        let (ins, skip) = raw_insert_batch(&mut conn, a2, &txs);
        assert_eq!(ins, 1, "different account = no collision");
        assert_eq!(skip, 0);
    }

    #[test]
    fn null_fitid_does_not_dedup() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let mut a = mk("X", "10.00");
        a.ofx_fitid = None;
        let mut b = mk("Y", "10.00");
        b.ofx_fitid = None;
        let (ins, skip) = raw_insert_batch(&mut conn, acc, &[a, b]);
        assert_eq!(ins, 2, "NULL FITID = SQLite considers each row unique");
        assert_eq!(skip, 0);
    }

    #[test]
    fn check_existing_tx_keys_matches_triple() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10"), mk("F2", "20"), mk("F3", "30")];
        raw_insert_batch(&mut conn, acc, &txs);

        // Exact match on the triple. F4 does not exist; F1 with the wrong
        // amount does not match; F1 with the right triple does.
        let mut stmt = conn
            .prepare(
                "SELECT 1 FROM transactions
                 WHERE account_id = ?1 AND ofx_fitid = ?2 AND date = ?3 AND amount = ?4
                 LIMIT 1",
            )
            .unwrap();
        let probes: &[(&str, &str, &str, bool)] = &[
            ("F1", "2026-04-12", "10", true),
            ("F1", "2026-04-12", "99", false),
            ("F4", "2026-04-12", "10", false),
        ];
        for (fitid, date, amount, expected) in probes {
            let found: bool = stmt
                .query_row(params![acc, fitid, date, amount], |_| Ok(true))
                .unwrap_or(false);
            assert_eq!(found, *expected, "probe {fitid}/{date}/{amount}");
        }
    }

    #[test]
    fn same_fitid_different_amount_is_not_duplicate() {
        // Real Nubank case: a purchase (DEBIT -108.14) and its refund
        // (CREDIT +108.14) share a FITID but have opposite signs. With the
        // composite UNIQUE both must be inserted — they are not duplicates.
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let mut original = mk("SHARED", "-108.14");
        original.date = "2026-03-17".into();
        let mut estorno = mk("SHARED", "108.14");
        estorno.date = "2026-04-18".into();
        let (ins, skip) = raw_insert_batch(&mut conn, acc, &[original, estorno]);
        assert_eq!(ins, 2, "compra original e estorno coexistem");
        assert_eq!(skip, 0);
    }

    #[test]
    fn top_expenses_filters_orders_and_limits() {
        // 4 outflows + 1 inflow + 1 transfer. Top 2 must return only the two
        // largest outflows by absolute value, descending; the inflow and the
        // transfer are excluded.
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let mut e1 = mk("F1", "-50.00");
        e1.date = "2026-04-10".into();
        let mut e2 = mk("F2", "-200.00");
        e2.date = "2026-04-12".into();
        let mut e3 = mk("F3", "-30.00");
        e3.date = "2026-04-15".into();
        let mut e4 = mk("F4", "-150.00");
        e4.date = "2026-04-20".into();
        let mut inc = mk("F5", "1000.00");
        inc.date = "2026-04-22".into();
        let mut transfer = mk("F6", "-300.00");
        transfer.date = "2026-04-22".into();
        raw_insert_batch(&mut conn, acc, &[e1, e2, e3, e4, inc, transfer]);

        // Marks F6 as kind='transfer' (credit-card bill payment).
        conn.execute(
            "UPDATE transactions SET category_id = (
                SELECT id FROM categories WHERE name = 'Transferências'
             ) WHERE ofx_fitid = 'F6'",
            [],
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT t.ofx_fitid FROM transactions t
                 LEFT JOIN categories c ON c.id = t.category_id
                 WHERE t.date LIKE ?1
                   AND CAST(t.amount AS REAL) < 0
                   AND COALESCE(c.kind, '') != 'transfer'
                 ORDER BY CAST(t.amount AS REAL) ASC
                 LIMIT ?2",
            )
            .unwrap();
        let hits: Vec<String> = stmt
            .query_map(params!["2026-04-%", 2i64], |r| r.get::<_, Option<String>>(0))
            .unwrap()
            .filter_map(|r| r.ok().flatten())
            .collect();

        assert_eq!(
            hits,
            vec!["F2".to_string(), "F4".to_string()],
            "must return F2 (-200) and F4 (-150), in that order; no F5 (income) and no F6 (transfer)"
        );
    }

    #[test]
    fn same_fitid_same_amount_same_date_is_duplicate() {
        // Two rows identical in (fitid, date, amount) still count as duplicates
        // — INSERT OR IGNORE drops the second.
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10.00"), mk("F1", "10.00")];
        let (ins, skip) = raw_insert_batch(&mut conn, acc, &txs);
        assert_eq!(ins, 1);
        assert_eq!(skip, 1);
    }

    #[test]
    fn list_transactions_filter_by_month() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let mut a = mk("F1", "10.00");
        a.date = "2026-03-15".into();
        let mut b = mk("F2", "-5.00");
        b.date = "2026-04-02".into();
        raw_insert_batch(&mut conn, acc, &[a, b]);

        let mut stmt = conn
            .prepare(
                "SELECT id FROM transactions WHERE account_id = ?1 AND date LIKE ?2 ORDER BY date DESC",
            )
            .unwrap();
        let ids: Vec<i64> = stmt
            .query_map(params![acc, "2026-03-%"], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids.len(), 1, "only March transaction should match");
    }

    #[test]
    fn limit_clause_caps_result_count() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10.00"), mk("F2", "20.00"), mk("F3", "30.00")];
        raw_insert_batch(&mut conn, acc, &txs);

        let mut stmt = conn
            .prepare("SELECT id FROM transactions ORDER BY date DESC, id DESC LIMIT 2")
            .unwrap();
        let rows: Vec<i64> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn list_transactions_filter_by_q_matches_description_and_notes() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10"), mk("F2", "20"), mk("F3", "30")];
        raw_insert_batch(&mut conn, acc, &txs);
        conn.execute(
            "UPDATE transactions SET notes = 'pay uber later' WHERE ofx_fitid = 'F2'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE transactions SET description = 'UBER TRIP' WHERE ofx_fitid = 'F3'",
            [],
        )
        .unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT ofx_fitid FROM transactions
                 WHERE LOWER(description) LIKE ?1 OR LOWER(COALESCE(notes, '')) LIKE ?1
                 ORDER BY id",
            )
            .unwrap();
        let hits: Vec<String> = stmt
            .query_map(params!["%uber%"], |r| r.get::<_, Option<String>>(0))
            .unwrap()
            .filter_map(|r| r.ok().flatten())
            .collect();

        assert_eq!(hits, vec!["F2".to_string(), "F3".to_string()]);
    }

    #[test]
    fn update_transaction_category_changes_value() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn, "test", Some("ACC1"));
        let txs = vec![mk("F1", "10")];
        raw_insert_batch(&mut conn, acc, &txs);
        let tx_id: i64 = conn
            .query_row(
                "SELECT id FROM transactions WHERE ofx_fitid = 'F1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let cat_id: i64 = conn
            .query_row(
                "SELECT id FROM categories WHERE name = 'Mercado'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        conn.execute(
            "UPDATE transactions SET category_id = ?1 WHERE id = ?2",
            params![cat_id, tx_id],
        )
        .unwrap();

        let stored: i64 = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, cat_id);
    }
}
