/** A CSV as rows of text, before anything is interpreted. `preamble` is how
 *  many file lines come before the header (bank title, period, blank lines). */
export type CsvTable = { header: string[]; rows: string[][]; preamble: number };

/** The file has no data rows — empty, or a header alone. */
export class CsvEmptyError extends Error {
  constructor(message = "CSV has no rows") {
    super(message);
    this.name = "CsvEmptyError";
  }
}

const DELIMITERS = [",", ";", "\t"] as const;

type Record_ = { fields: string[]; line: number };

/**
 * Reads a bank CSV. Banks disagree on everything here: UTF-8 with or without
 * BOM or windows-1252, `,` or `;` or tab, and some put a title block above
 * the header. The delimiter is the one that splits the most lines into the
 * same number of columns; the header is the first line with that many.
 */
export function readCsv(bytes: Uint8Array): CsvTable {
  const text = decode(bytes);
  let best: { records: Record_[]; width: number; count: number } | null = null;
  for (const d of DELIMITERS) {
    const records = split(text, d).filter((r) => !isBlank(r.fields));
    const { width, count } = modalWidth(records);
    if (width < 2) continue;
    if (!best || count > best.count || (count === best.count && width > best.width)) {
      best = { records, width, count };
    }
  }
  if (!best) throw new CsvEmptyError();
  // Some exports close every data row with a delimiter the header lacks; the
  // empty trailing fields every full-width row shares don't count against it.
  const { width } = best;
  const trailing = Math.min(
    ...best.records.filter((r) => r.fields.length === width).map((r) => trailingEmpty(r.fields)),
  );
  const headerAt = best.records.findIndex(
    (r) => r.fields.length <= width && r.fields.length >= Math.max(2, width - trailing),
  );
  const header = best.records[headerAt];
  const rows = best.records.slice(headerAt + 1).map((r) => r.fields);
  if (rows.length === 0) throw new CsvEmptyError();
  return { header: header.fields.map((h) => h.trim()), rows, preamble: header.line };
}

function decode(bytes: Uint8Array): string {
  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    text = new TextDecoder("windows-1252").decode(bytes);
  }
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

function trailingEmpty(fields: string[]): number {
  let n = 0;
  while (n < fields.length && fields[fields.length - 1 - n].trim() === "") n++;
  return n;
}

function isBlank(fields: string[]): boolean {
  return fields.every((f) => f.trim() === "");
}

function modalWidth(records: Record_[]): { width: number; count: number } {
  const counts = new Map<number, number>();
  for (const r of records) counts.set(r.fields.length, (counts.get(r.fields.length) ?? 0) + 1);
  let width = 0;
  let count = 0;
  for (const [w, c] of counts) {
    if (c > count || (c === count && w > width)) [width, count] = [w, c];
  }
  return { width, count };
}

/** RFC 4180 split: quotes may hold the delimiter, newlines and `""`. Each
 *  record keeps the 0-based file line it starts on. */
function split(text: string, delimiter: string): Record_[] {
  const out: Record_[] = [];
  let fields: string[] = [];
  let field = "";
  let quoted = false;
  let line = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') {
        quoted = false;
      } else {
        if (c === "\n") line++;
        field += c;
      }
    } else if (c === '"' && field === "") {
      quoted = true;
    } else if (c === delimiter) {
      fields.push(field);
      field = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      fields.push(field);
      out.push({ fields, line: start });
      fields = [];
      field = "";
      line++;
      start = line;
    } else {
      field += c;
    }
  }
  if (field !== "" || fields.length > 0) {
    fields.push(field);
    out.push({ fields, line: start });
  }
  return out;
}
