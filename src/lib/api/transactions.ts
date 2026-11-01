import { commands } from "../bindings";
import type {
  ExpenseRow,
  InsertResult,
  NewTransaction,
  Transaction,
  TransactionFilters,
  TxKey,
} from "../bindings";

function unwrap<T>(result: { status: "ok"; data: T } | { status: "error"; error: string }): T {
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}

export async function listTransactions(
  filters: TransactionFilters | null = null,
): Promise<Transaction[]> {
  return unwrap(await commands.listTransactions(filters));
}

export async function topExpenses(
  month: string | null = null,
  limit: number | null = null,
): Promise<ExpenseRow[]> {
  return unwrap(await commands.topExpenses(month, limit));
}

export async function insertTransactions(
  accountId: number,
  txs: NewTransaction[],
): Promise<InsertResult> {
  return unwrap(await commands.insertTransactions(accountId, txs));
}

/** Composite key for duplicate detection. The pipe is safe because FITID is a
 *  UUID, date is ISO and amount is decimal — none contains `|`. */
export function txKeyString(k: TxKey): string {
  return `${k.ofx_fitid}|${k.date}|${k.amount}`;
}

export async function checkExistingTxKeys(
  accountId: number,
  keys: TxKey[],
): Promise<Set<string>> {
  const existing = unwrap(await commands.checkExistingTxKeys(accountId, keys));
  return new Set(existing.map(txKeyString));
}

/** Keys that probably repeat a row stored under another id (an account
 *  imported by OFX before and by CSV now). Same key format as
 *  `checkExistingTxKeys`. */
export async function probableDuplicateKeys(
  accountId: number,
  keys: TxKey[],
): Promise<Set<string>> {
  const found = unwrap(await commands.probableDuplicateKeys(accountId, keys));
  return new Set(found.map(txKeyString));
}

export async function updateTransactionCategory(
  transactionId: number,
  categoryId: number | null,
): Promise<void> {
  unwrap(await commands.updateTransactionCategory(transactionId, categoryId));
}

export async function updateTransactionNotes(
  transactionId: number,
  notes: string | null,
): Promise<void> {
  unwrap(await commands.updateTransactionNotes(transactionId, notes));
}
