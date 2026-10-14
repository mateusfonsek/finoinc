//! Locale packs — everything country/language-specific lives in `locales/<code>/`
//! as JSON, so a new language is "copy a folder + translate JSON", no code edits.
//!
//! The active pack is loaded at startup from the bundled resources dir
//! (`resource_dir()/locales/<code>`) and kept in [`LocaleState`]. If resources
//! can't be read (e.g. `cargo test`, or a broken bundle), we fall back to the
//! `pt-BR` pack embedded at compile time so the app always has a valid locale.

#[cfg(test)]
mod contract;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};

fn invalid<E: std::fmt::Display>(e: E) -> AppError {
    AppError::Invalid(e.to_string())
}

// ---------------------------------------------------------------------------
// Deserialized shapes (mirror the JSON files in locales/<code>/)
// ---------------------------------------------------------------------------

// Several manifest fields (currency, dateLocale, code…) are consumed by the
// frontend and by tests, not by core Rust — silence dead-code noise here.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Currency {
    pub code: String,
    pub locale: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct TaxId {
    pub name: String,
    #[serde(default)]
    pub regex: String,
    #[serde(default)]
    pub provider: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub flag: String,
    pub currency: Currency,
    #[serde(rename = "dateLocale")]
    pub date_locale: String,
    /// Weekday the calendar grid starts on, as a `Date.getDay()` value
    /// (0 = Sunday). Omitted means Sunday, so a pack written before this field
    /// existed keeps rendering as it did.
    #[serde(rename = "firstDayOfWeek", default)]
    pub first_day_of_week: u8,
    #[serde(rename = "taxId")]
    pub tax_id: TaxId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryDef {
    pub key: String,
    pub name: String,
    pub color_token: String,
    pub kind: String,
    #[serde(default)]
    pub is_investment: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CategoriesFile {
    categories: Vec<CategoryDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CnaeEntry {
    pub prefix: String,
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedRule {
    pub pattern: String,
    pub category: String,
    pub priority: i64,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// One description-normalization rule. `kind` selects the matching strategy:
/// - `strip`: strip `prefix`, use the remainder as pattern; key = `{key_prefix}:{v}`.
/// - `masked`: match `prefix`, then find the tax/CPF mask; key = `{key_prefix}:{mask}`.
/// - `system`: match `prefix`; fixed `key`, pattern = `prefix`.
#[derive(Debug, Clone, Deserialize)]
pub struct NormRule {
    #[serde(rename = "type")]
    pub kind: String,
    pub prefix: String,
    #[serde(default)]
    pub key_prefix: String,
    #[serde(default)]
    pub key: String,
    pub label: String,
    /// Read only by the frontend, which loads its own copy of `rules.json` via
    /// `import.meta.glob` — Rust deserializes these for the contract test.
    #[allow(dead_code)]
    #[serde(default)]
    pub badge: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub tone: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Normalization {
    #[serde(default)]
    pub field_separator: String,
    #[serde(default)]
    pub cpf_mask_regex: String,
    #[serde(default)]
    pub cnpj_key_prefix: String,
    #[serde(default)]
    pub rules: Vec<NormRule>,
}

/// One reversal-detection phase. `strategy` picks the pairing algorithm (which
/// stays in the frontend); every other field is how this country's banks phrase
/// the pair.
///
/// `counterpart_prefix` is only read by `counterparty_signature`.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ReversalPhase {
    pub strategy: String,
    pub prefix: String,
    #[serde(default)]
    pub counterpart_prefix: String,
    pub window_days: u32,
    pub role: String,
    pub counterpart_role: String,
}

/// Empty `phases` disables reversal detection for the locale, the same way an
/// empty `taxId.regex` disables company lookup.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Reversals {
    #[serde(default)]
    pub phases: Vec<ReversalPhase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulesDef {
    #[serde(default)]
    pub cnae_map: Vec<CnaeEntry>,
    #[serde(default)]
    pub seed_rules: Vec<SeedRule>,
    pub normalization: Normalization,
    /// Read only by the contract test (`embedded_pt_br_declares_reversal_phases`)
    /// and by the frontend's own `import.meta.glob` copy of `rules.json`.
    #[allow(dead_code)]
    #[serde(default)]
    pub reversals: Reversals,
}

// ---------------------------------------------------------------------------
// LocalePack — a fully-loaded, ready-to-use locale (with compiled regexes)
// ---------------------------------------------------------------------------

/// `Clone` exists for the background work: the enrichment thread takes its own
/// copy instead of holding the locale `Mutex` for the job's ~30s, which would
/// block any other command that reads the pack. Every field was already
/// clonable; the cost is one copy per job.
#[derive(Clone)]
pub struct LocalePack {
    pub manifest: Manifest,
    pub categories: Vec<CategoryDef>,
    pub rules: RulesDef,
    /// Kept generic: only the native menu reads it on the Rust side; the
    /// frontend loads its own copy via `import.meta.glob`.
    pub strings: serde_json::Value,
    /// Compiled `manifest.taxId.regex` (None when the locale has no tax id).
    pub tax_id_re: Option<Regex>,
    /// Compiled `rules.normalization.cpf_mask_regex` (None when unset).
    pub cpf_mask_re: Option<Regex>,
}

impl LocalePack {
    fn from_parts(
        manifest_s: &str,
        categories_s: &str,
        rules_s: &str,
        strings_s: &str,
    ) -> AppResult<LocalePack> {
        let manifest: Manifest = serde_json::from_str(manifest_s).map_err(invalid)?;
        let categories: CategoriesFile = serde_json::from_str(categories_s).map_err(invalid)?;
        let rules: RulesDef = serde_json::from_str(rules_s).map_err(invalid)?;
        let strings: serde_json::Value = serde_json::from_str(strings_s).map_err(invalid)?;

        let tax_id_re = if manifest.tax_id.regex.trim().is_empty() {
            None
        } else {
            Some(Regex::new(&manifest.tax_id.regex).map_err(invalid)?)
        };
        let cpf_mask_re = if rules.normalization.cpf_mask_regex.trim().is_empty() {
            None
        } else {
            Some(Regex::new(&rules.normalization.cpf_mask_regex).map_err(invalid)?)
        };

        Ok(LocalePack {
            manifest,
            categories: categories.categories,
            rules,
            strings,
            tax_id_re,
            cpf_mask_re,
        })
    }

    /// Load a pack from `locales/<code>/` on disk.
    pub fn load_from_dir(dir: &Path) -> AppResult<LocalePack> {
        let read = |f: &str| -> AppResult<String> { Ok(std::fs::read_to_string(dir.join(f))?) };
        Self::from_parts(
            &read("manifest.json")?,
            &read("categories.json")?,
            &read("rules.json")?,
            &read("strings.json")?,
        )
    }

    /// The `pt-BR` pack embedded at compile time — guaranteed-valid fallback,
    /// and the pack used by unit tests (which have no resource dir).
    pub fn embedded_pt_br() -> LocalePack {
        Self::from_parts(
            include_str!("../../../locales/pt-BR/manifest.json"),
            include_str!("../../../locales/pt-BR/categories.json"),
            include_str!("../../../locales/pt-BR/rules.json"),
            include_str!("../../../locales/pt-BR/strings.json"),
        )
        .expect("embedded pt-BR pack must parse")
    }

    /// A menu label from `strings.menu.<key>`, falling back to `fallback`.
    pub fn menu_str(&self, key: &str, fallback: &str) -> String {
        self.strings
            .get("menu")
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or(fallback)
            .to_string()
    }
}

// ---------------------------------------------------------------------------
// Discovery + persistence of the active locale
// ---------------------------------------------------------------------------

const DEFAULT_LOCALE: &str = "pt-BR";
const ACTIVE_FILE: &str = "active_locale";

fn read_active_locale(data_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(data_dir.join(ACTIVE_FILE)).ok()?;
    let code = raw.trim().to_string();
    if code.is_empty() {
        None
    } else {
        Some(code)
    }
}

fn write_active_locale(data_dir: &Path, code: &str) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(ACTIVE_FILE), code)?;
    Ok(())
}

/// Load the pack for `code` from `root/<code>`, falling back to the embedded
/// pt-BR pack when the folder is missing or unreadable.
fn load_pack(root: Option<&Path>, code: &str) -> LocalePack {
    if let Some(root) = root {
        let dir = root.join(code);
        match LocalePack::load_from_dir(&dir) {
            Ok(p) => return p,
            // The path in the text is not ornament: without it this warning said
            // only "No such file or directory" and passed for a dev quirk for a
            // long time, while in truth the on-disk pack never loaded because the
            // files were being bundled one level to the side.
            Err(e) => eprintln!(
                "[finoinc] locale '{code}' failed to load from {}: {e}; using embedded pt-BR",
                dir.display()
            ),
        }
    }
    LocalePack::embedded_pt_br()
}

fn resource_locales_root(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|r| r.join("locales"))
}

/// Enumerate available locale codes by listing subfolders that contain a
/// `manifest.json`. Always includes the default when discovery finds nothing.
fn discover_codes(root: Option<&Path>) -> Vec<String> {
    let mut codes: Vec<String> = Vec::new();
    if let Some(root) = root {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                if entry.path().join("manifest.json").is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        codes.push(name.to_string());
                    }
                }
            }
        }
    }
    if codes.is_empty() {
        codes.push(DEFAULT_LOCALE.to_string());
    }
    codes.sort();
    codes
}

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

