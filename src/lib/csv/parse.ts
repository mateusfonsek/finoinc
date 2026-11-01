import { normalizeTransactionText } from "$lib/ofx/normalize";
import { computeSummary } from "$lib/statement/summary";
import type { ParsedStatement, ParsedTransaction } from "$lib/statement/types";
import type { CsvFormat } from "./formats";
import type { CsvTable } from "./read";

/** A data row left out of the statement, by its 1-based line in the file. */
export type SkippedRow = { line: number; reason: "date" | "amount" };

/** No row parsed: the mapping is wrong, and `column` says where. */
export class CsvMappingError extends Error {
  readonly column: "date" | "amount";

  constructor(column: "date" | "amount") {
    super(`no row has a valid ${column}`);
    this.name = "CsvMappingError";
    this.column = column;
  }
}

/**
 * A money string as a plain decimal (`"-1234.56"`). Accepts `R$`, Brazilian
 * and US grouping and accounting parentheses. With both `.` and `,` the
 * rightmost is the decimal mark; a single `.` alone is decimal too, because
 * banks that use `.` for thousands also print cents with `,`.
 */
export function parseAmount(raw: string): string | null {
  let s = raw.replace(/R\$/gi, "").replace(/\s/g, "");
  let negative = false;
  if (/^\(.*\)$/.test(s)) {
    negative = true;
    s = s.slice(1, -1);
  }
  if (s.startsWith("-")) {
    negative = !negative;
    s = s.slice(1);
  } else if (s.startsWith("+")) {
    s = s.slice(1);
  }
  if (!/^[\d.,]+$/.test(s) || !/\d/.test(s)) return null;

  const lastDot = s.lastIndexOf(".");
  const lastComma = s.lastIndexOf(",");
  let intPart: string;
  let decPart = "";
  if (lastDot >= 0 && lastComma >= 0) {
    const mark = Math.max(lastDot, lastComma);
    intPart = s.slice(0, mark).replace(/[.,]/g, "");
    decPart = s.slice(mark + 1);
  } else if (lastComma >= 0) {
    if (s.indexOf(",") !== lastComma) return null;
    [intPart, decPart] = s.split(",");
  } else if (lastDot >= 0 && s.indexOf(".") === lastDot) {
    [intPart, decPart] = s.split(".");
  } else {
    intPart = s.replace(/\./g, "");
  }
  if (!/^\d*$/.test(intPart) || !/^\d*$/.test(decPart)) return null;
  const decimals = decPart.padEnd(2, "0");
  const whole = intPart.replace(/^0+(?=\d)/, "") || "0";
  const value = `${whole}.${decimals}`;
  return negative && /[1-9]/.test(value) ? `-${value}` : value;
}

/** A date in one of the formats banks use, as ISO `YYYY-MM-DD`. */
export function parseDate(raw: string, fmt: CsvFormat["dateFormat"]): string | null {
  const s = raw.trim();
  const m =
    fmt === "yyyy-mm-dd"
      ? s.match(/^(\d{4})-(\d{1,2})-(\d{1,2})/)
      : s.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})/);
  if (!m) return null;
  const [y, mo, d] =
    fmt === "yyyy-mm-dd" ? [m[1], m[2], m[3]] : fmt === "dd/mm/yyyy" ? [m[3], m[2], m[1]] : [m[3], m[1], m[2]];
  const date = new Date(Date.UTC(Number(y), Number(mo) - 1, Number(d)));
  if (date.getUTCMonth() !== Number(mo) - 1 || date.getUTCDate() !== Number(d)) return null;
  return `${y}-${mo.padStart(2, "0")}-${d.padStart(2, "0")}`;
}

function negate(amount: string): string {
  if (amount.startsWith("-")) return amount.slice(1);
  return /[1-9]/.test(amount) ? `-${amount}` : amount;
}

/** A split column's value: `null` when the cell says nothing (empty, `-`,
 *  zero), so the other column decides. */
function sideAmount(raw: string): string | null {
  const v = parseAmount(raw);
  return v === null || !/[1-9]/.test(v) ? null : v.replace(/^-/, "");
}

function rowAmount(row: string[], f: CsvFormat): string | null {
  const c = f.columns;
  let amount: string | null;
  if (c.amount !== undefined) {
    amount = parseAmount(row[c.amount] ?? "");
  } else {
    // Banks with Crédito/Débito columns fill the unused side with "", "-" or
    // "0,00". Whatever is left is inflow minus outflow.
    const cell = (i: number | undefined) => (i === undefined ? "" : (row[i] ?? "").trim());
    const inflow = sideAmount(cell(c.inflow));
    const outflow = sideAmount(cell(c.outflow));
    if (inflow === null && outflow === null) {
      const zero = [cell(c.inflow), cell(c.outflow)].some((v) => parseAmount(v) !== null);
      amount = zero ? "0.00" : null;
    } else if (outflow === null) amount = inflow;
    else if (inflow === null) amount = negate(outflow);
    else {
      const cents = Math.round(Number(inflow) * 100) - Math.round(Number(outflow) * 100);
      amount = (cents / 100).toFixed(2);
    }
  }
  if (amount !== null && f.invertSign) amount = negate(amount);
  return amount;
}

/**
 * Turns a CSV into the statement the import preview reads. A row without a
 * valid date or amount (footers such as "Saldo final", blank cells) is left
 * out and reported, never fatal — unless every row fails, which means the
 * mapping itself is wrong.
 */
export function parseCsv(table: CsvTable, format: CsvFormat): ParsedStatement & { skipped: SkippedRow[] } {
  const transactions: ParsedTransaction[] = [];
  const skipped: SkippedRow[] = [];
  let anyDate = false;
  table.rows.forEach((row, i) => {
    const line = table.preamble + 2 + i;
    const date = parseDate(row[format.columns.date] ?? "", format.dateFormat);
    if (!date) {
      skipped.push({ line, reason: "date" });
      return;
    }
    anyDate = true;
    const amount = rowAmount(row, format);
    if (amount === null) {
      skipped.push({ line, reason: "amount" });
      return;
    }
    const bankId = format.columns.bankId !== undefined ? (row[format.columns.bankId] ?? "").trim() : "";
    transactions.push({
      fitid: bankId || null,
      date,
      amount,
      description: normalizeTransactionText(row[format.columns.description] ?? ""),
    });
  });
  if (transactions.length === 0) throw new CsvMappingError(anyDate ? "amount" : "date");
  return {
    account: {
      bank: format.bank ?? "unknown",
      ofxAcctid: null,
      ofxBankid: null,
      ofxFid: null,
      displayName: format.name,
      type: format.kind,
    },
    transactions,
    summary: computeSummary(transactions),
    skipped,
  };
}
