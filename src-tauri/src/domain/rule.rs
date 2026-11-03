use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Rule {
    pub id: i64,
    /// Snippets searched in the description, OR'd: the rule matches when the
    /// description contains ANY of them. Never empty.
    pub patterns: Vec<String>,
    pub category_id: i64,
    pub priority: i32,
    /// Day of month (1-31) the bill is due. NULL means no due date — the rule
    /// only shows on the calendar when it matches a transaction.
    pub due_day: Option<i32>,
    /// Which month this bill's payment lands in, relative to the due date.
    /// `0` = the month it falls due; `1` = the month before. Anything the user
    /// has not set stays `0`, which is how the calendar always behaved.
    pub pay_lead_months: i32,
    /// Friendly label (e.g. legal name from a CNPJ lookup). NULL means none,
    /// and the UI falls back to the first pattern.
    pub display_name: Option<String>,
    pub created_at: String,
}

/// A rule plus how many transactions it reaches — for the Rules screen, where
/// the question is "is this rule catching anything?".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RuleWithCount {
    pub id: i64,
    pub patterns: Vec<String>,
    pub category_id: i64,
    pub priority: i32,
    pub due_day: Option<i32>,
    /// Which month this bill's payment lands in, relative to the due date.
    /// `0` = the month it falls due; `1` = the month before. Anything the user
    /// has not set stays `0`, which is how the calendar always behaved.
    pub pay_lead_months: i32,
    pub display_name: Option<String>,
    pub created_at: String,
    /// Transactions whose description matches ANY of the rule's snippets,
    /// regardless of their current category. This is reach, not authorship: a
    /// manually categorized transaction still counts, and so does one a
    /// higher-priority rule took.
    pub transaction_count: u32,
}

/// The rules a transaction's detail panel shows: the one that set its
/// category, if any, and the others whose snippet matches its description —
/// what the user must see before creating yet another rule for it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TransactionRules {
    pub applied: Option<RuleWithCount>,
    /// Highest priority first; never includes `applied`.
    pub matching: Vec<RuleWithCount>,
}

/// A change applying the rules would cause. Material for the review screen —
/// nothing is written until the user chooses.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RulePreviewRow {
    pub transaction_id: i64,
    pub date: String,
    /// Decimal as string, like everywhere else. Never f64.
    pub amount: String,
    pub description: String,
    /// `None` means uncategorized. Anything else means applying the rule
    /// REPLACES the current category.
    ///
    /// Note: the DB does not record who set the category, so this cannot tell
    /// "the user picked it" from "an earlier rule set it". The UI must not
    /// claim authorship — only that a category exists and would change.
    pub current_category_id: Option<i64>,
    pub new_category_id: i64,
    pub rule_id: i64,
    /// Label of the winning rule: `display_name` when set, otherwise the
    /// snippet that actually matched this description.
    pub rule_label: String,
}

/// The transactions a rule reaches, plus their total.
///
/// "Reach" is the same criterion as the count in [`RuleWithCount`], and the two
/// MUST agree — the number in the table is what promises this list's contents.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RuleMatches {
    pub transactions: Vec<crate::domain::transaction::Transaction>,
    /// Sum of the amounts, added with `rust_decimal` — never f64, which would
    /// accumulate error over a long list.
    pub total: String,
}

/// A user's choice on the review screen: this transaction goes to this
/// category.
///
/// The category is explicit rather than recomputed at apply time — if rules
/// change between review and click, what was reviewed is what gets written.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RuleChoice {
    pub transaction_id: i64,
    pub category_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NewRule {
    pub patterns: Vec<String>,
    pub category_id: i64,
    pub priority: i32,
    pub due_day: Option<i32>,
    /// Which month this bill's payment lands in, relative to the due date.
    /// `0` = the month it falls due; `1` = the month before. Anything the user
    /// has not set stays `0`, which is how the calendar always behaved.
    #[serde(default)]
    pub pay_lead_months: i32,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UpdateRule {
    pub patterns: Vec<String>,
    pub category_id: i64,
    pub priority: i32,
    pub due_day: Option<i32>,
    /// Which month this bill's payment lands in, relative to the due date.
    /// `0` = the month it falls due; `1` = the month before. Anything the user
    /// has not set stays `0`, which is how the calendar always behaved.
    #[serde(default)]
    pub pay_lead_months: i32,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// A calendar event: a rule plus an optional due day plus an optional matching
/// transaction.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CalendarEvent {
    pub rule_id: i64,
    /// The snippet that actually matched — or the rule's first one, when the
    /// event exists only because of `due_day` and nothing matched yet.
    pub pattern: String,
    pub category_name: String,
    pub category_color_token: Option<String>,
    pub due_day: Option<i32>,
    /// Full payment date (`YYYY-MM-DD`), not a day of the month: with
    /// `pay_lead_months` the payment lives in another month, and 1..31 does not
    /// say which.
    pub paid_date: Option<String>,
    pub paid_amount: Option<String>,
    pub paid_transaction_id: Option<i64>,
    /// Came from `bill_settlements` rather than from the derivation, so the UI
    /// can offer to undo it.
    pub manually_settled: bool,
}
