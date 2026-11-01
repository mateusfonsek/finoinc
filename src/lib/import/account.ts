import type { Account, NewAccount } from "$lib/bindings";
import type { ParsedStatement } from "$lib/statement/types";

export type AccountDeps = {
  createOrGetAccount: (input: NewAccount) => Promise<Account>;
  getFormatAccount: (formatId: string) => Promise<number | null>;
  listAccounts: () => Promise<Account[]>;
};

/** What the user picked: an account that exists, or a name for a new one. */
export type AccountChoice = { accountId: number } | { newName: string };

/** Either the account to import into, or the picker has to ask. */
export type AccountResolution =
  | { account: Account }
  | { ask: { kind: ParsedStatement["account"]["type"]; remembered: number | null } };

/**
 * An OFX names its account (`ACCTID`), so it is found or created as always. A
 * CSV does not: it goes to the account its format imported into last time —
 * if that account still exists — and otherwise the user picks one.
 */
export async function resolveAccount(
  parsed: ParsedStatement,
  formatId: string | undefined,
  deps: AccountDeps,
): Promise<AccountResolution> {
  const a = parsed.account;
  if (a.ofxAcctid !== null || formatId === undefined) {
    return {
      account: await deps.createOrGetAccount({
        name: a.displayName,
        bank: a.bank === "unknown" ? null : a.bank,
        ofx_acctid: a.ofxAcctid,
        kind: a.type,
      }),
    };
  }
  const remembered = await deps.getFormatAccount(formatId);
  if (remembered !== null) {
    const found = (await deps.listAccounts()).find((x) => x.id === remembered);
    if (found) return { account: found };
  }
  return { ask: { kind: a.type, remembered: null } };
}

/** The picker's "New account" choice. */
export const NEW_ACCOUNT = -1;

/** What the account picker starts on: the remembered account, else one of
 *  the statement's own bank and type, else "New account" — never just any
 *  account of the same type, which one click would then make permanent. */
export function defaultPick(accounts: Account[], kind: string, bank: string | null, initial: number | null): number {
  if (initial !== null && accounts.some((a) => a.id === initial)) return initial;
  const same = bank ? accounts.find((a) => a.kind === kind && a.bank === bank) : undefined;
  return same?.id ?? NEW_ACCOUNT;
}
