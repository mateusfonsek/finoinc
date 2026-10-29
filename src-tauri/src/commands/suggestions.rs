use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use tauri::State;

use crate::commands::tax_id::TaxIdResolution;
use crate::enrich::extract_tax_id;
use crate::db::Db;
use crate::domain::rule::Rule;
use crate::error::AppResult;
use crate::locale::{LocalePack, LocaleState};

/// Reduces a free-form description to (grouping key, readable label, suggested
/// pattern) using the active locale's `rules.normalization`.
///
/// The key must be stable across repeats of the same counterparty. The pattern
/// becomes a rule snippet (LIKE substring).
///
/// Public so `income_sources` can reuse it — same counterparty grouping, but
/// for inflows instead of rules.
pub fn normalize(description: &str, pack: &LocalePack) -> (String, String, String) {
    let norm = &pack.rules.normalization;
    let sep = if norm.field_separator.is_empty() {
        " - "
    } else {
        norm.field_separator.as_str()
    };

    // 1. A tax id wins: the label is the text after the first separator.
    if let Some(tax_id) = extract_tax_id(pack, description) {
        let label = description
            .split_once(sep)
            .map(|(_, after)| after.trim())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| description.to_string());
        let kp = if norm.cnpj_key_prefix.is_empty() {
            "cnpj"
        } else {
            norm.cnpj_key_prefix.as_str()
        };
        return (format!("{kp}:{tax_id}"), label, tax_id);
    }

    // 2. Ordered rules from the pack (strip / masked / system).
    for rule in &norm.rules {
        match rule.kind.as_str() {
            "strip" => {
                if let Some(rest) = description.strip_prefix(&rule.prefix) {
                    let v = rest.trim();
                    return (
                        format!("{}:{v}", rule.key_prefix),
                        rule.label.replace("{v}", v),
                        v.to_string(),
                    );
                }
            }
            "masked" if description.starts_with(&rule.prefix) => {
                if let Some(re) = pack.cpf_mask_re.as_ref() {
                    if let Some(mask) = re.find(description) {
                        let mask_s = mask.as_str();
                        let name =
                            pix_name(description, sep).unwrap_or_else(|| mask_s.to_string());
                        return (
                            format!("{}:{mask_s}", rule.key_prefix),
                            rule.label.replace("{name}", &name),
                            mask_s.to_string(),
                        );
                    }
                }
            }
            "system" if description.starts_with(&rule.prefix) => {
                return (rule.key.clone(), rule.label.clone(), rule.prefix.clone());
            }
            _ => {}
        }
    }

    // 3. Fallback: the whole description (each tx becomes its own group).
    (
        format!("raw:{description}"),
        description.to_string(),
        description.to_string(),
    )
}

