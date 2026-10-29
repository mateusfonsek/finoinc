import { describe, it, expect } from "vitest";
import { planImport } from "./selection";
import type { ReversalPhase } from "./reversals";
import type { ParsedTransaction } from "./types";

const PHASES: ReversalPhase[] = [
  { strategy: "exact_remainder", prefix: "Estorno - ", window_days: 7,
    role: "reversal", counterpart_role: "reversed" },
];

const purchase: ParsedTransaction = { fitid: "P", date: "2026-03-09", amount: "-50.00", description: "LOJA FOO" };
const refund: ParsedTransaction = { fitid: "R", date: "2026-03-12", amount: "50.00", description: "Estorno - LOJA FOO" };
const other: ParsedTransaction = { fitid: "O", date: "2026-03-13", amount: "-9.90", description: "PADARIA" };

describe("planImport", () => {
  it("leaves out duplicates and a pair that cancels out within the statement", () => {
    const plan = planImport([purchase, refund, other], new Set(), PHASES);
    expect([...plan.selected]).toEqual(["O"]);
    expect(plan.reversals.size).toBe(2);
  });

  // The purchase came in with an earlier, overlapping statement. Pairing it
  // again would leave the refund out and the month off by its amount.
  it("selects a refund whose purchase was already imported", () => {
    const already = new Set(["P|2026-03-09|-50.00"]);
    const plan = planImport([purchase, refund, other], already, PHASES);
    expect([...plan.selected].sort()).toEqual(["O", "R"]);
    expect(plan.reversals.size).toBe(0);
  });
});
