import { describe, expect, it } from "vitest";
import type { ParsedTransaction } from "$lib/statement/types";
import { withSyntheticFitids } from "./fitid";

const tx = (description: string, amount = "-10.00", date = "2026-10-01", fitid: string | null = null): ParsedTransaction => ({
  fitid,
  date,
  amount,
  description,
});

describe("withSyntheticFitids", () => {
  it("format_is_csv_hash_occurrence", async () => {
    const [t] = await withSyntheticFitids([tx("UBER")]);
    expect(t.fitid).toMatch(/^csv:[0-9a-f]{16}#1$/);
  });

  it("identical_rows_get_distinct_ids", async () => {
    const [a, b] = await withSyntheticFitids([tx("UBER"), tx("UBER")]);
    expect(a.fitid).not.toBe(b.fitid);
    expect(a.fitid!.slice(0, -2)).toBe(b.fitid!.slice(0, -2));
    expect(a.fitid!.endsWith("#1")).toBe(true);
    expect(b.fitid!.endsWith("#2")).toBe(true);
  });

  it("reimport_same_csv_yields_same_fitids", async () => {
    const input = () => [tx("UBER"), tx("PADARIA", "-5.00"), tx("UBER")];
    const first = (await withSyntheticFitids(input())).map((t) => t.fitid);
    const second = (await withSyntheticFitids(input())).map((t) => t.fitid);
    expect(second).toEqual(first);
  });

  it("overlapping_period_keeps_ids", async () => {
    const r1 = tx("A", "-1.00", "2026-10-01");
    const r2 = tx("B", "-2.00", "2026-10-02");
    const r3 = tx("C", "-3.00", "2026-10-03");
    const a = await withSyntheticFitids([r1, r2]);
    const b = await withSyntheticFitids([r2, r3]);
    expect(a[1].fitid).toBe(b[0].fitid);
  });

  it("description_normalized_before_hash", async () => {
    const [a] = await withSyntheticFitids([tx("Uber  trip")]);
    const [b] = await withSyntheticFitids([tx(" UBER TRIP ")]);
    expect(a.fitid).toBe(b.fitid);
  });

  it("bank_ids_untouched", async () => {
    const [t] = await withSyntheticFitids([tx("UBER", "-1.00", "2026-10-01", "abc")]);
    expect(t.fitid).toBe("abc");
  });
});
