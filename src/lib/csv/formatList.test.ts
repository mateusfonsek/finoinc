import { describe, expect, it } from "vitest";
import type { Account } from "$lib/bindings";
import type { CsvFormat } from "./formats";
import { formatListItems } from "./formatList";

const fmt = (id: string, name: string): CsvFormat => ({
  id,
  name,
  bank: null,
  kind: "checking",
  signature: id,
  columns: { date: 0, description: 1, amount: 2 },
  columnNames: {},
  dateFormat: "dd/mm/yyyy",
  invertSign: false,
  builtin: false,
});

const acc = { id: 3, name: "Nubank PF", bank: "nubank", ofx_acctid: null, kind: "credit_card", created_at: "" } as Account;

describe("formatListItems", () => {
  it("builtins_listed_first_and_not_deletable", () => {
    const items = formatListItems([fmt("b", "Zeta"), fmt("a", "Inter")], [], {});
    expect(items.map((i) => i.id)).toEqual(["nubank-checking", "nubank-card", "a", "b"]);
    expect(items.filter((i) => i.builtin).map((i) => i.id)).toEqual(["nubank-checking", "nubank-card"]);
  });

  it("remembered_account_name_resolved", () => {
    const items = formatListItems([], [acc], { "nubank-card": 3, "nubank-checking": 99 });
    expect(items.find((i) => i.id === "nubank-card")?.accountName).toBe("Nubank PF");
    expect(items.find((i) => i.id === "nubank-checking")?.accountName).toBeNull();
  });
});
