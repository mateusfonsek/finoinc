//! Tax-id enrichment orchestration.
//!
//! Deliberately separate from the commands: there is no `State`, `AppHandle` or
//! `Channel` here. Only the logic — which is what makes it possible to test
//! event ordering, failure resilience and cancellation without touching the
//! network or booting a Tauri app.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::params;
use serde::Serialize;
use specta::Type;

use crate::commands::tax_id::TaxIdResolution;
use crate::commands::rules::apply_rules_internal;
use crate::commands::suggestions::AutoClassifyReport;
use crate::domain::rule::Rule;
use crate::enrich::provider::TaxIdProvider;
use crate::enrich::{extract_tax_id, lookup_with};
use crate::error::AppResult;
use crate::locale::LocalePack;

/// What the background thread tells the interface, in order.
///
/// `Started` carries the denominator — that is what buys the determinate bar
/// the HIG prefers, with no invented estimate, because the list of unique tax
/// ids is known before the first lookup.
///
/// `Failed` and `Aborted` are failures of different natures and the interface
/// needs to tell them apart: a lookup that went wrong inside a loop that keeps
/// going is not the work having died.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind")]
pub enum EnrichEvent {
    Started { total: u32 },
    Resolved { done: u32, label: String, rule: Rule },
    Unresolved { done: u32, resolution: TaxIdResolution },
    Failed { done: u32, tax_id: String },
    Finished { report: AutoClassifyReport },
    Cancelled { report: AutoClassifyReport },
    Aborted { message: String },
}

