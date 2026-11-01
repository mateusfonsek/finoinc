export type ColumnRole = "date" | "description" | "amount" | "inflow" | "outflow" | "bankId";

/**
 * How to read one bank's CSV. `columns` holds indexes into the header of the
 * file at hand; `columnNames` keeps the header names those indexes came from,
 * so a saved format still lines up when the bank reorders its columns.
 */
export type CsvFormat = {
  id: string;
  name: string;
  bank: string | null;
  kind: "checking" | "credit_card";
  signature: string;
  columns: { date: number; description: number; amount?: number; inflow?: number; outflow?: number; bankId?: number };
  columnNames: Partial<Record<ColumnRole, string>>;
  dateFormat: "dd/mm/yyyy" | "yyyy-mm-dd" | "mm/dd/yyyy";
  /** Card bills list purchases as positive; flip them to spending. */
  invertSign: boolean;
  builtin: boolean;
};

/** A header name reduced to what identifies it: no BOM, accents, case,
 *  units in parentheses or punctuation. */
export function normalizeName(name: string): string {
  return name
    .replace(/^﻿/, "")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/\(.*?\)/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

/** Identifies a CSV layout by its column names, in any order. Empty names
 *  (a trailing delimiter Excel adds) are left out. */
export function signatureOf(header: string[]): string {
  return header
    .map(normalizeName)
    .filter((n) => n !== "")
    .sort()
    .join("|");
}

function builtin(
  id: string,
  name: string,
  kind: CsvFormat["kind"],
  columnNames: CsvFormat["columnNames"],
  dateFormat: CsvFormat["dateFormat"],
  invertSign: boolean,
): CsvFormat {
  const names = Object.values(columnNames) as string[];
  const columns = Object.fromEntries(
    Object.entries(columnNames).map(([role, n]) => [role, names.indexOf(n as string)]),
  ) as CsvFormat["columns"];
  return { id, name, bank: "nubank", kind, signature: signatureOf(names), columns, columnNames, dateFormat, invertSign, builtin: true };
}

/** Formats that ship with the app. Each one is tested against a real export
 *  in `__fixtures__`; a bank joins this list only with a sample. */
export const BUILTIN_FORMATS: CsvFormat[] = [
  builtin(
    "nubank-checking",
    "Nubank — conta",
    "checking",
    { date: "Data", amount: "Valor", bankId: "Identificador", description: "Descrição" },
    "dd/mm/yyyy",
    false,
  ),
  builtin(
    "nubank-card",
    "Nubank — cartão",
    "credit_card",
    { date: "date", description: "title", amount: "amount" },
    "yyyy-mm-dd",
    true,
  ),
];

/** The format this header belongs to — saved ones first, so the user's own
 *  mapping beats a built-in — with its column indexes pointed at this file's
 *  order. `null` when no format matches. */
export function matchFormat(header: string[], saved: CsvFormat[]): CsvFormat | null {
  const sig = signatureOf(header);
  const found = [...saved, ...BUILTIN_FORMATS].find((f) => f.signature === sig);
  if (!found) return null;
  const at = header.map(normalizeName);
  const columns = { ...found.columns };
  for (const [role, name] of Object.entries(found.columnNames)) {
    const i = at.indexOf(normalizeName(name as string));
    if (i >= 0) (columns as Record<string, number>)[role] = i;
  }
  return { ...found, columns };
}

const GUESSES: Record<ColumnRole, string[]> = {
  date: ["data", "date", "data lancamento", "data do lancamento", "data movimento", "data da transacao"],
  description: ["descricao", "historico", "title", "lancamento", "description", "memo", "estabelecimento", "detalhes"],
  amount: ["valor", "amount", "value", "valor r"],
  inflow: ["entrada", "entradas", "credito", "creditos"],
  outflow: ["saida", "saidas", "debito", "debitos"],
  bankId: ["identificador", "id", "id transacao", "codigo"],
};

/** Pre-fills the mapping from the header names banks commonly use. */
export function guessColumns(header: string[]): Partial<CsvFormat["columns"]> {
  const out: Partial<CsvFormat["columns"]> = {};
  header.map(normalizeName).forEach((n, i) => {
    for (const [role, names] of Object.entries(GUESSES) as [ColumnRole, string[]][]) {
      if (out[role] === undefined && names.includes(n)) {
        out[role] = i;
        return;
      }
    }
  });
  return out;
}

/** The date layout of a column's values. Day-first unless a value proves
 *  otherwise — Brazilian banks are the common case. */
export function guessDateFormat(samples: string[]): CsvFormat["dateFormat"] {
  const s = samples.map((v) => v.trim());
  if (s.some((v) => /^\d{4}-\d{1,2}-\d{1,2}/.test(v))) return "yyyy-mm-dd";
  const parts = s.map((v) => v.match(/^(\d{1,2})\/(\d{1,2})\/\d{4}/)).filter((m) => m !== null);
  if (parts.some((m) => Number(m[1]) > 12)) return "dd/mm/yyyy";
  if (parts.some((m) => Number(m[2]) > 12)) return "mm/dd/yyyy";
  return "dd/mm/yyyy";
}
