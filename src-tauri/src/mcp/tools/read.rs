//! The six read tools.
//!
//! Each one is shaped around a question someone actually asks ("how did my
//! month go?"), not around a table. That is why `get_month_summary` answers
//! with four aggregations at once: an agent that needs four round-trips to
//! describe a month spends its context on plumbing.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::commands::bills::validate_month;
use crate::commands::transactions::TransactionFilters;
use crate::commands::{categories, rules, summary, transactions};
use crate::error::{AppError, AppResult};
use crate::locale::LocalePack;
use crate::mcp::config::McpConfig;

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key).and_then(Value::as_u64).map(|n| n as u32)
}

/// A month argument reaches SQL as a `LIKE` prefix, so it is validated with the
/// same strict check the rest of the app uses — never a looser one here.
fn checked_month(args: &Value) -> AppResult<Option<String>> {
    match opt_str(args, "month") {
        Some(m) => {
            validate_month(&m)?;
            Ok(Some(m))
        }
        None => Ok(None),
    }
}

pub fn list_transactions(conn: &Connection, cfg: &McpConfig, args: &Value) -> AppResult<Value> {
    let month = checked_month(args)?;
    let limit = opt_u32(args, "limit");
    let filters = TransactionFilters {
        account_id: None,
        month,
        category_id: args.get("category_id").and_then(Value::as_i64),
        q: opt_str(args, "q"),
        limit: None,
    };
    let mut rows = transactions::list(conn, &filters)?;

    if let Some(cutoff) = cfg.cutoff() {
        rows.retain(|t| t.date >= cutoff);
    }
    if args.get("uncategorized_only").and_then(Value::as_bool) == Some(true) {
        rows.retain(|t| t.category_id.is_none());
    }
    // `limit` must be the last thing applied: this is a local single-user
    // SQLite database, so fetching a few extra rows to filter in Rust is
    // cheap — but asking SQL to LIMIT before the window/category filters run
    // would silently hand back fewer rows than requested (or than exist).
    if let Some(n) = limit {
        rows.truncate(n as usize);
    }

    Ok(serde_json::to_value(rows)?)
}

/// Oldest month the window still allows, as `YYYY-MM` — the first seven
/// characters of `cutoff()`'s `YYYY-MM-01`. `None` when there is no window.
///
/// `pub(super)` because `write::settle_bill` bounds `due_month` against the
/// same window.
pub(super) fn oldest_allowed_month(cfg: &McpConfig) -> Option<String> {
    cfg.cutoff().map(|c| c[..7].to_string())
}

pub fn get_month_summary(
    conn: &Connection,
    pack: &LocalePack,
    cfg: &McpConfig,
    args: &Value,
) -> AppResult<Value> {
    let month = checked_month(args)?;

    // `income_sources` returns a label per counterparty — who pays the user —
    // which is exactly the kind of detail the window promises to hide. The
    // other three aggregations would silently go all-time too, so a missing
    // or too-old `month` is refused rather than quietly ignoring the window.
    if let Some(oldest) = oldest_allowed_month(cfg) {
        match month.as_deref() {
            None => {
                return Err(AppError::Invalid(format!(
                    "month is required: history before {oldest} is outside the allowed window"
                )));
            }
            Some(m) if m < oldest.as_str() => {
                return Err(AppError::Invalid(format!(
                    "month {m} is older than the allowed window (oldest: {oldest})"
                )));
            }
            _ => {}
        }
    }

    let m = month.as_deref();

    // `investments(...).accumulated_balance` sums the whole history (by its
    // own doc), `income_sources[].recurring_months` counts distinct months
    // across the whole history, and `income_sources[].is_recurring` is a
    // boolean derived from that same all-time count — all three leak rows the
    // window promises the agent cannot see, even for a window-legal month.
    // Stripped here rather than in `summary`, which the Dashboard also reads
    // and must keep seeing all three.
    let mut investments = serde_json::to_value(summary::investments(conn, m)?)?;
    if let Some(obj) = investments.as_object_mut() {
        obj.remove("accumulated_balance");
    }
    let mut income_sources = serde_json::to_value(summary::income(conn, pack, m)?)?;
    if let Some(rows) = income_sources.as_array_mut() {
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("recurring_months");
                obj.remove("is_recurring");
            }
        }
    }

    Ok(json!({
        "kpis": summary::kpis(conn, m)?,
        "by_category": summary::by_category(conn, m)?,
        "income_sources": income_sources,
        "investments": investments,
    }))
}

