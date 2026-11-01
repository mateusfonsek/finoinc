import type { ParsedTransaction } from "$lib/statement/types";

/**
 * A transaction's role within a pair. Four values because the UI labels
 * chargebacks and refunds differently, but all mean "part of a pair summing to
 * zero".
 */
export type ReversalRole = "reversal" | "reversed" | "refund" | "refunded";

export interface ReversalInfo {
  role: ReversalRole;
  /** fitid of the other end of the pair. */
  pairFitid: string;
}

/**
 * One detection phase, declared by the locale pack. `strategy` picks the
 * pairing algorithm below; the rest is how this country's banks phrase the
 * pair. An empty phase list disables detection.
 */
export interface ReversalPhase {
  strategy: "exact_remainder" | "counterparty_signature" | "quoted_merchant";
  prefix: string;
  /** Only read by `counterparty_signature`: the outgoing leg's prefix. */
  counterpart_prefix?: string;
  window_days: number;
  role: ReversalRole;
  counterpart_role: ReversalRole;
}

/** Amounts equal in magnitude within a cent. */
const CENT = 0.01;

/**
 * Compiles a pack-declared prefix into a regex. The prefix is escaped as a
 * literal, and each of its spaces becomes `\s+` — banks vary the spacing, and
 * demanding a regex from whoever writes a locale pack would defeat the point.
 */
function escapePrefix(prefix: string): string {
  return prefix
    .trim()
    .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    .replace(/\s+/g, "\\s+");
}

function prefixRe(prefix: string): RegExp {
  return new RegExp("^" + escapePrefix(prefix) + "\\s*", "i");
}

/** `<prefix> "Merchant" (Merchant)` — the quoted merchant is what pairs. */
function quotedMerchantRe(prefix: string): RegExp {
  return new RegExp(
    "^" + escapePrefix(prefix) + '\\s+"([^"]+)"\\s*(?:\\([^)]+\\))?\\s*$',
    "i",
  );
}

/** Distance in days between two ISO YYYY-MM-DD dates. */
function daysBetween(a: string, b: string): number {
  const da = Date.parse(a + "T00:00:00Z");
  const db = Date.parse(b + "T00:00:00Z");
  if (Number.isNaN(da) || Number.isNaN(db)) return Infinity;
  return Math.abs((da - db) / 86_400_000);
}

/**
 * From `<prefix> - NAME - ID - BANK/BRANCH/ACCOUNT`, takes everything after the
 * name. That identifies the counterparty even when the NAME varies (legal vs
 * trade name, as Amazon refunds do).
 */
function counterpartySig(description: string, res: RegExp[]): string | null {
  for (const re of res) {
    const m = description.match(re);
    if (!m) continue;
    const body = description.slice(m[0].length);
    const firstDash = body.indexOf(" - ");
    if (firstDash === -1) return null;
    return body.slice(firstDash + 3).trim();
  }
  return null;
}

type PhaseRun = (
  txs: ParsedTransaction[],
  phase: ReversalPhase,
  used: Set<string>,
  result: Map<string, ReversalInfo>,
) => void;

/**
 * Records the pair when a candidate matches: opposite sign, same magnitude,
 * inside the phase's window, and not already spoken for. FIFO — `find` takes
 * the first free candidate.
 */
function pair(
  txs: ParsedTransaction[],
  trigger: ParsedTransaction,
  phase: ReversalPhase,
  used: Set<string>,
  result: Map<string, ReversalInfo>,
  matches: (candidate: ParsedTransaction) => boolean,
): void {
  const triggerFitid = trigger.fitid;
  if (!triggerFitid) return;
  const amount = Number(trigger.amount);
  if (!Number.isFinite(amount)) return;

  const candidate = txs.find((t) => {
    if (!t.fitid || used.has(t.fitid) || t.fitid === triggerFitid) return false;
    if (!matches(t)) return false;
    const other = Number(t.amount);
    if (!Number.isFinite(other)) return false;
    if (Math.abs(other + amount) > CENT) return false;
    return daysBetween(t.date, trigger.date) <= phase.window_days;
  });

  if (!candidate?.fitid) return;
  result.set(triggerFitid, { role: phase.role, pairFitid: candidate.fitid });
  result.set(candidate.fitid, { role: phase.counterpart_role, pairFitid: triggerFitid });
  used.add(triggerFitid);
  used.add(candidate.fitid);
}

/** Checking-account chargeback: the original's description is what is left of
 *  the trigger after the prefix. */
const exactRemainder: PhaseRun = (txs, phase, used, result) => {
  const re = prefixRe(phase.prefix);
  for (const trigger of txs) {
    if (!trigger.fitid || used.has(trigger.fitid)) continue;
    const m = trigger.description.match(re);
    if (!m) continue;
    const core = trigger.description.slice(m[0].length).trim();
    pair(txs, trigger, phase, used, result, (t) => t.description.trim() === core);
  }
};

/** Refund of an outgoing transfer, paired by counterparty rather than by name. */
const counterpartySignature: PhaseRun = (txs, phase, used, result) => {
  const re = prefixRe(phase.prefix);
  const counterRe = prefixRe(phase.counterpart_prefix ?? "");
  const both = [re, counterRe];
  for (const trigger of txs) {
    if (!trigger.fitid || used.has(trigger.fitid)) continue;
    if (!re.test(trigger.description)) continue;
    const sig = counterpartySig(trigger.description, both);
    if (!sig) continue;
    pair(txs, trigger, phase, used, result, (t) => {
      if (!counterRe.test(t.description)) return false;
      return counterpartySig(t.description, both) === sig;
    });
  }
};

/** Card chargeback: the original purchase's whole description is the merchant
 *  quoted in the trigger. */
const quotedMerchant: PhaseRun = (txs, phase, used, result) => {
  const re = quotedMerchantRe(phase.prefix);
  for (const trigger of txs) {
    if (!trigger.fitid || used.has(trigger.fitid)) continue;
    const m = trigger.description.match(re);
    if (!m) continue;
    const merchant = m[1].trim().toLowerCase();
    pair(txs, trigger, phase, used, result, (t) =>
      t.description.trim().toLowerCase() === merchant,
    );
  }
};

const STRATEGIES: Record<ReversalPhase["strategy"], PhaseRun> = {
  exact_remainder: exactRemainder,
  counterparty_signature: counterpartySignature,
  quoted_merchant: quotedMerchant,
};

/**
 * Finds reversal pairs, running each phase the locale pack declares in order.
 * Phases share the set of already-paired transactions, so an earlier phase wins
 * a contested fitid — which is why the pack's array order is meaningful.
 */
export function detectReversalPairs(
  txs: ParsedTransaction[],
  phases: ReversalPhase[],
): Map<string, ReversalInfo> {
  const result = new Map<string, ReversalInfo>();
  const used = new Set<string>();
  for (const phase of phases) {
    STRATEGIES[phase.strategy]?.(txs, phase, used, result);
  }
  return result;
}