/// Extracts the counterparty name from Nubank Pix descriptions.
/// Format: `Transferência (enviada|recebida) pelo Pix - NAME - CPF_MASK - BANK ...`
fn pix_name(description: &str, sep: &str) -> Option<String> {
    let (_, rest) = description.split_once(sep)?;
    let name = rest.split_once(sep).map_or(rest, |(name, _)| name);
    Some(name.trim().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RuleSuggestion {
    pub key: String,
    pub label: String,
    pub suggested_pattern: String,
    pub count: u32,
    pub total: String,
    pub sample_description: String,
    pub transaction_ids: Vec<i64>,
}

/// Returns groups of uncategorized OUTFLOW transactions whose normalized key
/// repeats at least `min_count` times. Sorted by absolute total descending.
///
/// Only considers `amount < 0` (outflows). Inflows need no category — income is
/// tracked by counterparty in the Dashboard's income sources panel.
#[tauri::command]
#[specta::specta]
pub fn suggest_rules(
    db: State<'_, Db>,
    locale: State<'_, LocaleState>,
    min_count: u32,
) -> AppResult<Vec<RuleSuggestion>> {
    let conn = db.conn.lock().expect("db mutex poisoned");
    let pack = locale.pack.lock().expect("locale mutex poisoned");
    let mut stmt = conn.prepare(
        "SELECT id, description, amount FROM transactions
         WHERE category_id IS NULL
           AND CAST(amount AS REAL) < 0
         ORDER BY id ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    struct Bucket {
        label: String,
        pattern: String,
        ids: Vec<i64>,
        total: rust_decimal::Decimal,
        sample: String,
    }
    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    for (id, desc, amt) in rows {
        let (key, label, pattern) = normalize(&desc, &pack);
        let d: rust_decimal::Decimal = amt.parse().unwrap_or(rust_decimal::Decimal::ZERO);
        let entry = buckets.entry(key).or_insert_with(|| Bucket {
            label: label.clone(),
            pattern: pattern.clone(),
            ids: vec![],
            total: rust_decimal::Decimal::ZERO,
            sample: desc.clone(),
        });
        entry.ids.push(id);
        entry.total += d;
    }
    let min = min_count.max(1) as usize;
    let mut out: Vec<RuleSuggestion> = buckets
        .into_iter()
        .filter(|(_, b)| b.ids.len() >= min)
        .map(|(key, b)| RuleSuggestion {
            key,
            label: b.label,
            suggested_pattern: b.pattern,
            count: b.ids.len() as u32,
            total: b.total.to_string(),
            sample_description: b.sample,
            transaction_ids: b.ids,
        })
        .collect();
    out.sort_by(|a, b| {
        let ad: rust_decimal::Decimal = a.total.parse().unwrap_or(rust_decimal::Decimal::ZERO);
        let bd: rust_decimal::Decimal = b.total.parse().unwrap_or(rust_decimal::Decimal::ZERO);
        bd.abs().cmp(&ad.abs())
    });
    Ok(out)
}

/// Suggested pattern for a single description — same logic as the Suggestions
/// tab, exposed so a rule can be created from a transaction's detail panel.
#[tauri::command]
#[specta::specta]
pub fn suggest_pattern_for(locale: State<'_, LocaleState>, description: String) -> String {
    let pack = locale.pack.lock().expect("locale mutex poisoned");
    normalize(&description, &pack).2
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AutoClassifyReport {
    pub created_rules: Vec<Rule>,
    pub txs_classified: u32,
    pub unresolved: Vec<TaxIdResolution>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LocalePack;

    fn pack() -> LocalePack {
        LocalePack::embedded_pt_br()
    }

    #[test]
    fn normalize_debito_groups_by_merchant() {
        let p = pack();
        let (k1, _, p1) = normalize("Compra no débito - MISTER SUSHI", &p);
        let (k2, _, _p2) = normalize("Compra no débito - MISTER SUSHI", &p);
        assert_eq!(k1, k2);
        assert_eq!(p1, "MISTER SUSHI");
    }

    #[test]
    fn normalize_pix_out_groups_by_cpf_mask() {
        let p = pack();
        let a = "Transferência enviada pelo Pix - Ana Souza (CAIXA) - •••.123.456-•• - CAIXA";
        let b = "Transferência enviada pelo Pix - Ana Souza (Caixa) - •••.123.456-•• - CAIXA outro";
        let (k1, _, p1) = normalize(a, &p);
        let (k2, _, _) = normalize(b, &p);
        assert_eq!(k1, k2, "same CPF mask = same key, even with name variation");
        assert_eq!(p1, "•••.123.456-••");
    }

    #[test]
    fn normalize_cnpj_wins_over_pix_prefix() {
        let d = "Transferência enviada pelo Pix - ENERGISA - 09.095.183/0001-40 - ITAU";
        let (k, _, p) = normalize(d, &pack());
        assert!(k.starts_with("cnpj:"), "CNPJ should take precedence");
        assert_eq!(p, "09.095.183/0001-40");
    }

    #[test]
    fn normalize_aplicacao_rdb_is_system_key() {
        let (k, _, p) = normalize("Aplicação RDB", &pack());
        assert_eq!(k, "system:rdb_apl");
        assert_eq!(p, "Aplicação RDB");
    }

    #[test]
    fn normalize_fallback_uses_full_description() {
        let (k, _, _) = normalize("Algo bem estranho aqui", &pack());
        assert!(k.starts_with("raw:"));
    }
}