pub fn get_trend(conn: &Connection, cfg: &McpConfig, args: &Value) -> AppResult<Value> {
    let asked = opt_u32(args, "months_back").unwrap_or(12);
    // The window is a ceiling, not a suggestion: asking for 60 months inside a
    // 12-month window returns 12.
    let months_back = if cfg.window_months == 0 {
        asked
    } else {
        asked.min(cfg.window_months)
    };

    let Some(oldest) = oldest_allowed_month(cfg) else {
        return Ok(serde_json::to_value(summary::by_month(conn, months_back)?)?);
    };

    // `by_month` and the window both count months from the local "now", so
    // dropping what falls before `oldest` is all the guard has to do.
    let mut rows = summary::by_month(conn, months_back)?;
    rows.retain(|r| r.month.as_str() >= oldest.as_str());
    Ok(serde_json::to_value(rows)?)
}

pub fn list_bills(conn: &Connection, cfg: &McpConfig, args: &Value) -> AppResult<Value> {
    let month = opt_str(args, "month")
        .ok_or_else(|| AppError::Invalid("month is required".into()))?;
    validate_month(&month)?;

    // `calendar_events_with_conn` also reads the month immediately before the
    // one asked for (to resolve a lead-1 bill's payment), so the guard must
    // bound that earlier month too, not just the argument — a strict `>`
    // against the oldest allowed month, not `>=`.
    if let Some(oldest) = oldest_allowed_month(cfg) {
        if month.as_str() <= oldest.as_str() {
            return Err(AppError::Invalid(format!(
                "month {month} is older than the allowed window: reading it also reads the \
                 month before it, which would reach before {oldest}"
            )));
        }
    }
    Ok(serde_json::to_value(rules::calendar_events_with_conn(conn, &month)?)?)
}

pub fn list_categories(conn: &Connection, _cfg: &McpConfig, _args: &Value) -> AppResult<Value> {
    Ok(serde_json::to_value(categories::all(conn)?)?)
}

