use rusqlite::{params, OptionalExtension};
use tauri::State;

use crate::commands::bills::validate_month;
use crate::db::Db;
use rust_decimal::Decimal;
use std::collections::HashSet;
use std::str::FromStr;

use crate::domain::rule::{
    CalendarEvent, NewRule, Rule, RuleChoice, RuleMatches, RulePreviewRow, RuleWithCount,
    UpdateRule,
};
use crate::domain::transaction::Transaction;
use crate::error::{AppError, AppResult};

fn validate_due_day(d: Option<i32>) -> AppResult<()> {
    if let Some(day) = d {
        if !(1..=31).contains(&day) {
            return Err(AppError::Invalid(format!(
                "due_day must be between 1 and 31 (got: {day})"
            )));
        }
    }
    Ok(())
}

/// The column takes any integer; this takes what the form offers. Widening the
/// form later is a UI change, not a migration — but `calendar_events_with_conn`
/// only ever reads `month` and the one month before it, so a lead beyond 1 must
/// widen that read too, or its transactions are never loaded and the bill never
/// settles.
fn validate_pay_lead_months(v: i32) -> AppResult<()> {
    if !(0..=1).contains(&v) {
        return Err(AppError::Invalid(format!(
            "pay_lead_months must be 0 or 1 (got: {v})"
        )));
    }
    Ok(())
}

/// Normalizes the snippet list from the UI: trims, drops empties and removes
/// duplicates (case-insensitively, which is how matching works). Preserves the
/// order the user typed.
fn clean_patterns(raw: &[String]) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for p in raw {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if out.iter().any(|kept| kept.eq_ignore_ascii_case(p)) {
            continue;
        }
        out.push(p.to_string());
    }
    if out.is_empty() {
        return Err(AppError::Invalid("rule must have at least one pattern".into()));
    }
    Ok(out)
}

/// Rewrites a rule's snippets. Replaces wholesale instead of diffing: the list
/// is short and order matters, so recreating is simpler to get right.
fn replace_patterns(tx: &rusqlite::Transaction, rule_id: i64, patterns: &[String]) -> AppResult<()> {
    tx.execute("DELETE FROM rule_patterns WHERE rule_id = ?1", params![rule_id])?;
    let mut stmt = tx.prepare("INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, ?2)")?;
    for p in patterns {
        stmt.execute(params![rule_id, p])?;
    }
    Ok(())
}

