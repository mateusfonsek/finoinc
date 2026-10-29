use rusqlite::params;
use rusqlite::Connection;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use tauri::State;

use crate::commands::suggestions::normalize;
use crate::db::Db;
use crate::domain::summary::{
    CategorySpend, IncomeSource, InvestmentSummary, KpiSummary, MonthSummary, TransferSummary,
};
use crate::error::{AppError, AppResult};

pub fn kpis(conn: &Connection, month: Option<&str>) -> AppResult<KpiSummary> {
    let pattern: Option<String> = month.map(|m| format!("{m}-%"));

    // Excludes tx whose category.kind = 'transfer' (internal moves like paying
    // a card bill or moving money to savings). A NULL category counts normally.
    let mut stmt = conn.prepare(
        "SELECT t.amount, c.kind
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE (?1 IS NULL OR t.date LIKE ?1)
           AND COALESCE(c.kind, '') != 'transfer'",
    )?;
    let pat_ref: Option<&str> = pattern.as_deref();
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map(params![pat_ref], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut income = Decimal::ZERO;
    let mut expense = Decimal::ZERO;
    let count = rows.len() as u32;
    for (s, kind) in &rows {
        let d = Decimal::from_str(s)
            .map_err(|e| AppError::Invalid(format!("bad amount '{s}': {e}")))?;
        let (i, e) = flow(d, kind.as_deref());
        income += i;
        expense += e;
    }
    let net = income - expense;

    Ok(KpiSummary {
        income: income.to_string(),
        expense: expense.to_string(),
        net: net.to_string(),
        transaction_count: count,
    })
}

/// Splits an amount into (income, expense) by its category's kind. A refund in
/// an expense category gives spending back instead of counting as income;
/// an uncategorized row goes by its sign.
fn flow(amount: Decimal, kind: Option<&str>) -> (Decimal, Decimal) {
    match kind {
        Some("expense") => (Decimal::ZERO, -amount),
        Some("income") => (amount, Decimal::ZERO),
        _ if amount.is_sign_negative() => (Decimal::ZERO, -amount),
        _ => (amount, Decimal::ZERO),
    }
}

#[tauri::command]
#[specta::specta]
pub fn summary_kpis(db: State<'_, Db>, month: Option<String>) -> AppResult<KpiSummary> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    kpis(&conn, month.as_deref())
}

pub fn by_category(conn: &Connection, month: Option<&str>) -> AppResult<Vec<CategorySpend>> {
    let pattern = month.map(|m| format!("{m}-%"));

    let mut stmt = conn.prepare(
        "SELECT t.amount, t.category_id, c.name, c.color_token, c.kind
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE (?1 IS NULL OR t.date LIKE ?1)
           AND COALESCE(c.kind, '') != 'transfer'",
    )?;
    type CategoryRow = (String, Option<i64>, Option<String>, Option<String>, Option<String>);
    let pat_for_query: Option<&str> = pattern.as_deref();
    let rows: Vec<CategoryRow> = stmt
        .query_map(params![pat_for_query], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut by: HashMap<Option<i64>, (String, Option<String>, Decimal)> = HashMap::new();

    for (amt, cat_id, name, color, kind) in rows {
        let d =
            Decimal::from_str(&amt).map_err(|e| AppError::Invalid(format!("bad amount: {e}")))?;
        let spends = match kind.as_deref() {
            Some(k) => k == "expense",
            None => d.is_sign_negative(),
        };
        if !spends {
            continue;
        }
        // No label invented here: `category_id: None` already means
        // "uncategorized", and the wording belongs to the locale pack.
        let display_name = name.unwrap_or_default();
        let entry = by
            .entry(cat_id)
            .or_insert((display_name, color, Decimal::ZERO));
        entry.2 += flow(d, kind.as_deref()).1;
    }
    // A category whose refunds outweigh its purchases this month has no slice.
    by.retain(|_, (_, _, total)| total.is_sign_positive() && !total.is_zero());
    let total_expense: Decimal = by.values().map(|(_, _, total)| *total).sum();

    let mut out: Vec<CategorySpend> = by
        .into_iter()
        .map(|(cat_id, (name, color, total))| {
            let percent = if total_expense.is_zero() {
                0.0
            } else {
                let p: f64 = (total / total_expense).to_string().parse().unwrap_or(0.0);
                p * 100.0
            };
            CategorySpend {
                category_id: cat_id,
                name,
                color_token: color,
                total: total.to_string(),
                percent,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        let ad: Decimal = Decimal::from_str(&a.total).unwrap_or(Decimal::ZERO);
        let bd: Decimal = Decimal::from_str(&b.total).unwrap_or(Decimal::ZERO);
        bd.cmp(&ad)
    });

    Ok(out)
}

#[tauri::command]
#[specta::specta]
pub fn summary_by_category(
    db: State<'_, Db>,
    month: Option<String>,
) -> AppResult<Vec<CategorySpend>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    by_category(&conn, month.as_deref())
}

const MAX_MONTHS_BACK: u32 = 1200;

/// One entry per month from `months_back` months ago through the current one,
/// empty months included, so a chart can give each month its own column.
pub fn by_month(conn: &Connection, months_back: u32) -> AppResult<Vec<MonthSummary>> {
    let today = chrono::Local::now().date_naive();
    let (year, month) = (chrono::Datelike::year(&today), chrono::Datelike::month(&today));
    // A series is built month by month, so an absurd request (an agent asking
    // for u32::MAX) is capped at a century instead of allocating forever.
    let back = months_back.min(MAX_MONTHS_BACK) as i32;
    let first = shift_month(year, month, -back);
    let cutoff = format!("{first}-01");
    let end = format!("{}-01", shift_month(year, month, 1));

    let mut stmt = conn.prepare(
        "SELECT substr(t.date, 1, 7) AS month, t.amount, c.kind
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.date >= ?1 AND t.date < ?2
           AND COALESCE(c.kind, '') != 'transfer'
         ORDER BY t.date ASC",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map(params![cutoff, end], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut by_month: HashMap<String, (Decimal, Decimal)> = HashMap::new();
    for (m, amt, kind) in rows {
        let d =
            Decimal::from_str(&amt).map_err(|e| AppError::Invalid(format!("bad amount: {e}")))?;
        let entry = by_month.entry(m).or_insert((Decimal::ZERO, Decimal::ZERO));
        let (i, e) = flow(d, kind.as_deref());
        entry.0 += i;
        entry.1 += e;
    }

    Ok((-back..=0)
        .map(|delta| {
            let month = shift_month(year, month, delta);
            let (income, expense) = by_month.get(&month).copied().unwrap_or_default();
            MonthSummary {
                month,
                income: income.to_string(),
                expense: expense.to_string(),
            }
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn summary_by_month(db: State<'_, Db>, months_back: u32) -> AppResult<Vec<MonthSummary>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    by_month(&conn, months_back)
}

pub fn investments(conn: &Connection, month: Option<&str>) -> AppResult<InvestmentSummary> {
    let pattern: Option<String> = month.map(|m| format!("{m}-%"));

    // Month aggregation.
    let mut stmt = conn.prepare(
        "SELECT t.amount
         FROM transactions t
         JOIN categories c ON c.id = t.category_id
         WHERE c.is_investment = 1 AND c.kind = 'transfer'
           AND (?1 IS NULL OR t.date LIKE ?1)",
    )?;
    let pat_ref: Option<&str> = pattern.as_deref();
    let month_rows: Vec<String> = stmt
        .query_map(params![pat_ref], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut applied = Decimal::ZERO;
    let mut redeemed = Decimal::ZERO;
    let mut applications = 0u32;
    let mut redemptions = 0u32;
    for s in &month_rows {
        let d = Decimal::from_str(s)
            .map_err(|e| AppError::Invalid(format!("bad amount '{s}': {e}")))?;
        if d.is_sign_negative() {
            applied += -d;
            applications += 1;
        } else if !d.is_zero() {
            redeemed += d;
            redemptions += 1;
        }
    }

    // All-time running balance, aggregated as Decimal in Rust to keep monetary
    // precision (rust_decimal does not pair well with SQL SUM over TEXT).
    let balance: Decimal = {
        let mut all_stmt = conn.prepare(
            "SELECT t.amount FROM transactions t
             JOIN categories c ON c.id = t.category_id
             WHERE c.is_investment = 1 AND c.kind = 'transfer'",
        )?;
        let all_rows: Vec<String> = all_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut s = Decimal::ZERO;
        for amt in &all_rows {
            let d = Decimal::from_str(amt)
                .map_err(|e| AppError::Invalid(format!("bad amount '{amt}': {e}")))?;
            // A negative amount (a deposit) flips positive and raises the
            // invested balance; a positive amount (a redemption) lowers it.
            s += -d;
        }
        s
    };

    Ok(InvestmentSummary {
        applied_in_month: applied.to_string(),
        redeemed_in_month: redeemed.to_string(),
        applications_count: applications,
        redemptions_count: redemptions,
        accumulated_balance: balance.to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn investment_summary(
    db: State<'_, Db>,
    month: Option<String>,
) -> AppResult<InvestmentSummary> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    investments(&conn, month.as_deref())
}

#[tauri::command]
#[specta::specta]
pub fn transfer_summary(
    db: State<'_, Db>,
    month: Option<String>,
) -> AppResult<TransferSummary> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let pattern: Option<String> = month.as_ref().map(|m| format!("{m}-%"));

    let mut stmt = conn.prepare(
        "SELECT t.amount
         FROM transactions t
         JOIN categories c ON c.id = t.category_id
         WHERE c.kind = 'transfer'
           AND c.is_investment = 0
           AND (?1 IS NULL OR t.date LIKE ?1)",
    )?;
    let pat_ref: Option<&str> = pattern.as_deref();
    let rows: Vec<String> = stmt
        .query_map(params![pat_ref], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut total_out = Decimal::ZERO;
    let mut total_in = Decimal::ZERO;
    let count = rows.len() as u32;
    for s in &rows {
        let d = Decimal::from_str(s)
            .map_err(|e| AppError::Invalid(format!("bad amount '{s}': {e}")))?;
        if d.is_sign_negative() {
            total_out += -d;
        } else {
            total_in += d;
        }
    }

    Ok(TransferSummary {
        total_out: total_out.to_string(),
        total_in: total_in.to_string(),
        count,
    })
}

/// Aggregates the month's inflows (`amount > 0`, kind not transfer or expense) by
/// counterparty. Marks a source recurring when it also appeared in BOTH months
/// immediately before the displayed one (see `is_recurring`).
pub fn income(
    conn: &Connection,
    pack: &crate::locale::LocalePack,
    month: Option<&str>,
) -> AppResult<Vec<IncomeSource>> {
    // Loads ALL real inflows from the whole DB — the full history is needed to
    // detect recurrence. A positive row in an expense category is a refund.
    let mut stmt = conn.prepare(
        "SELECT t.date, t.amount, t.description
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE COALESCE(c.kind, '') NOT IN ('transfer', 'expense')
           AND CAST(t.amount AS REAL) > 0",
    )?;
    type Row = (String, String, String);
    let rows: Vec<Row> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let target_prefix = month.map(|m| format!("{m}-"));

    // Per normalized key, tracks the distinct months it appeared in (for
    // recurrence) and the filtered month's aggregate (total, count, label).
    struct Agg {
        label: String,
        total: Decimal,
        count: u32,
    }
    let mut months_seen: HashMap<String, HashSet<String>> = HashMap::new();
    let mut current: HashMap<String, Agg> = HashMap::new();

    for (date, amount_str, desc) in rows {
        let (key, label, _pattern) = normalize(&desc, pack);
        let month_key: &str = if date.len() >= 7 { &date[..7] } else { &date };

        months_seen
            .entry(key.clone())
            .or_default()
            .insert(month_key.to_string());

        let in_month = match target_prefix.as_deref() {
            Some(p) => date.starts_with(p),
            None => true,
        };
        if !in_month {
            continue;
        }
        let amt = Decimal::from_str(&amount_str)
            .map_err(|e| AppError::Invalid(format!("bad amount '{amount_str}': {e}")))?;
        let entry = current.entry(key).or_insert_with(|| Agg {
            label,
            total: Decimal::ZERO,
            count: 0,
        });
        entry.total += amt;
        entry.count += 1;
    }

    let total_income: Decimal = current.values().map(|a| a.total).sum();

    let mut sources: Vec<IncomeSource> = current
        .into_iter()
        .map(|(key, agg)| {
            let seen = months_seen.get(&key);
            let recurring_months = seen.map(|s| s.len()).unwrap_or(0) as u32;
            let recurring = seen.is_some_and(|s| is_recurring(s, month));
            let percent = if total_income.is_zero() {
                0.0
            } else {
                let ratio: f64 = (agg.total / total_income).to_string().parse().unwrap_or(0.0);
                ratio * 100.0
            };
            IncomeSource {
                key,
                label: agg.label,
                total: agg.total.to_string(),
                count: agg.count,
                percent,
                is_recurring: recurring,
                recurring_months,
            }
        })
        .collect();
    sources.sort_by(|a, b| {
        let ad: Decimal = Decimal::from_str(&a.total).unwrap_or(Decimal::ZERO);
        let bd: Decimal = Decimal::from_str(&b.total).unwrap_or(Decimal::ZERO);
        bd.cmp(&ad)
    });
    Ok(sources)
}

#[tauri::command]
#[specta::specta]
pub fn income_sources(
    db: State<'_, Db>,
    locale: State<'_, crate::locale::LocaleState>,
    month: Option<String>,
) -> AppResult<Vec<IncomeSource>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let pack = locale.pack.lock().expect("locale mutex poisoned");
    income(&conn, &pack, month.as_deref())
}

/// Shifts a year/month by `delta` months, handling year rollover. Returns
/// "YYYY-MM".
fn shift_month(year: i32, month: u32, delta: i32) -> String {
    let zero_based = year * 12 + (month as i32 - 1) + delta;
    let y = zero_based.div_euclid(12);
    let m = zero_based.rem_euclid(12) + 1;
    format!("{:04}-{:02}", y, m)
}

/// For a "YYYY-MM" month, returns (M-1, M-2). `None` when the string is not a
/// valid month (e.g. a year-only filter or empty).
fn prev_two_months(month: &str) -> Option<(String, String)> {
    let (y, m) = month.split_once('-')?;
    if m.len() != 2 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let mon: u32 = m.parse().ok()?;
    if !(1..=12).contains(&mon) {
        return None;
    }
    Some((shift_month(year, mon, -1), shift_month(year, mon, -2)))
}

/// Recurrence relative to the displayed month `anchor`: a source is recurring
/// only if it appeared in BOTH immediately preceding months (M-1 and M-2). A
/// gap in either breaks it. With no month anchor (year filter or "all"), falls
/// back to the coarse signal: >= 2 distinct months in history.
fn is_recurring(months_seen: &HashSet<String>, anchor: Option<&str>) -> bool {
    match anchor.and_then(prev_two_months) {
        Some((m1, m2)) => months_seen.contains(&m1) && months_seen.contains(&m2),
        None => months_seen.len() >= 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
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
        date: &str,
        amount: &str,
        category_id: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description, category_id, ofx_fitid)
             VALUES (?1, ?2, ?3, 'desc', ?4, NULL)",
            params![account_id, date, amount, category_id],
        )
        .unwrap();
    }

    fn cat_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM categories WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// The dashboard draws one column per month: the series has to be exactly
    /// as long as asked, end at the current month and keep empty months.
    #[test]
    fn by_month_is_a_dense_series_ending_this_month() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let today = chrono::Local::now().date_naive();
        let this_month = today.format("%Y-%m").to_string();
        let next_year = format!("{}-01-15", chrono::Datelike::year(&today) + 1);
        insert_tx(&conn, acc, &format!("{this_month}-01"), "-10.00", None);
        insert_tx(&conn, acc, &next_year, "-99.00", None);

        let months = by_month(&conn, 11).unwrap();

        assert_eq!(months.len(), 12);
        assert_eq!(months.last().unwrap().month, this_month);
        assert_eq!(months.last().unwrap().expense, "10.00");
        assert_eq!(months[0].expense, "0");
    }

    /// A refund filed under an expense category gives back what the purchase
    /// spent; it is not income.
    #[test]
    fn a_refund_in_an_expense_category_cancels_the_purchase() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let market = cat_id(&conn, "Mercado");
        insert_tx(&conn, acc, "2026-04-05", "-200.00", Some(market));
        insert_tx(&conn, acc, "2026-04-08", "200.00", Some(market));
        insert_tx(&conn, acc, "2026-04-09", "-50.00", Some(market));

        let k = kpis(&conn, Some("2026-04")).unwrap();
        assert_eq!((k.income.as_str(), k.expense.as_str()), ("0", "50.00"));
        let cats = by_category(&conn, Some("2026-04")).unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].total, "50.00");
        let months = by_month(&conn, 1200).unwrap();
        let april = months.iter().find(|m| m.month == "2026-04").unwrap();
        assert_eq!((april.income.as_str(), april.expense.as_str()), ("0", "50.00"));
        let pack = crate::locale::LocalePack::embedded_pt_br();
        assert!(income(&conn, &pack, Some("2026-04")).unwrap().is_empty());
    }

    /// Changing Investimentos to an expense makes its rows ordinary spending;
    /// they must not also stay in the investments panel.
    #[test]
    fn an_investment_category_turned_into_an_expense_counts_once() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let inv = cat_id(&conn, "Investimentos");
        conn.execute("UPDATE categories SET kind = 'expense' WHERE id = ?1", [inv])
            .unwrap();
        insert_tx(&conn, acc, "2026-04-05", "-1000.00", Some(inv));

        assert_eq!(kpis(&conn, Some("2026-04")).unwrap().expense, "1000.00");
        assert_eq!(investments(&conn, Some("2026-04")).unwrap().applied_in_month, "0");
    }

    #[test]
    fn kpis_sum_correctly_for_month() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        insert_tx(&conn, acc, "2026-04-05", "100.00", None);
        insert_tx(&conn, acc, "2026-04-10", "-30.00", None);
        insert_tx(&conn, acc, "2026-04-15", "-20.50", None);
        insert_tx(&conn, acc, "2026-03-30", "999.00", None);

        let mut stmt = conn
            .prepare("SELECT amount FROM transactions WHERE date LIKE ?1")
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map(params!["2026-04-%"], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let mut income = Decimal::ZERO;
        let mut expense = Decimal::ZERO;
        for s in &rows {
            let d: Decimal = s.parse().unwrap();
            if d.is_sign_negative() {
                expense += -d;
            } else {
                income += d;
            }
        }
        assert_eq!(income, Decimal::from_str("100.00").unwrap());
        assert_eq!(expense, Decimal::from_str("50.50").unwrap());
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn by_category_groups_only_expenses() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let mercado = cat_id(&conn, "Mercado");

        insert_tx(&conn, acc, "2026-04-05", "-50.00", Some(mercado));
        insert_tx(&conn, acc, "2026-04-10", "-30.00", Some(mercado));
        // Positive tx with no category — income is uncategorized since 0008.
        insert_tx(&conn, acc, "2026-04-15", "5000.00", None);
        insert_tx(&conn, acc, "2026-04-20", "-10.00", None);

        let mut stmt = conn
            .prepare(
                "SELECT t.amount, t.category_id
                 FROM transactions t WHERE t.date LIKE ?1",
            )
            .unwrap();
        let rows: Vec<(String, Option<i64>)> = stmt
            .query_map(params!["2026-04-%"], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        let mut mercado_total = Decimal::ZERO;
        let mut sem_cat_total = Decimal::ZERO;
        let mut renda_count = 0u32;
        for (amt, cid) in &rows {
            let d: Decimal = amt.parse().unwrap();
            if !d.is_sign_negative() {
                renda_count += 1;
                continue;
            }
            let abs = -d;
            if *cid == Some(mercado) {
                mercado_total += abs;
            } else if cid.is_none() {
                sem_cat_total += abs;
            }
        }

        assert_eq!(mercado_total, Decimal::from_str("80.00").unwrap());
        assert_eq!(sem_cat_total, Decimal::from_str("10.00").unwrap());
        assert_eq!(renda_count, 1);
    }

    #[test]
    fn kpis_exclude_transfer_kind_categories() {
        let conn = fresh_conn();
        let acc = insert_account(&conn);
        let transferencias = cat_id(&conn, "Transferências");
        let mercado = cat_id(&conn, "Mercado");

        // Real spending and income, plus a transfer that should be ignored.
        // Income stays uncategorized (categories are for spending).
        insert_tx(&conn, acc, "2026-04-05", "-100.00", Some(mercado));
        insert_tx(&conn, acc, "2026-04-10", "5000.00", None);
        insert_tx(&conn, acc, "2026-04-15", "-732.52", Some(transferencias)); // pagamento de fatura
        insert_tx(&conn, acc, "2026-04-20", "-1000.00", Some(transferencias)); // RDB deposit

        let mut stmt = conn
            .prepare(
                "SELECT t.amount FROM transactions t
                 LEFT JOIN categories c ON c.id = t.category_id
                 WHERE t.date LIKE ?1 AND COALESCE(c.kind, '') != 'transfer'",
            )
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map(params!["2026-04-%"], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        let mut income = Decimal::ZERO;
        let mut expense = Decimal::ZERO;
        for s in &rows {
            let d: Decimal = s.parse().unwrap();
            if d.is_sign_negative() {
                expense += -d;
            } else {
                income += d;
            }
        }
        assert_eq!(income, Decimal::from_str("5000.00").unwrap());
        assert_eq!(expense, Decimal::from_str("100.00").unwrap(),
            "transfers (-732.52 -1000.00) must be ignored");
        assert_eq!(rows.len(), 2);
    }

    fn seen(months: &[&str]) -> HashSet<String> {
        months.iter().map(|m| m.to_string()).collect()
    }

    #[test]
    fn recurring_requires_both_preceding_months() {
        // Pix in Jan, Feb and Mar. In March it is recurring (Jan and Feb present).
        let s = seen(&["2026-01", "2026-02", "2026-03"]);
        assert!(is_recurring(&s, Some("2026-03")));
    }

    #[test]
    fn recurring_breaks_on_gap() {
        // Pix in Mar and May, skipped Apr. In May → NOT recurring.
        let s = seen(&["2026-03", "2026-05"]);
        assert!(!is_recurring(&s, Some("2026-05")));
    }

    #[test]
    fn recurring_handles_year_boundary() {
        // Nov/dez 2025 + jan 2026. Em jan/2026 → recorrente (dez+nov presentes).
        let s = seen(&["2025-11", "2025-12", "2026-01"]);
        assert!(is_recurring(&s, Some("2026-01")));
    }

    #[test]
    fn recurring_fallback_without_month_anchor() {
        // No month anchor (all) or a year filter: >= 2 distinct months.
        let s = seen(&["2026-01", "2026-06"]);
        assert!(is_recurring(&s, None));
        assert!(is_recurring(&s, Some("2026")));
        assert!(!is_recurring(&seen(&["2026-01"]), None));
    }

    // Compared as an exact key set, not by substring: a substring check still
    // passes if a Portuguese name coexists (e.g. via `#[serde(alias)]` or an
    // extra field), so it can't catch a name that was never actually removed.
    #[test]
    fn investment_summary_fields_serialize_in_english() {
        let s = InvestmentSummary {
            applied_in_month: "10.00".into(),
            redeemed_in_month: "0".into(),
            applications_count: 1,
            redemptions_count: 0,
            accumulated_balance: "10.00".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "accumulated_balance",
                "applications_count",
                "applied_in_month",
                "redeemed_in_month",
                "redemptions_count",
            ]
        );
    }
}
