import { txKeyString } from "$lib/api/transactions";
import { detectReversalPairs, type ReversalInfo, type ReversalPhase } from "./reversals";
import type { ParsedTransaction } from "./types";

/** What Import pre-selects: everything except rows already in the database
 *  and reversal pairs that cancel out inside this statement. */
export function planImport(
  txs: ParsedTransaction[],
  duplicateKeys: Set<string>,
  phases: ReversalPhase[],
): { reversals: Map<string, ReversalInfo>; selected: Set<string> } {
  const isDuplicate = (t: ParsedTransaction) =>
    !!t.fitid && duplicateKeys.has(txKeyString({ ofx_fitid: t.fitid, date: t.date, amount: t.amount }));
  // Pairing runs on new rows only: a refund whose purchase came in with an
  // earlier statement has nothing left to cancel against, so it must import.
  const fresh = txs.filter((t) => !isDuplicate(t));
  const reversals = detectReversalPairs(fresh, phases);
  const selected = new Set(
    fresh.filter((t) => t.fitid && !reversals.has(t.fitid)).map((t) => t.fitid as string),
  );
  return { reversals, selected };
}
