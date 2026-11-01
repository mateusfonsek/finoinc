import type { ParsedTransaction } from "$lib/statement/types";
import { type ColumnRole, type CsvFormat, guessColumns, guessDateFormat, signatureOf } from "./formats";
import { parseAmount, parseCsv, parseDate } from "./parse";
import type { CsvTable } from "./read";

export type Role = ColumnRole | "ignore";

/** The mapping screen's state: one role per column plus the format's settings. */
export type MappingDraft = {
  name: string;
  kind: CsvFormat["kind"];
  dateFormat: CsvFormat["dateFormat"];
  invertSign: boolean;
  roles: Role[];
};

/** An error on a column (`column` = its index) or on the mapping as a whole
 *  (`column` = -1). `message` is an i18n key. */
export type DraftError = { column: number; message: string };

/** Rows checked when validating: enough to tell a date column from text,
 *  cheap enough to run on every change. */
const SAMPLE = 20;

/** The mapping screen's starting point: guessed from the header names, or —
 *  when a known format stopped reading this file — that format's own mapping,
 *  with only the date format guessed again (usually what the bank changed). */
export function mappingDraft(table: CsvTable, fileName: string, seed?: CsvFormat): MappingDraft {
  const columns: Partial<CsvFormat["columns"]> = seed ? seed.columns : guessColumns(table.header);
  const roles: Role[] = table.header.map(() => "ignore");
  for (const [role, i] of Object.entries(columns) as [ColumnRole, number | undefined][]) {
    if (i !== undefined && i < roles.length) roles[i] = role;
  }
  const dateCol = columns.date;
  return {
    name: seed?.name ?? fileName.replace(/\.[^.]+$/, ""),
    kind: seed?.kind ?? "checking",
    dateFormat: dateCol === undefined ? "dd/mm/yyyy" : guessDateFormat(table.rows.slice(0, SAMPLE).map((r) => r[dateCol] ?? "")),
    invertSign: seed?.invertSign ?? false,
    roles,
  };
}

function columnsOf(draft: MappingDraft): Partial<CsvFormat["columns"]> {
  const out: Partial<CsvFormat["columns"]> = {};
  draft.roles.forEach((role, i) => {
    if (role !== "ignore" && out[role] === undefined) out[role] = i;
  });
  return out;
}

export function validateDraft(draft: MappingDraft, table: CsvTable): DraftError[] {
  const errors: DraftError[] = [];
  const seen = new Set<Role>();
  draft.roles.forEach((role, i) => {
    if (role === "ignore") return;
    if (seen.has(role)) errors.push({ column: i, message: "import.mapping.err_repeated" });
    seen.add(role);
  });
  const c = columnsOf(draft);
  if (c.date === undefined) errors.push({ column: -1, message: "import.mapping.err_need_date" });
  if (c.description === undefined) errors.push({ column: -1, message: "import.mapping.err_need_description" });
  const split = c.inflow !== undefined || c.outflow !== undefined;
  if (c.amount === undefined && !split) errors.push({ column: -1, message: "import.mapping.err_need_amount" });
  if (c.amount !== undefined && split) errors.push({ column: -1, message: "import.mapping.err_amount_or_split" });

  const sample = table.rows.slice(0, SAMPLE);
  const some = (col: number, ok: (v: string) => boolean) => sample.some((r) => ok(r[col] ?? ""));
  if (c.date !== undefined && !some(c.date, (v) => parseDate(v, draft.dateFormat) !== null)) {
    errors.push({ column: c.date, message: "import.mapping.err_no_dates" });
  }
  for (const role of ["amount", "inflow", "outflow"] as const) {
    const col = c[role];
    if (col !== undefined && !some(col, (v) => parseAmount(v) !== null)) {
      errors.push({ column: col, message: "import.mapping.err_no_amounts" });
    }
  }
  return errors;
}

export function draftToFormat(draft: MappingDraft, table: CsvTable): CsvFormat {
  const columns = columnsOf(draft) as CsvFormat["columns"];
  const columnNames: CsvFormat["columnNames"] = {};
  for (const [role, i] of Object.entries(columns) as [ColumnRole, number][]) columnNames[role] = table.header[i];
  return {
    id: crypto.randomUUID(),
    name: draft.name.trim() || "CSV",
    bank: null,
    kind: draft.kind,
    signature: signatureOf(table.header),
    columns,
    columnNames,
    dateFormat: draft.dateFormat,
    invertSign: draft.invertSign,
    builtin: false,
  };
}

/** The first `n` rows as the draft reads them, for the live preview. Empty
 *  while the draft is invalid. */
export function previewRows(draft: MappingDraft, table: CsvTable, n = 3): ParsedTransaction[] {
  if (validateDraft(draft, table).length > 0) return [];
  try {
    return parseCsv({ ...table, rows: table.rows.slice(0, SAMPLE) }, draftToFormat(draft, table)).transactions.slice(0, n);
  } catch {
    return [];
  }
}
