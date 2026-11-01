import type { ParsedSummary, ParsedTransaction } from "./types";

/** Totals and date range of a statement, the same for OFX and CSV. */
export function computeSummary(txs: ParsedTransaction[]): ParsedSummary {
  let totalIn = 0;
  let totalOut = 0;
  let earliest: string | null = null;
  let latest: string | null = null;
  for (const t of txs) {
    const n = Number(t.amount);
    if (Number.isFinite(n)) {
      if (n >= 0) totalIn += n;
      else totalOut += -n;
    }
    if (t.date) {
      if (earliest === null || t.date < earliest) earliest = t.date;
      if (latest === null || t.date > latest) latest = t.date;
    }
  }
  return {
    totalIn: totalIn.toFixed(2),
    totalOut: totalOut.toFixed(2),
    net: (totalIn - totalOut).toFixed(2),
    earliest,
    latest,
  };
}
