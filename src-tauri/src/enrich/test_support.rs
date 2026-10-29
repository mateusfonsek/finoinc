//! Test support for enrichment. Compiled under `cfg(test)` only.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::enrich::provider::{Company, TaxIdProvider};
use crate::error::{AppError, AppResult};

/// Deterministic provider: answers from an in-memory map and records every
/// call, so tests can assert *how many* lookups happened — which is half of
/// what cancellation and the skip-on-existing-rule path need to prove.
pub struct FakeProvider {
    /// Tax-id digits → activity code returned.
    pub by_digits: HashMap<String, String>,
    pub calls: Mutex<Vec<String>>,
}

impl FakeProvider {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            by_digits: pairs
                .iter()
                .map(|(d, c)| ((*d).to_string(), (*c).to_string()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("calls mutex poisoned").len()
    }
}

impl TaxIdProvider for FakeProvider {
    fn lookup(&self, tax_id_digits: &str) -> AppResult<Company> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(tax_id_digits.to_string());
        match self.by_digits.get(tax_id_digits) {
            Some(code) => Ok(Company {
                legal_name: Some(format!("EMPRESA {tax_id_digits}")),
                trade_name: None,
                activity_code: Some(code.clone()),
                activity_label: None,
            }),
            // Absent = "can't know right now", which is the trait's contract.
            None => Err(AppError::Invalid(format!(
                "fake: no answer for {tax_id_digits}"
            ))),
        }
    }

    /// Zero: no test should pay the courtesy toll.
    fn courtesy_delay_ms(&self) -> u64 {
        0
    }
}
