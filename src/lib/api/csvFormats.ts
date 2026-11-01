import { commands } from "../bindings";
import type { CsvFormatRow, NewCsvFormat } from "../bindings";
import type { CsvFormat } from "$lib/csv/formats";

function unwrap<T>(result: { status: "ok"; data: T } | { status: "error"; error: string }): T {
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}

/** What the database keeps: identity columns plus the mapping as JSON. */
export function formatToRow(f: CsvFormat): NewCsvFormat {
  const { columns, columnNames, dateFormat, invertSign } = f;
  return {
    id: f.id,
    name: f.name,
    bank: f.bank,
    kind: f.kind,
    signature: f.signature,
    spec: JSON.stringify({ columns, columnNames, dateFormat, invertSign }),
  };
}

export function rowToFormat(row: CsvFormatRow): CsvFormat {
  const spec = JSON.parse(row.spec) as Pick<CsvFormat, "columns" | "columnNames" | "dateFormat" | "invertSign">;
  return {
    id: row.id,
    name: row.name,
    bank: row.bank,
    kind: row.kind as CsvFormat["kind"],
    signature: row.signature,
    ...spec,
    builtin: false,
  };
}

export async function listCsvFormats(): Promise<CsvFormat[]> {
  return unwrap(await commands.listCsvFormats()).map(rowToFormat);
}

export async function saveCsvFormat(f: CsvFormat): Promise<CsvFormat> {
  return rowToFormat(unwrap(await commands.saveCsvFormat(formatToRow(f))));
}

export async function deleteCsvFormat(id: string): Promise<void> {
  unwrap(await commands.deleteCsvFormat(id));
}

/** The account a format imported into last time, or `null` (never, or that
 *  account was deleted). */
export async function getFormatAccount(formatId: string): Promise<number | null> {
  return unwrap(await commands.getFormatAccount(formatId));
}

export async function setFormatAccount(formatId: string, accountId: number): Promise<void> {
  unwrap(await commands.setFormatAccount(formatId, accountId));
}
