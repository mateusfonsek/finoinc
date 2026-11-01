import { describe, expect, it, vi } from "vitest";
import type { Account } from "$lib/bindings";
import type { ParsedStatement } from "$lib/statement/types";
import { defaultPick, NEW_ACCOUNT, resolveAccount } from "./account";

const acc = (id: number, kind = "checking"): Account =>
  ({ id, name: `a${id}`, bank: null, ofx_acctid: null, kind, created_at: "" }) as Account;

const stmt = (ofxAcctid: string | null, type: "checking" | "credit_card" = "checking"): ParsedStatement => ({
  account: { bank: "nubank", ofxAcctid, ofxBankid: null, ofxFid: null, displayName: "Nubank", type },
  transactions: [],
  summary: { totalIn: "0", totalOut: "0", net: "0", earliest: null, latest: null },
});

const deps = (over: Partial<Parameters<typeof resolveAccount>[2]> = {}) => ({
  createOrGetAccount: vi.fn(async () => acc(9)),
  getFormatAccount: vi.fn(async () => null as number | null),
  listAccounts: vi.fn(async () => [acc(1), acc(2, "credit_card")]),
  ...over,
});

describe("resolveAccount", () => {
  it("ofx_uses_acctid_as_today", async () => {
    const d = deps();
    const r = await resolveAccount(stmt("123"), undefined, d);
    expect(r).toEqual({ account: acc(9) });
    expect(d.createOrGetAccount).toHaveBeenCalledWith({ name: "Nubank", bank: "nubank", ofx_acctid: "123", kind: "checking" });
    expect(d.getFormatAccount).not.toHaveBeenCalled();
  });

  it("csv_with_remembered_account_skips_picker", async () => {
    const d = deps({ getFormatAccount: vi.fn(async () => 2) });
    const r = await resolveAccount(stmt(null, "credit_card"), "nubank-card", d);
    expect(r).toEqual({ account: acc(2, "credit_card") });
    expect(d.createOrGetAccount).not.toHaveBeenCalled();
  });

  it("csv_first_time_asks", async () => {
    const r = await resolveAccount(stmt(null, "credit_card"), "nubank-card", deps());
    expect(r).toEqual({ ask: { kind: "credit_card", remembered: null } });
  });

  it("remembered_account_deleted_asks_again", async () => {
    const d = deps({ getFormatAccount: vi.fn(async () => 77) });
    const r = await resolveAccount(stmt(null), "inter", d);
    expect(r).toEqual({ ask: { kind: "checking", remembered: null } });
  });
});

describe("defaultPick", () => {
  const nubankCard = { ...acc(5, "credit_card"), bank: "nubank" } as Account;
  const interCard = { ...acc(6, "credit_card"), bank: "inter" } as Account;

  it("remembered_account_wins", () => {
    expect(defaultPick([nubankCard, interCard], "credit_card", "nubank", 6)).toBe(6);
  });

  it("same_bank_and_kind_is_preselected", () => {
    expect(defaultPick([interCard, nubankCard], "credit_card", "nubank", null)).toBe(5);
  });

  // A first CSV from a new bank must not land in another bank's account on
  // one click — and be remembered there.
  it("no_matching_bank_defaults_to_new_account", () => {
    expect(defaultPick([interCard], "credit_card", "nubank", null)).toBe(NEW_ACCOUNT);
    expect(defaultPick([interCard], "credit_card", null, null)).toBe(NEW_ACCOUNT);
  });
});