pub struct LocaleState {
    locales_root: Option<PathBuf>,
    data_dir: PathBuf,
    pub active: Mutex<String>,
    pub pack: Mutex<LocalePack>,
}

impl LocaleState {
    pub fn init(app: &AppHandle) -> LocaleState {
        let locales_root = resource_locales_root(app);
        let data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        let active = read_active_locale(&data_dir).unwrap_or_else(|| DEFAULT_LOCALE.to_string());
        let pack = load_pack(locales_root.as_deref(), &active);
        LocaleState {
            locales_root,
            data_dir,
            active: Mutex::new(active),
            pack: Mutex::new(pack),
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct LocaleInfo {
    pub code: String,
    pub name: String,
    pub flag: String,
}

#[tauri::command]
#[specta::specta]
pub fn list_locales(state: State<'_, LocaleState>) -> Vec<LocaleInfo> {
    let codes = discover_codes(state.locales_root.as_deref());
    codes
        .into_iter()
        .map(|code| {
            // Load each manifest for its display name/flag; fall back to the code.
            let (name, flag) = state
                .locales_root
                .as_deref()
                .and_then(|root| LocalePack::load_from_dir(&root.join(&code)).ok())
                .map(|p| (p.manifest.name, p.manifest.flag))
                .unwrap_or_else(|| (code.clone(), String::new()));
            LocaleInfo { code, name, flag }
        })
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn get_active_locale(state: State<'_, LocaleState>) -> String {
    state.active.lock().expect("locale mutex poisoned").clone()
}

/// Swaps the active pack and, when the database is still pristine (nothing
/// imported yet), reseeds it from the new pack — otherwise a language switch
/// after `pt-BR` seeded the DB would leave the old categories and rules in
/// place forever. Locks are taken `db` then `locale`, same order as every
/// other command that holds both (see `commands::enrichment::enrichment_status`),
/// to avoid a deadlock.
#[tauri::command]
#[specta::specta]
pub fn set_active_locale(
    db: State<'_, crate::db::Db>,
    state: State<'_, LocaleState>,
    code: String,
) -> AppResult<()> {
    let pack = load_pack(state.locales_root.as_deref(), &code);

    {
        let mut conn = db.conn.lock().expect("db mutex poisoned");
        if crate::db::is_pristine(&conn)? {
            crate::db::reseed_from_pack(&mut conn, &pack)?;
        }
    }

    write_active_locale(&state.data_dir, &code)?;
    *state.active.lock().expect("locale mutex poisoned") = code;
    *state.pack.lock().expect("locale mutex poisoned") = pack;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locales must be bundled EXACTLY where [`resource_locales_root`] looks:
    /// `<resources>/locales`.
    ///
    /// This test exists because of a bug that went unnoticed for so long that the
    /// fallback hid it perfectly. The config was a LIST with a path escaping the
    /// crate (`["../locales/**/*"]`), and Tauri maps every `..` of the source
    /// path to a literal directory named `_up_` when computing the destination
    /// (`tauri_utils::resources::resource_relpath`). The files landed in
    /// `<resources>/_up_/locales/`, which nobody reads — so the on-disk pack
    /// NEVER loaded, in any build, and the app always fell back to the embedded
    /// one.
    ///
    /// The MAP form sets the destination explicitly, without deriving it from
    /// the source. And it must name the DIRECTORY, not a glob: with `*` in the
    /// pattern Tauri uses `dest.join(file_name)` and flattens everything — the
    /// four `.json` files of `pt-BR/` would collide at the root and a second
    /// locale would overwrite the first.
    #[test]
    fn locales_are_bundled_where_the_app_looks_for_them() {
        let cfg: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let resources = &cfg["bundle"]["resources"];

        assert!(
            resources.is_object(),
            "resources must be a source→destination map, not a list: the list \
             derives the destination from the source and `..` becomes `_up_`. \
             Got: {resources}"
        );
        assert_eq!(
            resources["../locales"], "locales",
            "the locales directory must land in <resources>/locales"
        );
        for (src, _) in resources.as_object().unwrap() {
            assert!(
                !src.contains('*'),
                "a glob in resources flattens the directory structure: {src}"
            );
        }
    }

    #[test]
    fn embedded_pt_br_parses() {
        let pack = LocalePack::embedded_pt_br();
        assert_eq!(pack.manifest.code, "pt-BR");
        assert_eq!(pack.manifest.currency.code, "BRL");
        assert!(pack.tax_id_re.is_some());
        assert!(pack.cpf_mask_re.is_some());
        assert_eq!(pack.categories.len(), 13);
        assert!(!pack.rules.cnae_map.is_empty());
        assert!(!pack.rules.seed_rules.is_empty());
    }

    #[test]
    fn menu_str_reads_strings() {
        let pack = LocalePack::embedded_pt_br();
        assert_eq!(pack.menu_str("edit", "X"), "Editar");
        assert_eq!(pack.menu_str("nonexistent", "fallback"), "fallback");
    }

    #[test]
    fn category_keys_are_stable() {
        let pack = LocalePack::embedded_pt_br();
        let keys: Vec<&str> = pack.categories.iter().map(|c| c.key.as_str()).collect();
        for expected in ["market", "restaurant", "transport", "transfer", "investment"] {
            assert!(keys.contains(&expected), "missing key {expected}");
        }
    }

    #[test]
    fn embedded_pt_br_declares_reversal_phases() {
        let pack = LocalePack::embedded_pt_br();
        let strategies: Vec<&str> = pack
            .rules
            .reversals
            .phases
            .iter()
            .map(|p| p.strategy.as_str())
            .collect();
        assert_eq!(
            strategies,
            vec!["exact_remainder", "counterparty_signature", "quoted_merchant"]
        );

        let pix = &pack.rules.reversals.phases[1];
        assert_eq!(pix.counterpart_prefix, "Transferência enviada pelo Pix - ");
        assert_eq!(pix.role, "refund");
        assert_eq!(pix.counterpart_role, "refunded");
        assert_eq!(pix.window_days, 30);
    }
}
