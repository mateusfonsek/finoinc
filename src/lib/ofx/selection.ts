import { txKeyString } from "$lib/api/transactions";
import { detectReversalPairs, type ReversalInfo, type ReversalPhase } from "./reversals";
import type { ParsedTransaction } from "$lib/statement/types";

/** What Import pre-selects: everything except rows already in the database,
 *  reversal pairs that cancel out inside this statement and — for a CSV —
 *  rows that probably repeat one stored under another id (`probable`, keyed
 *  like `duplicateKeys`). Probable rows stay visible and can be re-selected. */
export function planImport(
  txs: ParsedTransaction[],
  duplicateKeys: Set<string>,
  phases: ReversalPhase[],
  probable: Set<string> = new Set(),
): { reversals: Map<string, ReversalInfo>; selected: Set<string>; probable: Set<string> } {
  const keyOf = (t: ParsedTransaction) => txKeyString({ ofx_fitid: t.fitid ?? "", date: t.date, amount: t.amount });
  const isDuplicate = (t: ParsedTransaction) => !!t.fitid && duplicateKeys.has(keyOf(t));
  // Pairing runs on new rows only: a refund whose purchase came in with an
  // earlier statement has nothing left to cancel against, so it must import.
  const fresh = txs.filter((t) => !isDuplicate(t));
  const reversals = detectReversalPairs(fresh, phases);
  const probableIds = new Set(
    fresh.filter((t) => t.fitid && probable.has(keyOf(t))).map((t) => t.fitid as string),
  );
  const selected = new Set(
    fresh
      .filter((t) => t.fitid && !reversals.has(t.fitid) && !probableIds.has(t.fitid))
      .map((t) => t.fitid as string),
  );
  return { reversals, selected, probable: probableIds };
}