/// A rule's snippets, in the order they were written.
fn patterns_of(conn: &rusqlite::Connection, rule_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT pattern FROM rule_patterns WHERE rule_id = ?1 ORDER BY id")?;
    let rows = stmt.query_map(params![rule_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub fn all(conn: &rusqlite::Connection) -> AppResult<Vec<Rule>> {
    let mut stmt = conn.prepare(
        "SELECT id, category_id, priority, due_day, display_name, created_at, pay_lead_months
         FROM rules
         ORDER BY priority DESC, created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        Ok(Rule {
            id,
            patterns: patterns_of(conn, id)?,
            category_id: row.get(1)?,
            priority: row.get(2)?,
            due_day: row.get(3)?,
            display_name: row.get(4)?,
            created_at: row.get(5)?,
            pay_lead_months: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub fn list_rules(db: State<'_, Db>) -> AppResult<Vec<Rule>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    all(&conn)
}

/// Like `list_rules` but with each rule's reach. A separate command because
/// the count scans transactions — callers that only need the rules (import,
/// suggestions, calendar) should not pay for it.
#[tauri::command]
#[specta::specta]
pub fn list_rules_with_count(db: State<'_, Db>) -> AppResult<Vec<RuleWithCount>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let mut stmt = conn.prepare(
        "SELECT r.id, r.category_id, r.priority, r.due_day, r.display_name, r.created_at,
                r.pay_lead_months,
                (SELECT COUNT(*) FROM transactions t
                  WHERE EXISTS (
                      SELECT 1 FROM rule_patterns p
                       WHERE p.rule_id = r.id
                         AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%'
                  )) AS tx_count
         FROM rules r
         ORDER BY r.priority DESC, r.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        Ok(RuleWithCount {
            id,
            patterns: patterns_of(&conn, id)?,
            category_id: row.get(1)?,
            priority: row.get(2)?,
            due_day: row.get(3)?,
            display_name: row.get(4)?,
            created_at: row.get(5)?,
            pay_lead_months: row.get(6)?,
            transaction_count: row.get::<_, i64>(7)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(AppError::from)
}

pub fn create(conn: &mut rusqlite::Connection, input: NewRule) -> AppResult<Rule> {
    create_with_scope(conn, input, None)
}

/// Same as `create`, but the backfill it triggers only reaches transactions on
/// or after `apply_since` (`YYYY-MM-DD`) when given. The user's own "new rule"
/// form has no such limit and always calls `create`; an MCP agent bounded by a
/// data window may only backfill inside it — its write must not reach further
/// back than its read is allowed to see.
pub fn create_with_scope(
    conn: &mut rusqlite::Connection,
    input: NewRule,
    apply_since: Option<&str>,
) -> AppResult<Rule> {
    let patterns = clean_patterns(&input.patterns)?;
    validate_due_day(input.due_day)?;
    validate_pay_lead_months(input.pay_lead_months)?;

    let id = {
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO rules (category_id, priority, due_day, display_name, pay_lead_months)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.category_id,
                input.priority,
                input.due_day,
                input.display_name.as_deref().map(str::trim),
                input.pay_lead_months,
            ],
        )?;
        let id = tx.last_insert_rowid();
        replace_patterns(&tx, id, &patterns)?;
        tx.commit()?;
        id
    };

    apply_rules_since(conn, None, apply_since)?;
    fetch_rule(conn, id)
}

#[tauri::command]
#[specta::specta]
pub fn create_rule(db: State<'_, Db>, input: NewRule) -> AppResult<Rule> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    create(&mut conn, input)
}

#[tauri::command]
#[specta::specta]
pub fn update_rule(db: State<'_, Db>, rule_id: i64, input: UpdateRule) -> AppResult<Rule> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    update_rule_with_conn(&mut conn, rule_id, input)
}

fn update_rule_with_conn(
    conn: &mut rusqlite::Connection,
    rule_id: i64,
    input: UpdateRule,
) -> AppResult<Rule> {
    let patterns = clean_patterns(&input.patterns)?;
    validate_due_day(input.due_day)?;
    validate_pay_lead_months(input.pay_lead_months)?;

    {
        let tx = conn.transaction()?;
        let previous_due_day: Option<i32> = tx
            .query_row(
                "SELECT due_day FROM rules WHERE id = ?1",
                params![rule_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let changed = tx.execute(
            "UPDATE rules
             SET category_id = ?1, priority = ?2, due_day = ?3, display_name = ?4,
                 pay_lead_months = ?5
             WHERE id = ?6",
            params![
                input.category_id,
                input.priority,
                input.due_day,
                input.display_name.as_deref().map(str::trim),
                input.pay_lead_months,
                rule_id
            ],
        )?;
        if changed == 0 {
            return Err(AppError::Invalid(format!("rule {rule_id} not found")));
        }
        // An occurrence IS (rule, due month), so a rule with no due day has no
        // occurrences and a settlement on one is a row about something that no
        // longer exists — invisible in every surface, with no way to undo it,
        // and still holding its transaction out of derivation for every other
        // rule. Clearing the due day clears them and releases the transaction.
        if previous_due_day.is_some() && input.due_day.is_none() {
            tx.execute(
                "DELETE FROM bill_settlements WHERE rule_id = ?1",
                params![rule_id],
            )?;
        }
        replace_patterns(&tx, rule_id, &patterns)?;
        tx.commit()?;
    }

    apply_rules_internal(conn, None)?;
    fetch_rule(conn, rule_id)
}

#[tauri::command]
#[specta::specta]
pub fn delete_rule(db: State<'_, Db>, rule_id: i64) -> AppResult<()> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let changed = conn.execute("DELETE FROM rules WHERE id = ?1", params![rule_id])?;
    if changed == 0 {
        return Err(AppError::Invalid(format!("rule {rule_id} not found")));
    }
    Ok(())
}

/// Deletes a rule AND clears category_id from any transaction that was likely
/// categorized BY this rule (description matches ANY of its patterns +
/// category_id is this rule's category). Then re-applies remaining rules to
/// pick alternatives.
///
/// Used by the import screen when the user wants to undo an auto-created rule.
/// Returns the count of transactions whose category was cleared.
#[tauri::command]
#[specta::specta]
pub fn delete_rule_with_cleanup(db: State<'_, Db>, rule_id: i64) -> AppResult<u32> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    delete_with_cleanup(&mut conn, rule_id)
}

fn delete_with_cleanup(conn: &mut rusqlite::Connection, rule_id: i64) -> AppResult<u32> {
    let tx = conn.transaction()?;
    // Clear before deleting: ON DELETE SET NULL would erase the very marker
    // that says what this rule categorized.
    let cleared = tx.execute(
        "UPDATE transactions SET category_id = NULL, categorized_by_rule = NULL
          WHERE categorized_by_rule = ?1",
        params![rule_id],
    )?;
    if tx.execute("DELETE FROM rules WHERE id = ?1", params![rule_id])? == 0 {
        return Err(AppError::Invalid(format!("rule {rule_id} not found")));
    }
    tx.commit()?;

    // Re-apply remaining rules — a previously-shadowed rule may now match.
    apply_rules_internal(conn, None)?;
    Ok(cleared as u32)
}

/// The transactions a rule reaches, newest first.
///
/// Uses the SAME `EXISTS` over `rule_patterns` as the count in
/// `list_rules_with_count`: the number the table shows promises what this list
/// contains, and the two diverging would be a silent lie.
#[tauri::command]
#[specta::specta]
pub fn transactions_matching_rule(db: State<'_, Db>, rule_id: i64) -> AppResult<RuleMatches> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let mut stmt = conn.prepare(
        "SELECT id, account_id, date, amount, description, category_id, notes, ofx_fitid, imported_at
           FROM transactions t
          WHERE EXISTS (
                SELECT 1 FROM rule_patterns p
                 WHERE p.rule_id = ?1
                   AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%'
          )
          ORDER BY t.date DESC, t.id DESC",
    )?;
    let transactions: Vec<Transaction> = stmt
        .query_map(params![rule_id], |row| {
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
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut total = Decimal::ZERO;
    for t in &transactions {
        total += Decimal::from_str(&t.amount)
            .map_err(|e| AppError::Invalid(format!("bad amount: {e}")))?;
    }

    Ok(RuleMatches {
        transactions,
        total: total.to_string(),
    })
}

/// Everything applying the rules WOULD change, without writing anything.
///
/// Unlike `apply_rules_to_uncategorized`, which only touches uncategorized
/// rows, this also includes transactions that already have a category whose
/// winning rule points elsewhere — the ones the review screen must show before
/// overwriting anything.
///
/// Transactions where the winning rule already agrees with the current category
/// are left out: they are not changes, and listing them would only make the
/// review look bigger than it is.
#[tauri::command]
#[specta::specta]
pub fn preview_rule_application(
    db: State<'_, Db>,
    account_id: Option<i64>,
) -> AppResult<Vec<RulePreviewRow>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let scope_filter = match account_id {
        Some(_) => "AND t.account_id = ?1",
        None => "",
    };
    // The correlated subquery picks the SAME winning rule `apply_rules_internal`
    // would — priority desc, ties by newest. If the two diverged, the review
    // would lie about the outcome.
    let sql = format!(
        "SELECT t.id, t.date, t.amount, t.description, t.category_id,
                r.id, r.category_id, r.display_name
           FROM transactions t
           JOIN rules r ON r.id = (
                SELECT r2.id FROM rules r2
                 WHERE EXISTS (
                       SELECT 1 FROM rule_patterns p
                        WHERE p.rule_id = r2.id
                          AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%'
                 )
                 ORDER BY r2.priority DESC, r2.created_at DESC
                 LIMIT 1
           )
          WHERE (t.category_id IS NULL OR t.category_id <> r.category_id)
                {scope_filter}
          ORDER BY t.date DESC, t.id DESC",
    );

    let mut stmt = conn.prepare(&sql)?;
    type Row = (i64, String, String, String, Option<i64>, i64, i64, Option<String>);
    let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Row> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
        ))
    };
    let raw: Vec<Row> = match account_id {
        Some(id) => stmt.query_map(params![id], map)?.collect::<rusqlite::Result<_>>()?,
        None => stmt.query_map([], map)?.collect::<rusqlite::Result<_>>()?,
    };

    let mut out = Vec::with_capacity(raw.len());
    for (tx_id, date, amount, description, current, rule_id, new_cat, display_name) in raw {
        // Without `display_name`, the label is the snippet that matched THIS
        // description, not the rule's first one, which may be unrelated here.
        let rule_label = match display_name {
            Some(name) => name,
            None => {
                let desc_lc = description.to_lowercase();
                patterns_of(&conn, rule_id)?
                    .into_iter()
                    .find(|p| desc_lc.contains(&p.to_lowercase()))
                    .unwrap_or_default()
            }
        };
        out.push(RulePreviewRow {
            transaction_id: tx_id,
            date,
            amount,
            description,
            current_category_id: current,
            new_category_id: new_cat,
            rule_id,
            rule_label,
        });
    }
    Ok(out)
}

/// Writes only the changes the user ticked in the review.
///
/// Takes the target category along rather than re-querying the rules: what gets
/// written is exactly what was shown, even if a rule changed meanwhile.
#[tauri::command]
#[specta::specta]
pub fn apply_rule_choices(db: State<'_, Db>, choices: Vec<RuleChoice>) -> AppResult<u32> {
    if choices.is_empty() {
        return Ok(0);
    }
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    let tx = conn.transaction()?;
    let mut applied = 0u32;
    {
        let mut stmt =
            tx.prepare("UPDATE transactions SET category_id = ?1 WHERE id = ?2")?;
        for c in &choices {
            // A missing category would orphan the FK: fail loudly rather than
            // write garbage silently.
            let exists: bool = tx
                .query_row(
                    "SELECT 1 FROM categories WHERE id = ?1",
                    params![c.category_id],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if !exists {
                return Err(AppError::Invalid(format!(
                    "category {} not found",
                    c.category_id
                )));
            }
            applied += stmt.execute(params![c.category_id, c.transaction_id])? as u32;
        }
    }
    tx.commit()?;
    Ok(applied)
}

/// Run all rules on transactions with `category_id IS NULL` (manual categorization
/// is never overwritten). When multiple rules match the same transaction, the one
/// with the highest priority wins; ties broken by most recent created_at.
/// Returns the count of transactions newly categorized.
#[tauri::command]
#[specta::specta]
pub fn apply_rules_to_uncategorized(db: State<'_, Db>, account_id: Option<i64>) -> AppResult<u32> {
    let mut conn = db.conn.lock().expect("db mutex poisoned");
    apply_rules_internal(&mut conn, account_id)
}

/// Shared engine used both by the public command and by insert_transactions.
pub fn apply_rules_internal(
    conn: &mut rusqlite::Connection,
    account_id: Option<i64>,
) -> AppResult<u32> {
    apply_rules_since(conn, account_id, None)
}

/// Same engine as `apply_rules_internal`, additionally scoped to transactions
/// dated on or after `since` (`YYYY-MM-DD`) when given. `None` backfills the
/// whole history, same as before this filter existed.
pub fn apply_rules_since(
    conn: &mut rusqlite::Connection,
    account_id: Option<i64>,
    since: Option<&str>,
) -> AppResult<u32> {
    let mut scope_filter = String::new();
    let mut bound: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = account_id {
        scope_filter.push_str(&format!(" AND account_id = ?{}", bound.len() + 1));
        bound.push(Box::new(id));
    }
    if let Some(date) = since {
        scope_filter.push_str(&format!(" AND date >= ?{}", bound.len() + 1));
        bound.push(Box::new(date.to_string()));
    }

    // A rule matches when ANY of its snippets appears in the description. The
    // winning rule is recorded too, so undoing it clears only its own work.
    let sql = format!(
        "UPDATE transactions
         SET (category_id, categorized_by_rule) = (
             SELECT r.category_id, r.id FROM rules r
             WHERE EXISTS (
                 SELECT 1 FROM rule_patterns p
                 WHERE p.rule_id = r.id
                   AND LOWER(transactions.description) LIKE '%' || LOWER(p.pattern) || '%'
             )
             ORDER BY r.priority DESC, r.created_at DESC
             LIMIT 1
         )
         WHERE category_id IS NULL
           AND EXISTS (
               SELECT 1 FROM rules r
               JOIN rule_patterns p ON p.rule_id = r.id
               WHERE LOWER(transactions.description) LIKE '%' || LOWER(p.pattern) || '%'
           )
           {scope_filter}",
    );
    let params_refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let changed = conn.execute(&sql, params_refs.as_slice())?;
    Ok(changed as u32)
}

fn fetch_rule(conn: &rusqlite::Connection, id: i64) -> AppResult<Rule> {
    let patterns = patterns_of(conn, id)?;
    conn.query_row(
        "SELECT id, category_id, priority, due_day, display_name, created_at, pay_lead_months
         FROM rules WHERE id = ?1",
        params![id],
        |row| {
            Ok(Rule {
                id: row.get(0)?,
                patterns,
                category_id: row.get(1)?,
                priority: row.get(2)?,
                due_day: row.get(3)?,
                display_name: row.get(4)?,
                created_at: row.get(5)?,
                pay_lead_months: row.get(6)?,
            })
        },
    )
    .map_err(AppError::from)
}

/// `month` (`YYYY-MM`) moved `back` whole months. Arithmetic on a month index
/// rather than on a date, so December does not need a special case.
fn month_shifted(month: &str, back: i64) -> AppResult<String> {
    validate_month(month)?;
    // Four digits, '-', two digits: guaranteed above, so neither parse can fail.
    let year: i64 = month[0..4].parse().expect("validated month");
    let m: i64 = month[5..7].parse().expect("validated month");
    let index = year * 12 + (m - 1) - back;
    Ok(format!(
        "{:04}-{:02}",
        index.div_euclid(12),
        index.rem_euclid(12) + 1
    ))
}

/// Crosses rules with transactions to build the month's calendar events.
///
/// Resolution order per rule, stopping at the first hit:
/// 1. a row in `bill_settlements` for this occurrence — the user's word wins;
/// 2. a transaction matching any snippet in `month - pay_lead_months`;
/// 3. otherwise the state follows `due_day` alone.
///
/// A rule with no `due_day` and no payment does not appear, which is the user's
/// mental model: it only shows up once it costs something.
#[tauri::command]
#[specta::specta]
pub fn calendar_events(db: State<'_, Db>, month: String) -> AppResult<Vec<CalendarEvent>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    calendar_events_with_conn(&conn, &month)
}

pub fn calendar_events_with_conn(
    conn: &rusqlite::Connection,
    month: &str,
) -> AppResult<Vec<CalendarEvent>> {
    validate_month(month)?;

    type RuleRow = (i64, Vec<String>, Option<i32>, String, Option<String>, i64, String);
    // Bills first, then the order the categorization engine uses, so when two
    // bills could both claim a payment, the one that categorizes it gets it.
    let rule_rows: Vec<RuleRow> = {
        let mut stmt = conn.prepare(
            "SELECT r.id, r.due_day, c.name, c.color_token, r.pay_lead_months, c.kind
             FROM rules r
             JOIN categories c ON c.id = r.category_id
             ORDER BY r.due_day IS NULL, r.priority DESC, r.created_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                Ok((
                    id,
                    patterns_of(conn, id)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    // Both candidate months in one read: the lead is 0 or 1, so no rule can
    // need anything older than the previous month.
    let previous = month_shifted(month, 1)?;
    type TxRow = (i64, String, String, String);
    let tx_rows: Vec<TxRow> = {
        let mut stmt = conn.prepare(
            "SELECT id, date, amount, description
             FROM transactions
             WHERE (date LIKE ?1 OR date LIKE ?2)
               AND id NOT IN (
                   SELECT transaction_id FROM bill_settlements
                    WHERE transaction_id IS NOT NULL
               )
             ORDER BY date ASC, id ASC",
        )?;
        let rows = stmt
            .query_map(
                params![format!("{month}-%"), format!("{previous}-%")],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    let mut events: Vec<CalendarEvent> = Vec::new();
    // A transaction pays one bill at most. A rule with no due day is not a
    // bill: it shows what it matched but holds nothing back from the bills.
    let mut claimed: HashSet<i64> = HashSet::new();
    for (rule_id, patterns, due_day, cat_name, cat_color, lead, kind) in rule_rows {
        // A bill is money going out — or coming in, for an income rule. The
        // opposite sign is its refund, which pays nothing.
        let pays = |amount: &str| {
            let outgoing = amount.trim_start().starts_with('-');
            if kind == "income" { !outgoing } else { outgoing }
        };
        let settlement: Option<(Option<i64>,)> = conn
            .query_row(
                "SELECT transaction_id FROM bill_settlements
                  WHERE rule_id = ?1 AND due_month = ?2",
                params![rule_id, month],
                |row| Ok((row.get(0)?,)),
            )
            .ok();

        let patterns_lc: Vec<String> = patterns.iter().map(|p| p.to_lowercase()).collect();
        let pay_month = month_shifted(month, lead)?;
        let pay_prefix = format!("{pay_month}-");

        let (paid_date, paid_amount, paid_tx_id, manually_settled, hit) = match settlement {
            Some((linked,)) => {
                let paid = linked.and_then(|id| {
                    conn.query_row(
                        "SELECT date, amount FROM transactions WHERE id = ?1",
                        params![id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .ok()
                    .map(|(date, amount)| (date, amount, id))
                });
                match paid {
                    Some((date, amount, id)) => (Some(date), Some(amount), Some(id), true, None),
                    None => (None, None, None, true, None),
                }
            }
            None => {
                let matched = tx_rows
                    .iter()
                    .filter(|(id, date, amount, _)| {
                        date.starts_with(&pay_prefix) && !claimed.contains(id) && pays(amount)
                    })
                    .find_map(|(tx_id, date, amount, desc)| {
                        let desc_lc = desc.to_lowercase();
                        let hit = patterns_lc.iter().position(|p| desc_lc.contains(p))?;
                        Some((*tx_id, date.clone(), amount.clone(), hit))
                    });
                match matched {
                    Some((tx_id, date, amount, hit)) => {
                        if due_day.is_some() {
                            claimed.insert(tx_id);
                        }
                        (Some(date), Some(amount), Some(tx_id), false, Some(hit))
                    }
                    None => (None, None, None, false, None),
                }
            }
        };

        // Label: the snippet that actually matched, or the first one when the
        // event exists only because of the due date.
        let label = match hit {
            Some(i) => patterns[i].clone(),
            None => patterns.first().cloned().unwrap_or_default(),
        };

        if due_day.is_some() || paid_date.is_some() || manually_settled {
            events.push(CalendarEvent {
                rule_id,
                pattern: label,
                category_name: cat_name,
                category_color_token: cat_color,
                due_day,
                paid_date,
                paid_amount,
                paid_transaction_id: paid_tx_id,
                manually_settled,
            });
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::apply_rules_internal;
    use crate::db::migrations;
    use crate::domain::rule::UpdateRule;
    use rusqlite::{params, Connection};

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    fn category_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM categories WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn insert_account(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid) VALUES ('test', NULL, 'A1')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_tx(
        conn: &Connection,
        account_id: i64,
        description: &str,
        category_id: Option<i64>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description, category_id, ofx_fitid)
             VALUES (?1, '2026-04-12', '10.00', ?2, ?3, NULL)",
            params![account_id, description, category_id],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_rule(conn: &Connection, pattern: &str, cat: i64, priority: i32) -> i64 {
        insert_rule_multi(conn, &[pattern], cat, priority)
    }

    fn insert_rule_multi(conn: &Connection, patterns: &[&str], cat: i64, priority: i32) -> i64 {
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day) VALUES (?1, ?2, NULL)",
            params![cat, priority],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        for p in patterns {
            conn.execute(
                "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, ?2)",
                params![id, p],
            )
            .unwrap();
        }
        id
    }

    /// `apply_rules_internal` became a delegate to `apply_rules_since(.., None)`
    /// — this pins that the UI and import paths still reach transactions of
    /// any age, since neither of them ever wants a window.
    #[test]
    fn apply_rules_internal_still_backfills_a_transaction_from_any_date() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        insert_rule(&conn, "testmerchant", transporte, 0);
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description, category_id, ofx_fitid)
             VALUES (?1, '2019-01-05', '10.00', 'TESTMERCHANT ancient', NULL, NULL)",
            params![acc],
        )
        .unwrap();

        let n = apply_rules_internal(&mut conn, None).unwrap();

        assert_eq!(n, 1, "the delegate must still reach transactions from any date");
    }

    #[test]
    fn applies_rule_case_insensitive() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        insert_rule(&conn, "testmerchant", transporte, 0);
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT * TRIP 12345", None);

        let n = apply_rules_internal(&mut conn, None).unwrap();
        assert_eq!(n, 1);

        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(transporte));
    }

    #[test]
    fn higher_priority_rule_wins() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let outros = category_id(&conn, "Outros");

        insert_rule(&conn, "testmerchant", outros, 0);
        insert_rule(&conn, "testmerchant trip", transporte, 10);

        let tx_id = insert_tx(&conn, acc, "uber trip via app", None);
        apply_rules_internal(&mut conn, None).unwrap();

        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(transporte), "priority 10 wins over priority 0");
    }

    #[test]
    fn manual_categorization_is_never_overwritten() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let mercado = category_id(&conn, "Mercado");
        let transporte = category_id(&conn, "Transporte");

        insert_rule(&conn, "testmerchant", transporte, 0);
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT trip", Some(mercado));

        let n = apply_rules_internal(&mut conn, None).unwrap();
        assert_eq!(n, 0, "manual category preserved");

        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(mercado));
    }

    #[test]
    fn scope_to_account_id() {
        let mut conn = fresh_conn();
        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid) VALUES ('a1', NULL, 'A1'),
                                                                  ('a2', NULL, 'A2')",
            [],
        )
        .unwrap();
        let a1: i64 = conn
            .query_row("SELECT id FROM accounts WHERE ofx_acctid='A1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let a2: i64 = conn
            .query_row("SELECT id FROM accounts WHERE ofx_acctid='A2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let transporte = category_id(&conn, "Transporte");
        insert_rule(&conn, "testmerchant", transporte, 0);
        insert_tx(&conn, a1, "testmerchant a1", None);
        insert_tx(&conn, a2, "testmerchant a2", None);

        let n = apply_rules_internal(&mut conn, Some(a1)).unwrap();
        assert_eq!(n, 1, "scope must limit to account a1");

        let a1_cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE account_id = ?1",
                params![a1],
                |r| r.get(0),
            )
            .unwrap();
        let a2_cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE account_id = ?1",
                params![a2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a1_cat, Some(transporte));
        assert_eq!(a2_cat, None);
    }

    fn cleanup_after_delete(conn: &mut Connection, rule_id: i64) -> u32 {
        super::delete_with_cleanup(conn, rule_id).unwrap()
    }

    #[test]
    fn delete_with_cleanup_clears_matching_txs() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let rule = insert_rule(&conn, "testmerchant", transporte, 0);
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT trip", None);
        apply_rules_internal(&mut conn, None).unwrap();
        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(transporte), "precondition: classified by rule");

        let cleared = cleanup_after_delete(&mut conn, rule);
        assert_eq!(cleared, 1);

        let cat_after: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat_after, None, "category cleared after rule deletion");
    }

    #[test]
    fn delete_with_cleanup_preserves_manual_overrides() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let mercado = category_id(&conn, "Mercado");
        let rule = insert_rule(&conn, "testmerchant", transporte, 0);
        // Manually categorized as Mercado (different from rule's category) — shouldn't be cleared.
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT trip", Some(mercado));

        cleanup_after_delete(&mut conn, rule);

        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(mercado), "manual category Mercado preserved");
    }

    /// The user filed it under the rule's own category by hand; undoing the
    /// rule must not take that away.
    #[test]
    fn delete_with_cleanup_keeps_a_manual_choice_of_the_same_category() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT trip", None);
        crate::commands::transactions::set_category(&conn, tx_id, Some(transporte)).unwrap();
        let rule = insert_rule(&conn, "testmerchant", transporte, 0);

        assert_eq!(cleanup_after_delete(&mut conn, rule), 0);

        let cat: Option<i64> = conn
            .query_row("SELECT category_id FROM transactions WHERE id = ?1", [tx_id], |r| r.get(0))
            .unwrap();
        assert_eq!(cat, Some(transporte));
    }

    #[test]
    fn pattern_with_no_match_does_nothing() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        insert_rule(&conn, "testmerchant", transporte, 0);
        insert_tx(&conn, acc, "padaria do bairro", None);

        let n = apply_rules_internal(&mut conn, None).unwrap();
        assert_eq!(n, 0);
    }

    /// The case that motivated multiple snippets: the same charge shows up
    /// sometimes as an account debit, sometimes as a bill payment.
    #[test]
    fn any_pattern_of_a_rule_matches() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let moradia = category_id(&conn, "Casa");
        insert_rule_multi(
            &conn,
            &["CAIXA ECONOMICA FEDERAL (0104)", "GCI CAIXA - HABITACAO"],
            moradia,
            0,
        );
        let a = insert_tx(&conn, acc, "CAIXA ECONOMICA FEDERAL (0104) Agencia: 37", None);
        let b = insert_tx(
            &conn,
            acc,
            "Pagamento de boleto efetuado - GCI CAIXA - HABITACAO",
            None,
        );
        let c = insert_tx(&conn, acc, "padaria do bairro", None);

        let n = apply_rules_internal(&mut conn, None).unwrap();
        assert_eq!(n, 2, "both formats match the same rule");

        for id in [a, b] {
            let cat: Option<i64> = conn
                .query_row(
                    "SELECT category_id FROM transactions WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(cat, Some(moradia));
        }
        let untouched: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![c],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(untouched, None);
    }

    /// Priority belongs to the RULE, not the snippet: a generic snippet in a
    /// high-priority rule beats a specific one in a low-priority rule.
    #[test]
    fn priority_is_per_rule_not_per_pattern() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let outros = category_id(&conn, "Outros");

        insert_rule_multi(&conn, &["never-matches", "testmerchant"], transporte, 10);
        insert_rule(&conn, "testmerchant trip", outros, 0);

        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT TRIP 99", None);
        apply_rules_internal(&mut conn, None).unwrap();

        let cat: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, Some(transporte));
    }

    /// Deleting a rule must clear what ANY of its snippets categorized.
    #[test]
    fn delete_with_cleanup_clears_every_pattern() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let moradia = category_id(&conn, "Casa");
        let rule = insert_rule_multi(&conn, &["alpha-pattern", "beta-pattern"], moradia, 0);
        let a = insert_tx(&conn, acc, "cobranca ALPHA-PATTERN 1", None);
        let b = insert_tx(&conn, acc, "cobranca BETA-PATTERN 2", None);
        apply_rules_internal(&mut conn, None).unwrap();

        let cleared = cleanup_after_delete(&mut conn, rule);
        assert_eq!(cleared, 2);

        for id in [a, b] {
            let cat: Option<i64> = conn
                .query_row(
                    "SELECT category_id FROM transactions WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(cat, None);
        }
    }

    /// The count is REACH: what the rule matches, even if the transaction now
    /// sits in another category (manual or via a higher-priority rule).
    #[test]
    fn rule_reach_counts_matches_regardless_of_current_category() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let mercado = category_id(&conn, "Mercado");
        let rule = insert_rule_multi(&conn, &["alpha", "beta"], transporte, 0);

        insert_tx(&conn, acc, "compra ALPHA 1", None);
        insert_tx(&conn, acc, "compra BETA 2", None);
        // Manually put in another category — still within the rule's reach.
        insert_tx(&conn, acc, "compra alpha 3", Some(mercado));
        insert_tx(&conn, acc, "padaria", None);
        apply_rules_internal(&mut conn, None).unwrap();

        let reach: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions t
                  WHERE EXISTS (SELECT 1 FROM rule_patterns p
                                 WHERE p.rule_id = ?1
                                   AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%')",
                params![rule],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reach, 3);
    }

    /// Mirrors `preview_rule_application`'s SQL (the command needs State, which
    /// does not exist outside Tauri). If this drifts from that, the tests below
    /// stop meaning anything.
    fn preview(conn: &Connection) -> Vec<(i64, Option<i64>, i64)> {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.category_id, r.category_id
                   FROM transactions t
                   JOIN rules r ON r.id = (
                        SELECT r2.id FROM rules r2
                         WHERE EXISTS (
                               SELECT 1 FROM rule_patterns p
                                WHERE p.rule_id = r2.id
                                  AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%'
                         )
                         ORDER BY r2.priority DESC, r2.created_at DESC
                         LIMIT 1
                   )
                  WHERE (t.category_id IS NULL OR t.category_id <> r.category_id)
                  ORDER BY t.date DESC, t.id DESC",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// The preview shows both classes: uncategorized AND already-categorized
    /// rows that would change — the second is what `apply_rules_internal` skips.
    #[test]
    fn preview_lists_both_uncategorized_and_overrides() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let outros = category_id(&conn, "Outros");
        insert_rule(&conn, "testmerchant", transporte, 0);

        let sem_cat = insert_tx(&conn, acc, "TESTMERCHANT trip 1", None);
        let com_outra = insert_tx(&conn, acc, "TESTMERCHANT trip 2", Some(outros));
        // Already in the category the rule wants: not a change, excluded.
        insert_tx(&conn, acc, "TESTMERCHANT trip 3", Some(transporte));
        // Matches no rule at all.
        insert_tx(&conn, acc, "padaria do bairro", None);

        let rows = preview(&conn);
        let ids: Vec<i64> = rows.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(ids.len(), 2, "only the two that would change");
        assert!(ids.contains(&sem_cat));
        assert!(ids.contains(&com_outra));

        let overrides: Vec<_> = rows.iter().filter(|(_, cur, _)| cur.is_some()).collect();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].2, transporte, "target is the rule's category");
    }

    /// The preview must elect the SAME rule apply would — otherwise the review
    /// promises one thing and the DB writes another.
    #[test]
    fn preview_agrees_with_apply_on_uncategorized() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let outros = category_id(&conn, "Outros");

        insert_rule(&conn, "testmerchant", outros, 0);
        insert_rule(&conn, "testmerchant trip", transporte, 10);
        let tx_id = insert_tx(&conn, acc, "TESTMERCHANT TRIP 9", None);

        let promised = preview(&conn)
            .into_iter()
            .find(|(id, _, _)| *id == tx_id)
            .expect("preview must list the uncategorized transaction")
            .2;

        apply_rules_internal(&mut conn, None).unwrap();
        let actual: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = ?1",
                params![tx_id],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(Some(promised), actual);
        assert_eq!(actual, Some(transporte));
    }

    /// After applying, the uncategorized rows drop out of the preview — only
    /// what needs a human decision remains.
    #[test]
    fn preview_shrinks_to_overrides_after_apply() {
        let mut conn = fresh_conn();
        let acc = insert_account(&conn);
        let transporte = category_id(&conn, "Transporte");
        let mercado = category_id(&conn, "Mercado");
        insert_rule(&conn, "testmerchant", transporte, 0);

        insert_tx(&conn, acc, "TESTMERCHANT a", None);
        let manual = insert_tx(&conn, acc, "TESTMERCHANT b", Some(mercado));

        assert_eq!(preview(&conn).len(), 2);
        apply_rules_internal(&mut conn, None).unwrap();

        let rows = preview(&conn);
        assert_eq!(rows.len(), 1, "the uncategorized transaction was resolved");
        assert_eq!(rows[0].0, manual);
        assert_eq!(rows[0].1, Some(mercado));
        assert_eq!(rows[0].2, transporte);
    }

    /// The dialog's list and the table's count share one criterion. If they
    /// diverge, the number becomes a promise the list does not keep.
    #[test]
    fn matching_list_agrees_with_reach_count() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let casa = category_id(&conn, "Casa");
        let mercado = category_id(&conn, "Mercado");
        let rule = insert_rule_multi(&conn, &["alpha", "beta"], casa, 0);

        insert_tx(&conn, acc, "cobranca ALPHA 1", None);
        insert_tx(&conn, acc, "cobranca BETA 2", Some(mercado));
        insert_tx(&conn, acc, "cobranca alpha 3", Some(casa));
        insert_tx(&conn, acc, "padaria", None);

        let reach: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions t
                  WHERE EXISTS (SELECT 1 FROM rule_patterns p
                                 WHERE p.rule_id = ?1
                                   AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%')",
                params![rule],
                |r| r.get(0),
            )
            .unwrap();

        let listed: Vec<String> = conn
            .prepare(
                "SELECT description FROM transactions t
                  WHERE EXISTS (SELECT 1 FROM rule_patterns p
                                 WHERE p.rule_id = ?1
                                   AND LOWER(t.description) LIKE '%' || LOWER(p.pattern) || '%')
                  ORDER BY t.date DESC, t.id DESC",
            )
            .unwrap()
            .query_map(params![rule], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        assert_eq!(reach, 3);
        assert_eq!(listed.len() as i64, reach, "list and count must agree");
        assert!(!listed.iter().any(|d| d.contains("padaria")));
    }

    #[test]
    fn pay_lead_months_accepts_only_what_the_form_offers() {
        assert!(super::validate_pay_lead_months(0).is_ok());
        assert!(super::validate_pay_lead_months(1).is_ok());
        assert!(super::validate_pay_lead_months(2).is_err());
        assert!(super::validate_pay_lead_months(-1).is_err());
    }

    #[test]
    fn clean_patterns_trims_dedupes_and_rejects_empty() {
        use super::clean_patterns;

        let out = clean_patterns(&[
            "  uber  ".into(),
            "UBER".into(),
            "".into(),
            "   ".into(),
            "99pop".into(),
        ])
        .unwrap();
        assert_eq!(out, vec!["uber".to_string(), "99pop".to_string()]);

        assert!(clean_patterns(&[]).is_err());
        assert!(clean_patterns(&["   ".into()]).is_err());
    }

    fn calendar_fixture() -> (Connection, i64) {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Home', NULL, 'expense')",
            [],
        )
        .unwrap();
        let cat = conn.last_insert_rowid();
        conn.execute("INSERT INTO accounts (name) VALUES ('checking')", [])
            .unwrap();
        (conn, cat)
    }

    fn bill_rule(conn: &Connection, cat: i64, pattern: &str, due_day: i32, lead: i32) -> i64 {
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day, pay_lead_months)
             VALUES (?1, 0, ?2, ?3)",
            params![cat, due_day, lead],
        )
        .unwrap();
        let rule = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, ?2)",
            params![rule, pattern],
        )
        .unwrap();
        rule
    }

    fn payment(conn: &Connection, date: &str, description: &str) -> i64 {
        let account: i64 = conn
            .query_row("SELECT id FROM accounts LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description)
             VALUES (?1, ?2, '-182.40', ?3)",
            params![account, date, description],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// SQLite's own LOWER only folds ASCII, so a rule typed in lowercase with
    /// an accent never categorized the statement's uppercase line — while the
    /// calendar, matching in Rust, already called the bill paid.
    #[test]
    fn a_rule_matches_an_accented_description_in_any_case() {
        let (mut conn, cat) = calendar_fixture();
        crate::db::register_functions(&conn).unwrap();
        bill_rule(&conn, cat, "conta de água", 10, 0);
        let tx = payment(&conn, "2026-10-08", "CONTA DE ÁGUA SABESP");

        super::apply_rules_internal(&mut conn, None).unwrap();

        let got: Option<i64> = conn
            .query_row("SELECT category_id FROM transactions WHERE id = ?1", [tx], |r| r.get(0))
            .unwrap();
        assert_eq!(got, Some(cat));
    }

    #[test]
    fn one_payment_settles_one_bill_only() {
        let (conn, cat) = calendar_fixture();
        bill_rule(&conn, cat, "ENERGIA", 6, 0);
        bill_rule(&conn, cat, "ENERGIA ELETRICA", 6, 0);
        payment(&conn, "2026-08-04", "PAG ENERGIA ELETRICA");

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert_eq!(events.iter().filter(|e| e.paid_date.is_some()).count(), 1);
    }

    #[test]
    fn a_refund_does_not_pay_a_bill() {
        let (conn, cat) = calendar_fixture();
        bill_rule(&conn, cat, "NETFLIX", 6, 0);
        let refund = payment(&conn, "2026-08-04", "Estorno NETFLIX");
        conn.execute("UPDATE transactions SET amount = '39.90' WHERE id = ?1", [refund])
            .unwrap();

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert_eq!(events[0].paid_date, None);
    }

    /// The month becomes a SQL `LIKE` prefix, so anything that reaches it must
    /// pass the strict check and not a looser one alongside it.
    #[test]
    fn the_calendar_rejects_a_month_that_merely_looks_like_one() {
        let (conn, _) = calendar_fixture();
        assert!(super::calendar_events_with_conn(&conn, "20-26-8").is_err());
        assert!(super::calendar_events_with_conn(&conn, "202%-08").is_err());
    }

    #[test]
    fn month_shifted_walks_back_across_a_year() {
        assert_eq!(super::month_shifted("2026-08", 1).unwrap(), "2026-07");
        assert_eq!(super::month_shifted("2026-01", 1).unwrap(), "2025-12");
        assert_eq!(super::month_shifted("2026-08", 0).unwrap(), "2026-08");
    }

    /// The reported bug: the bill due in August is paid in July, and August was
    /// calling it overdue because it looked for the payment in August.
    #[test]
    fn a_lead_of_one_month_lets_july_pay_the_august_bill() {
        let (conn, cat) = calendar_fixture();
        bill_rule(&conn, cat, "ENERGIA", 6, 1);
        payment(&conn, "2026-07-15", "ENERGIA ELETRICA");

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].paid_date.as_deref(), Some("2026-07-15"));
    }

    /// The other half of the same bug: that July payment must NOT also settle
    /// July's own occurrence, which is paid back in June.
    #[test]
    fn a_lead_of_one_month_leaves_july_unpaid_without_a_june_payment() {
        let (conn, cat) = calendar_fixture();
        bill_rule(&conn, cat, "ENERGIA", 6, 1);
        payment(&conn, "2026-07-15", "ENERGIA ELETRICA");

        let events = super::calendar_events_with_conn(&conn, "2026-07").unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].paid_date, None);
    }

    #[test]
    fn without_a_lead_the_payment_settles_its_own_month() {
        let (conn, cat) = calendar_fixture();
        bill_rule(&conn, cat, "ENERGIA", 6, 0);
        payment(&conn, "2026-08-04", "ENERGIA ELETRICA");

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert_eq!(events[0].paid_date.as_deref(), Some("2026-08-04"));
    }

    #[test]
    fn a_manual_settlement_beats_the_derivation() {
        let (conn, cat) = calendar_fixture();
        let rule = bill_rule(&conn, cat, "ENERGIA", 6, 0);
        // A transaction that WOULD derive-match the rule's own pay month — the
        // manual settlement must win over it, not merely exist alongside it.
        payment(&conn, "2026-08-04", "ENERGIA ELETRICA");
        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month) VALUES (?1, '2026-08')",
            [rule],
        )
        .unwrap();

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert!(events[0].manually_settled);
        assert_eq!(events[0].paid_date, None, "paid in cash has no date");
        assert_eq!(events[0].paid_amount, None);
    }

    #[test]
    fn a_manual_settlement_can_point_at_the_transaction_that_paid_it() {
        let (conn, cat) = calendar_fixture();
        let rule = bill_rule(&conn, cat, "ENERGIA", 6, 0);
        let tx = payment(&conn, "2026-07-15", "ENERGIA ELETRICA");
        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month, transaction_id)
             VALUES (?1, '2026-08', ?2)",
            params![rule, tx],
        )
        .unwrap();

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert!(events[0].manually_settled);
        assert_eq!(events[0].paid_date.as_deref(), Some("2026-07-15"));
        assert_eq!(events[0].paid_amount.as_deref(), Some("-182.40"));
    }

    /// A transaction the user has already assigned must not also settle another
    /// occurrence by text similarity — that is the guessing this change removes.
    #[test]
    fn a_transaction_already_assigned_settles_nothing_else() {
        let (conn, cat) = calendar_fixture();
        let rule = bill_rule(&conn, cat, "ENERGIA", 6, 0);
        let tx = payment(&conn, "2026-08-04", "ENERGIA ELETRICA");
        conn.execute(
            "INSERT INTO bill_settlements (rule_id, due_month, transaction_id)
             VALUES (?1, '2026-09', ?2)",
            params![rule, tx],
        )
        .unwrap();

        let events = super::calendar_events_with_conn(&conn, "2026-08").unwrap();

        assert_eq!(events[0].paid_date, None, "August must not claim September's payment");
    }

    /// Clearing a rule's due day destroys its occurrences, so the settlements
    /// hanging off them must go too: otherwise they are unreachable in every
    /// surface — none of which renders an event without a due day — while the
    /// transaction they cite stays out of derivation for every other rule.
    #[test]
    fn clearing_a_rules_due_day_drops_its_settlements() {
        let (mut conn, cat) = calendar_fixture();
        let energia = bill_rule(&conn, cat, "ENERGIA", 6, 0);
        let other = bill_rule(&conn, cat, "ENERGIA", 20, 0);
        let tx = payment(&conn, "2026-08-04", "ENERGIA ELETRICA");
        crate::commands::bills::settle(&conn, energia, "2026-08", Some(tx)).unwrap();

        let before = super::calendar_events_with_conn(&conn, "2026-08").unwrap();
        let other_before = before.iter().find(|e| e.rule_id == other).unwrap();
        assert_eq!(other_before.paid_date, None, "the linked transaction is held out");

        super::update_rule_with_conn(
            &mut conn,
            energia,
            UpdateRule {
                patterns: vec!["ENERGIA".into()],
                category_id: cat,
                priority: 0,
                due_day: None,
                pay_lead_months: 0,
                display_name: None,
            },
        )
        .unwrap();

        let left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM bill_settlements WHERE rule_id = ?1",
                params![energia],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);

        let after = super::calendar_events_with_conn(&conn, "2026-08").unwrap();
        let other_after = after.iter().find(|e| e.rule_id == other).unwrap();
        assert_eq!(other_after.paid_date.as_deref(), Some("2026-08-04"));
    }
}
