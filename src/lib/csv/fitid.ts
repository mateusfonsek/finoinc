import type { ParsedTransaction } from "$lib/statement/types";

/**
 * Gives every row the bank left without an id one derived from what it says,
 * so the database's `UNIQUE(account_id, ofx_fitid, date, amount)` skips it
 * when the same CSV, or an overlapping period, comes in again.
 *
 * `#n` counts identical rows within the file: two equal Uber rides on the same
 * day stay two, and re-importing the file produces the same `#1`, `#2`.
 */
export async function withSyntheticFitids(txs: ParsedTransaction[]): Promise<ParsedTransaction[]> {
  const seen = new Map<string, number>();
  return Promise.all(
    txs.map(async (t) => {
      if (t.fitid) return t;
      const key = `${t.date}|${t.amount}|${t.description.replace(/\s+/g, " ").trim().toUpperCase()}`;
      const n = (seen.get(key) ?? 0) + 1;
      seen.set(key, n);
      return { ...t, fitid: `csv:${await sha256Hex16(key)}#${n}` };
    }),
  );
}

async function sha256Hex16(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest).slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");
}