/// Distinct tax ids in the descriptions of uncategorized outflows.
///
/// Sorted alphabetically because `HashSet` has no order: without this, the same
/// statement would produce a different progress sequence on every run, and the
/// event-ordering tests would be flaky.
pub fn collect_unique_tax_ids(
    conn: &rusqlite::Connection,
    pack: &LocalePack,
    account_id: Option<i64>,
) -> AppResult<Vec<String>> {
    let (sql, scoped) = match account_id {
        Some(_) => (
            "SELECT description FROM transactions
             WHERE category_id IS NULL
               AND CAST(amount AS REAL) < 0
               AND account_id = ?1",
            true,
        ),
        None => (
            "SELECT description FROM transactions
             WHERE category_id IS NULL
               AND CAST(amount AS REAL) < 0",
            false,
        ),
    };

    let mut stmt = conn.prepare(sql)?;
    let descriptions: Vec<String> = if scoped {
        stmt.query_map(params![account_id.unwrap()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut seen: HashSet<String> = HashSet::new();
    for d in descriptions {
        if let Some(id) = extract_tax_id(pack, &d) {
            seen.insert(id);
        }
    }
    let mut out: Vec<String> = seen.into_iter().collect();
    out.sort();
    Ok(out)
}

/// Runs the whole enrichment, counting progress along the way.
///
/// Never holds the database lock across a network wait: each lock is per
/// operation, as in the rest of the code. That is what lets the interface keep
/// querying the database while this job runs.
pub fn run_enrichment(
    conn: &Mutex<rusqlite::Connection>,
    pack: &LocalePack,
    provider: &dyn TaxIdProvider,
    account_id: Option<i64>,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(EnrichEvent),
) -> AppResult<()> {
    let tax_ids = {
        let c = conn.lock().expect("db mutex poisoned");
        collect_unique_tax_ids(&c, pack, account_id)?
    };

    on_event(EnrichEvent::Started {
        total: tax_ids.len() as u32,
    });

    let mut created_rules: Vec<Rule> = Vec::new();
    let mut unresolved: Vec<TaxIdResolution> = Vec::new();
    let mut done: u32 = 0;
    let mut cancelled = false;
    let mut consulted = 0usize;

    for tax_id in &tax_ids {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        // The existing rule is the cache: if there is one, there is nothing to look up.
        let has_rule = {
            let c = conn.lock().expect("db mutex poisoned");
            c.query_row(
                "SELECT 1 FROM rule_patterns WHERE pattern = ?1 LIMIT 1",
                params![tax_id],
                |_| Ok(()),
            )
            .is_ok()
        };
        if has_rule {
            done += 1;
            continue;
        }

        // Courtesy only between real lookups. Counting skipped iterations here
        // would make the job sleep for work that never happened.
        if consulted > 0 {
            std::thread::sleep(Duration::from_millis(provider.courtesy_delay_ms()));
        }
        consulted += 1;

        let enrichment = {
            let c = conn.lock().expect("db mutex poisoned");
            lookup_with(&c, tax_id, pack, provider)
        };

        done += 1;

        let enrichment = match enrichment {
            Ok(e) => e,
            Err(_) => {
                // A failed lookup does not bring the job down — but it is not
                // swallowed either: it becomes a countable event.
                on_event(EnrichEvent::Failed {
                    done,
                    tax_id: tax_id.clone(),
                });
                continue;
            }
        };

        let resolution = TaxIdResolution {
            tax_id: tax_id.clone(),
            legal_name: enrichment.company.legal_name.clone(),
            trade_name: enrichment.company.trade_name.clone(),
            activity_code: enrichment.company.activity_code.clone(),
            activity_label: enrichment.company.activity_label.clone(),
            suggested_category_id: enrichment.suggested_category_id,
        };

        match resolution.suggested_category_id {
            Some(category_id) => {
                let rule = insert_rule(conn, tax_id, category_id, &resolution)?;
                let label = rule.display_name.clone().unwrap_or_else(|| tax_id.clone());
                created_rules.push(rule.clone());
                on_event(EnrichEvent::Resolved { done, label, rule });
            }
            None => {
                unresolved.push(resolution.clone());
                on_event(EnrichEvent::Unresolved { done, resolution });
            }
        }
    }

    // Applying the rules happens even on cancellation: what was already created
    // must count. Cancelling is stopping work, not undoing the work done.
    let txs_classified = {
        let mut c = conn.lock().expect("db mutex poisoned");
        apply_rules_internal(&mut c, account_id)?
    };

    let report = AutoClassifyReport {
        created_rules,
        txs_classified,
        unresolved,
    };

    on_event(if cancelled {
        EnrichEvent::Cancelled { report }
    } else {
        EnrichEvent::Finished { report }
    });

    Ok(())
}

/// Creates the rule for the resolved tax id. Starts with a single snippet: the id itself.
fn insert_rule(
    conn: &Mutex<rusqlite::Connection>,
    tax_id: &str,
    category_id: i64,
    resolution: &TaxIdResolution,
) -> AppResult<Rule> {
    let c = conn.lock().expect("db mutex poisoned");
    let display = resolution
        .legal_name
        .clone()
        .or_else(|| resolution.trade_name.clone());
    c.execute(
        "INSERT INTO rules (category_id, priority, due_day, display_name)
         VALUES (?1, 10, NULL, ?2)",
        params![category_id, display],
    )?;
    let id = c.last_insert_rowid();
    c.execute(
        "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, ?2)",
        params![id, tax_id],
    )?;
    Ok(c.query_row(
        "SELECT id, category_id, priority, due_day, display_name, created_at, pay_lead_months
         FROM rules WHERE id = ?1",
        params![id],
        |row| {
            Ok(Rule {
                id: row.get(0)?,
                patterns: vec![tax_id.to_string()],
                category_id: row.get(1)?,
                priority: row.get(2)?,
                due_day: row.get(3)?,
                display_name: row.get(4)?,
                created_at: row.get(5)?,
                pay_lead_months: row.get(6)?,
            })
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::locale::LocalePack;
    use rusqlite::{params, Connection};

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        crate::db::seed_from_pack(&conn, &crate::locale::LocalePack::embedded_pt_br()).unwrap();
        conn
    }

    fn pack() -> LocalePack {
        LocalePack::embedded_pt_br()
    }

    /// Creates an account and returns its id.
    fn seed_account(conn: &Connection) -> i64 {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO accounts (name, bank, ofx_acctid, kind)
             VALUES ('Conta', NULL, ?1, 'checking')",
            params![n.to_string()],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Uncategorized outflow — the only shape the job looks at.
    fn seed_tx(conn: &Connection, account_id: i64, description: &str) {
        conn.execute(
            "INSERT INTO transactions (account_id, date, amount, description, ofx_fitid, category_id)
             VALUES (?1, '2026-03-01', '-100.00', ?2, ?3, NULL)",
            params![account_id, description, format!("fit-{description}")],
        )
        .unwrap();
    }

    use crate::enrich::test_support::FakeProvider;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    /// Event collector: the tests assert over the whole sequence.
    fn collect_events(
        conn: Connection,
        provider: &FakeProvider,
        account_id: Option<i64>,
        cancel: &AtomicBool,
    ) -> (Vec<EnrichEvent>, Mutex<Connection>) {
        let guarded = Mutex::new(conn);
        let mut events = Vec::new();
        run_enrichment(&guarded, &pack(), provider, account_id, cancel, &mut |e| {
            events.push(e)
        })
        .unwrap();
        (events, guarded)
    }

    /// How many rules exist for this tax id.
    ///
    /// Counting the whole `rules` table does not work: migrations 0006 and 0012
    /// seed transfer and credit-card rules, so a freshly migrated database
    /// already starts with 15. What these tests assert is about THIS tax id.
    fn rules_for(db: &Mutex<Connection>, tax_id: &str) -> i64 {
        db.lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM rule_patterns WHERE pattern = ?1",
                params![tax_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn kinds(events: &[EnrichEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e {
                EnrichEvent::Started { .. } => "started",
                EnrichEvent::Resolved { .. } => "resolved",
                EnrichEvent::Unresolved { .. } => "unresolved",
                EnrichEvent::Failed { .. } => "failed",
                EnrichEvent::Finished { .. } => "finished",
                EnrichEvent::Cancelled { .. } => "cancelled",
                EnrichEvent::Aborted { .. } => "aborted",
            })
            .collect()
    }

    #[test]
    fn emits_started_with_the_real_total_then_finishes() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        let fake = FakeProvider::new(&[
            ("09095183000140", "4711301"), // mercado — mapeado
            ("33967103000184", "5611201"), // restaurante — mapeado
        ]);

        let (events, _db) = collect_events(conn, &fake, Some(acc), &AtomicBool::new(false));

        assert_eq!(
            kinds(&events),
            vec!["started", "resolved", "resolved", "finished"]
        );
        match &events[0] {
            EnrichEvent::Started { total } => assert_eq!(*total, 2),
            other => panic!("the first event should be Started, got {other:?}"),
        }
    }

    #[test]
    fn progress_counter_advances_monotonically_to_the_total() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        let fake = FakeProvider::new(&[
            ("09095183000140", "4711301"),
            ("33967103000184", "5611201"),
        ]);

        let (events, _db) = collect_events(conn, &fake, Some(acc), &AtomicBool::new(false));

        let dones: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                EnrichEvent::Resolved { done, .. } => Some(*done),
                _ => None,
            })
            .collect();
        assert_eq!(dones, vec![1, 2], "the counter never goes backwards nor skips");
    }

    #[test]
    fn unmapped_activity_becomes_unresolved_and_creates_no_rule() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - FAZENDA - 09.095.183/0001-40 - ITAU");
        let fake = FakeProvider::new(&[("09095183000140", "0111301")]); // fora do mapa

        let (events, db) = collect_events(conn, &fake, Some(acc), &AtomicBool::new(false));

        assert_eq!(kinds(&events), vec!["started", "unresolved", "finished"]);
        assert_eq!(
            rules_for(&db, "09.095.183/0001-40"),
            0,
            "with no suggested category, no rule is created"
        );
    }

    #[test]
    fn skips_tax_ids_that_already_have_a_rule() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        conn.execute(
            "INSERT INTO rules (category_id, priority, due_day, display_name)
             VALUES ((SELECT id FROM categories LIMIT 1), 10, NULL, 'already exists')",
            [],
        )
        .unwrap();
        let rule_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO rule_patterns (rule_id, pattern) VALUES (?1, '09.095.183/0001-40')",
            params![rule_id],
        )
        .unwrap();
        let fake = FakeProvider::new(&[("09095183000140", "4711301")]);

        let (events, _db) = collect_events(conn, &fake, Some(acc), &AtomicBool::new(false));

        assert_eq!(
            fake.call_count(),
            0,
            "the existing rule is the cache — no lookup"
        );
        assert_eq!(kinds(&events), vec!["started", "finished"]);
    }

    #[test]
    fn a_failed_lookup_is_counted_and_the_loop_carries_on() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        // Only the second has an answer; the first (alphabetical order) fails.
        let fake = FakeProvider::new(&[("33967103000184", "5611201")]);

        let (events, _db) = collect_events(conn, &fake, Some(acc), &AtomicBool::new(false));

        assert_eq!(
            kinds(&events),
            vec!["started", "failed", "resolved", "finished"],
            "the failure does not interrupt what comes next"
        );
        match &events[1] {
            EnrichEvent::Failed { done, tax_id } => {
                assert_eq!(*done, 1);
                assert_eq!(tax_id, "09.095.183/0001-40");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn cancelling_stops_early_and_creates_nothing() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        let fake = FakeProvider::new(&[
            ("09095183000140", "4711301"),
            ("33967103000184", "5611201"),
        ]);
        // Already cancelled before the first pass: no lookup should go out.
        let cancel = AtomicBool::new(true);

        let (events, db) = collect_events(conn, &fake, Some(acc), &cancel);

        assert_eq!(kinds(&events), vec!["started", "cancelled"]);
        assert_eq!(fake.call_count(), 0, "cancelled before looking anything up");
        assert_eq!(rules_for(&db, "09.095.183/0001-40"), 0);
        assert_eq!(rules_for(&db, "33.967.103/0001-84"), 0);
    }

    #[test]
    fn cancelled_report_carries_the_partial_result() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        let fake = FakeProvider::new(&[
            ("09095183000140", "4711301"),
            ("33967103000184", "5611201"),
        ]);
        let cancel = AtomicBool::new(false);

        // Cancels as soon as the first resolves: the second pass never happens.
        let guarded = Mutex::new(conn);
        let mut events = Vec::new();
        run_enrichment(&guarded, &pack(), &fake, Some(acc), &cancel, &mut |e| {
            if matches!(e, EnrichEvent::Resolved { .. }) {
                cancel.store(true, Ordering::Relaxed);
            }
            events.push(e);
        })
        .unwrap();

        assert_eq!(kinds(&events), vec!["started", "resolved", "cancelled"]);
        match events.last().unwrap() {
            EnrichEvent::Cancelled { report } => {
                assert_eq!(
                    report.created_rules.len(),
                    1,
                    "the rule created before the stop stays in the report"
                );
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        assert_eq!(
            rules_for(&guarded, "09.095.183/0001-40"),
            1,
            "the rule stays in the database after cancellation"
        );
        assert_eq!(
            rules_for(&guarded, "33.967.103/0001-84"),
            0,
            "the one never looked up does not exist"
        );
    }

    #[test]
    fn collects_unique_tax_ids_sorted_and_deduped() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - ENERGISA 2 - 09.095.183/0001-40 - ITAU");
        seed_tx(&conn, acc, "Pix - DEMERGE - 33.967.103/0001-84 - BB");
        seed_tx(&conn, acc, "Compra no débito - MISTER SUSHI");

        let ids = collect_unique_tax_ids(&conn, &pack(), Some(acc)).unwrap();

        assert_eq!(ids, vec!["09.095.183/0001-40", "33.967.103/0001-84"]);
    }

    #[test]
    fn collects_nothing_when_no_description_has_a_tax_id() {
        let conn = fresh_conn();
        let acc = seed_account(&conn);
        seed_tx(&conn, acc, "Compra no débito - MISTER SUSHI");

        assert!(collect_unique_tax_ids(&conn, &pack(), Some(acc))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ignores_transactions_from_another_account() {
        let conn = fresh_conn();
        let a = seed_account(&conn);
        let b = seed_account(&conn);
        seed_tx(&conn, b, "Pix - ENERGISA - 09.095.183/0001-40 - ITAU");

        assert!(collect_unique_tax_ids(&conn, &pack(), Some(a))
            .unwrap()
            .is_empty());
        assert_eq!(
            collect_unique_tax_ids(&conn, &pack(), None).unwrap().len(),
            1
        );
    }
}
