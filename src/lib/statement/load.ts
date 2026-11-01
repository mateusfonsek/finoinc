import { readFileBytes } from "$lib/api/files";
import { t } from "$lib/i18n/locale.svelte";
import { withSyntheticFitids } from "$lib/csv/fitid";
import { type CsvFormat, guessColumns, matchFormat } from "$lib/csv/formats";
import { CsvMappingError, parseCsv, type SkippedRow } from "$lib/csv/parse";
import { CsvEmptyError, type CsvTable, readCsv } from "$lib/csv/read";
import { decodeOfxFile } from "$lib/ofx/normalize";
import { parseOfx } from "$lib/ofx/parse";
import { StatementReadError } from "./errors";
import type { ParsedStatement } from "./types";

/** A statement ready for the preview, or a CSV whose layout Finoinc does not
 *  know yet — the mapping screen turns the second into the first. */
export type LoadResult =
  | { kind: "parsed"; file: File; parsed: ParsedStatement & { skipped?: SkippedRow[] }; formatId?: string }
  | {
      kind: "needs_mapping";
      file: File;
      table: CsvTable;
      guess: Partial<CsvFormat["columns"]>;
      /** The format that matched the header but read no row: the mapping
       *  starts from it, and `failed` names the column that broke. */
      seed?: CsvFormat;
      failed?: "date" | "amount";
    };

/** Neither `.ofx` nor `.csv`. */
export class UnsupportedFileError extends Error {
  constructor() {
    super(t("import.unsupported_file"));
    this.name = "UnsupportedFileError";
  }
}

export function isStatementName(name: string): boolean {
  return /\.(ofx|csv)$/i.test(name);
}

/** The one door every import path goes through: drop, file picker, Open With
 *  and the watched folder. `saved` are the user's CSV formats. */
export async function loadStatement(file: File, saved: CsvFormat[]): Promise<LoadResult> {
  if (/\.ofx$/i.test(file.name)) {
    return { kind: "parsed", file, parsed: parseOfx(await decodeOfxFile(file)) };
  }
  if (!/\.csv$/i.test(file.name)) throw new UnsupportedFileError();

  let table: CsvTable;
  try {
    table = readCsv(new Uint8Array(await file.arrayBuffer()));
  } catch (e) {
    if (e instanceof CsvEmptyError) throw new CsvEmptyError(t("import.csv_empty"));
    throw e;
  }
  const format = matchFormat(table.header, saved);
  if (!format) return { kind: "needs_mapping", file, table, guess: guessColumns(table.header) };
  let parsed: ReturnType<typeof parseCsv>;
  try {
    parsed = parseCsv(table, format);
  } catch (e) {
    if (!(e instanceof CsvMappingError)) throw e;
    return { kind: "needs_mapping", file, table, guess: guessColumns(table.header), seed: format, failed: e.column };
  }
  return {
    kind: "parsed",
    file,
    parsed: { ...parsed, transactions: await withSyntheticFitids(parsed.transactions) },
    formatId: format.id,
  };
}

/** [`loadStatement`] by path. A failure to read the bytes is a
 *  `StatementReadError` (transient); anything after that is about content. */
export async function loadStatementFromPath(path: string, saved: CsvFormat[]): Promise<LoadResult> {
  let bytes: Uint8Array;
  try {
    bytes = await readFileBytes(path);
  } catch (e) {
    throw new StatementReadError(path, e);
  }
  const name = path.split(/[\\/]/).pop() || "extrato";
  return loadStatement(new File([bytes as BlobPart], name), saved);
}