pub fn list_rules(conn: &Connection, _cfg: &McpConfig, _args: &Value) -> AppResult<Value> {
    Ok(serde_json::to_value(rules::all(conn)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::mcp::config::McpConfig;
    use rusqlite::Connection;
    use serde_json::json;

    fn seeded() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn.execute("INSERT INTO accounts (name) VALUES ('Checking')", []).unwrap();
        conn
    }

    fn tx(conn: &Connection, date: &str, amount: &str, description: &str) -> i64 {
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description)
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![date, amount, description],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn cfg(window_months: u32) -> McpConfig {
        let mut c = McpConfig {
            enabled: true,
            tools: Default::default(),
            window_months,
        };
        for (name, _) in crate::mcp::config::TOOL_DEFAULTS {
            c.tools.insert((*name).to_string(), true);
        }
        c
    }

    #[test]
    fn list_transactions_returns_what_is_there() {
        let conn = seeded();
        tx(&conn, "2026-09-01", "-42.00", "MERCADO");

        let out = list_transactions(&conn, &cfg(0), &json!({})).unwrap();

        let rows = out.as_array().expect("an array of transactions");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["description"], "MERCADO");
        assert_eq!(rows[0]["amount"], "-42.00", "money stays a decimal string");
    }

    /// The whole point of the window: what falls outside it does not exist for
    /// the agent, no matter what it asks for.
    #[test]
    fn list_transactions_hides_what_is_outside_the_window() {
        let conn = seeded();
        tx(&conn, "2019-01-05", "-10.00", "ANCIENT");
        tx(&conn, "2026-09-01", "-42.00", "RECENT");

        let out = list_transactions(&conn, &cfg(12), &json!({})).unwrap();

        let descriptions: Vec<&str> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["description"].as_str().unwrap())
            .collect();
        assert!(descriptions.contains(&"RECENT"));
        assert!(!descriptions.contains(&"ANCIENT"), "the window is not decorative");
    }

    #[test]
    fn no_window_shows_the_whole_history() {
        let conn = seeded();
        tx(&conn, "2019-01-05", "-10.00", "ANCIENT");

        let out = list_transactions(&conn, &cfg(0), &json!({})).unwrap();

        assert_eq!(out.as_array().unwrap().len(), 1);
    }

    #[test]
    fn list_transactions_honours_a_limit() {
        let conn = seeded();
        for i in 1..=5 {
            tx(&conn, &format!("2026-09-0{i}"), "-1.00", "X");
        }

        let out = list_transactions(&conn, &cfg(0), &json!({ "limit": 2 })).unwrap();

        assert_eq!(out.as_array().unwrap().len(), 2);
    }

    /// Pins that the window filter and the limit agree on ordering: rows
    /// outside the window sort last under `ORDER BY date DESC`, and `limit` is
    /// a Rust-side `truncate` rather than a SQL `LIMIT`, so there is no clause
    /// ordering for this test to actually depend on — it exists to catch a
    /// regression that would reintroduce one (e.g. a SQL-side `LIMIT`) and
    /// silently start truncating before the window filter runs.
    #[test]
    fn a_limit_is_applied_after_the_window_filter_not_before() {
        let conn = seeded();
        tx(&conn, "2019-01-05", "-1.00", "ANCIENT1");
        tx(&conn, "2019-01-06", "-1.00", "ANCIENT2");
        for i in 1..=4 {
            tx(&conn, &format!("2026-09-0{i}"), "-1.00", "RECENT");
        }

        let out = list_transactions(&conn, &cfg(12), &json!({ "limit": 2 })).unwrap();

        let rows = out.as_array().unwrap();
        assert_eq!(rows.len(), 2, "there are enough recent rows to fill the limit");
        assert!(
            rows.iter().all(|r| r["description"] == "RECENT"),
            "the window must exclude ancient rows, not just shrink the count"
        );
    }

    /// The bug this guards against: `uncategorized_only` is a Rust-side filter
    /// with no correlation to the SQL `ORDER BY date`, so a SQL-side `LIMIT`
    /// can fill its quota with already-categorized rows before the filter
    /// ever runs, undercounting the answer.
    #[test]
    fn a_limit_is_applied_after_uncategorized_only_not_before() {
        let conn = seeded();
        let mercado: i64 = conn
            .query_row("SELECT id FROM categories WHERE name = 'Mercado'", [], |r| r.get(0))
            .unwrap();
        tx(&conn, "2026-09-05", "-1.00", "CAT1");
        tx(&conn, "2026-09-04", "-1.00", "UNCAT1");
        tx(&conn, "2026-09-03", "-1.00", "CAT2");
        tx(&conn, "2026-09-02", "-1.00", "UNCAT2");
        tx(&conn, "2026-09-01", "-1.00", "CAT3");
        conn.execute(
            "UPDATE transactions SET category_id = ?1 WHERE description IN ('CAT1', 'CAT2', 'CAT3')",
            rusqlite::params![mercado],
        )
        .unwrap();

        let out = list_transactions(
            &conn,
            &cfg(0),
            &json!({ "uncategorized_only": true, "limit": 2 }),
        )
        .unwrap();

        let rows = out.as_array().unwrap();
        assert_eq!(rows.len(), 2, "both uncategorized rows must survive the limit");
        assert!(rows.iter().all(|r| r["category_id"].is_null()));
    }

    /// One call instead of four: the shape is the reason these tools exist.
    #[test]
    fn get_month_summary_answers_in_one_payload() {
        let conn = seeded();
        tx(&conn, "2026-09-01", "-42.00", "MERCADO");
        tx(&conn, "2026-09-02", "3000.00", "SALARY");
        let pack = crate::locale::LocalePack::embedded_pt_br();

        let out = get_month_summary(&conn, &pack, &cfg(0), &json!({ "month": "2026-09" })).unwrap();

        assert!(out.get("kpis").is_some());
        assert!(out.get("by_category").is_some());
        assert!(out.get("income_sources").is_some());
        assert!(out.get("investments").is_some());
        assert_eq!(out["kpis"]["expense"], "42.00");
    }

    #[test]
    fn get_month_summary_needs_a_real_month() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();

        let out = get_month_summary(&conn, &pack, &cfg(0), &json!({ "month": "setembro" }));

        assert!(out.is_err(), "a malformed month must not reach SQL as a LIKE prefix");
    }

    /// `income_sources` scans full history to detect recurrence and returns a
    /// label per counterparty — an all-time summary would leak exactly what
    /// the window promises to hide.
    #[test]
    fn get_month_summary_without_a_month_is_refused_when_a_window_is_set() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();

        let err = get_month_summary(&conn, &pack, &cfg(12), &json!({})).unwrap_err();

        assert!(
            err.to_string().contains("window"),
            "the error must name the window as the reason: {err}"
        );
    }

    #[test]
    fn get_month_summary_for_a_month_older_than_the_window_is_refused() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();

        let out = get_month_summary(&conn, &pack, &cfg(12), &json!({ "month": "2019-01" }));

        assert!(out.is_err(), "2019-01 is far outside any realistic 12-month window");
    }

    #[test]
    fn get_month_summary_for_a_month_inside_the_window_still_works() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();
        let this_month = chrono::Local::now().format("%Y-%m").to_string();

        let out = get_month_summary(&conn, &pack, &cfg(12), &json!({ "month": this_month })).unwrap();

        assert!(out.get("kpis").is_some());
    }

    /// The escape hatch survives: a window of `0` means no limit, so `month`
    /// stays optional and omitting it means all time, same as before this fix.
    #[test]
    fn get_month_summary_with_no_window_still_allows_no_month() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();

        let out = get_month_summary(&conn, &pack, &cfg(0), &json!({}));

        assert!(out.is_ok(), "no window configured means all-time stays available");
    }

    #[test]
    fn get_trend_returns_a_month_series() {
        let conn = seeded();
        tx(&conn, "2026-09-01", "-42.00", "MERCADO");

        let out = get_trend(&conn, &cfg(0), &json!({ "months_back": 3 })).unwrap();

        assert!(out.as_array().is_some());
    }

    #[test]
    fn list_categories_comes_from_the_active_pack() {
        let conn = seeded();

        let out = list_categories(&conn, &cfg(0), &json!({})).unwrap();

        assert!(!out.as_array().unwrap().is_empty());
        assert!(out.as_array().unwrap()[0].get("name").is_some());
    }

    #[test]
    fn list_rules_is_empty_until_there_are_rules() {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();

        let out = list_rules(&conn, &cfg(0), &json!({})).unwrap();

        assert!(out.as_array().unwrap().is_empty());
    }

    #[test]
    fn list_bills_needs_a_real_month() {
        let conn = seeded();

        assert!(list_bills(&conn, &cfg(0), &json!({ "month": "2026-9" })).is_err());
    }

    #[test]
    fn list_bills_with_no_window_reaches_any_month() {
        let conn = seeded();

        assert!(list_bills(&conn, &cfg(0), &json!({ "month": "2019-01" })).is_ok());
    }

    /// The bug this guards against: `list_bills` ignored `cfg` entirely, so an
    /// agent could walk the whole history two months at a time even though the
    /// tool is on by default.
    #[test]
    fn list_bills_refuses_a_month_outside_the_window() {
        let conn = seeded();

        let out = list_bills(&conn, &cfg(12), &json!({ "month": "2019-01" }));

        assert!(out.is_err(), "2019-01 is far outside any realistic 12-month window");
    }

    /// `calendar_events_with_conn` reads `month` AND `month - 1`, so a month
    /// equal to the oldest allowed one would still leak the month before it —
    /// the guard must refuse that month too, not just anything strictly older.
    #[test]
    fn list_bills_refuses_the_oldest_allowed_month_because_it_reads_one_month_before_it() {
        let conn = seeded();
        let cfg12 = cfg(12);
        let oldest = oldest_allowed_month(&cfg12).unwrap();

        let out = list_bills(&conn, &cfg12, &json!({ "month": oldest }));

        assert!(out.is_err(), "the oldest allowed month still reads one month before it");
    }

    #[test]
    fn list_bills_for_the_month_after_the_oldest_allowed_one_still_works() {
        let conn = seeded();
        let cfg12 = cfg(12);
        let oldest = oldest_allowed_month(&cfg12).unwrap();
        let (y, m): (i32, u32) = {
            let (y, m) = oldest.split_once('-').unwrap();
            (y.parse().unwrap(), m.parse().unwrap())
        };
        let next = chrono::NaiveDate::from_ymd_opt(y, m, 1)
            .unwrap()
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
            .format("%Y-%m")
            .to_string();

        assert!(list_bills(&conn, &cfg12, &json!({ "month": next })).is_ok());
    }

    /// `accumulated_balance` sums the whole history, `recurring_months`
    /// counts distinct months across the whole history, and `is_recurring` is
    /// a boolean derived from that same all-time count — all three leak
    /// exactly what the window promises to hide, even for a month inside it.
    #[test]
    fn get_month_summary_omits_all_time_figures() {
        let conn = seeded();
        let pack = crate::locale::LocalePack::embedded_pt_br();
        tx(&conn, "2026-09-01", "3000.00", "SALARY");

        let out = get_month_summary(&conn, &pack, &cfg(12), &json!({ "month": "2026-09" })).unwrap();

        assert!(
            out["investments"].get("accumulated_balance").is_none(),
            "accumulated_balance is all-time, the window must not leak it"
        );
        for source in out["income_sources"].as_array().unwrap() {
            assert!(
                source.get("recurring_months").is_none(),
                "recurring_months is all-time, the window must not leak it"
            );
            assert!(
                source.get("is_recurring").is_none(),
                "is_recurring is derived from the all-time count, the window must not leak it either"
            );
        }
    }

    #[test]
    fn get_trend_never_returns_a_month_older_than_the_window() {
        let conn = seeded();
        let cfg12 = cfg(12);
        let oldest = oldest_allowed_month(&cfg12).unwrap();
        // One month exactly at the boundary the bug used to leak (one older
        // than the window), one exactly at the oldest allowed edge.
        let (y, m): (i32, u32) = {
            let (y, m) = oldest.split_once('-').unwrap();
            (y.parse().unwrap(), m.parse().unwrap())
        };
        let leaked = chrono::NaiveDate::from_ymd_opt(y, m, 1)
            .unwrap()
            .checked_sub_months(chrono::Months::new(1))
            .unwrap();
        tx(&conn, &format!("{}-01", leaked.format("%Y-%m")), "-1.00", "LEAKED");
        tx(&conn, &format!("{oldest}-01"), "-1.00", "OLDEST_ALLOWED");

        let out = get_trend(&conn, &cfg12, &json!({ "months_back": 12 })).unwrap();

        let months: Vec<&str> =
            out.as_array().unwrap().iter().map(|r| r["month"].as_str().unwrap()).collect();
        assert!(
            months.iter().all(|m| *m >= oldest.as_str()),
            "no row may be older than the window: {months:?}"
        );
        assert!(months.contains(&oldest.as_str()), "the oldest allowed month must still show up");
    }

    /// The bug this guards against: padding the underlying query to absorb
    /// the window's Utc/Local skew also widened what `months_back` itself
    /// answers — `months_back: 3` came back with 5 rows instead of the 4
    /// `by_month`'s own contract gives (the current month plus 3 previous).
    /// A window wide enough not to trim anything isolates that the request,
    /// not the window, is what should bound the answer here.
    #[test]
    fn get_trend_does_not_answer_more_than_the_request_implies() {
        let conn = seeded();
        let today = chrono::Local::now().date_naive();
        for i in 0..=5u32 {
            let month = today
                .checked_sub_months(chrono::Months::new(i))
                .unwrap()
                .format("%Y-%m")
                .to_string();
            tx(&conn, &format!("{month}-01"), "-1.00", "X");
        }

        let out = get_trend(&conn, &cfg(24), &json!({ "months_back": 3 })).unwrap();

        let rows = out.as_array().unwrap();
        assert_eq!(
            rows.len(),
            4,
            "current month plus 3 previous — by_month's own contract, not padded further: {rows:?}"
        );
    }
}
