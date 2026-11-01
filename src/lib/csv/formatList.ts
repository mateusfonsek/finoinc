import type { Account } from "$lib/bindings";
import { BUILTIN_FORMATS, type CsvFormat } from "./formats";

export type FormatListItem = {
  id: string;
  name: string;
  bank: string | null;
  /** The account the format imports into, `null` when none is remembered. */
  accountName: string | null;
  builtin: boolean;
};

/** Settings' list of CSV formats: built-ins first (they cannot be deleted),
 *  then the user's own by name. */
export function formatListItems(
  saved: CsvFormat[],
  accounts: Account[],
  remembered: Record<string, number | null>,
): FormatListItem[] {
  const byName = [...saved].sort((a, b) => a.name.localeCompare(b.name));
  return [...BUILTIN_FORMATS, ...byName].map((f) => ({
    id: f.id,
    name: f.name,
    bank: f.bank,
    accountName: accounts.find((a) => a.id === remembered[f.id])?.name ?? null,
    builtin: f.builtin,
  }));
}
