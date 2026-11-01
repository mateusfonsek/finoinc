import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi, beforeEach } from "vitest";

const readFileBytes = vi.hoisted(() => vi.fn());
vi.mock("$lib/api/files", () => ({ readFileBytes }));

import { StatementReadError } from "./errors";
import { loadStatement, loadStatementFromPath, UnsupportedFileError } from "./load";
import { CsvEmptyError } from "$lib/csv/read";

const bytes = (rel: string) => new Uint8Array(readFileSync(join(__dirname, "..", rel)));
const file = (name: string, content: Uint8Array | string) =>
  new File([typeof content === "string" ? new TextEncoder().encode(content) : (content as BlobPart)], name);

describe("loadStatement", () => {
  it("ofx_goes_through_ofx_parser", async () => {
    const r = await loadStatement(file("itau.ofx", bytes("ofx/__fixtures__/itau-minimal.ofx")), []);
    expect(r.kind).toBe("parsed");
    if (r.kind !== "parsed") return;
    expect(r.formatId).toBeUndefined();
    expect(r.parsed.transactions.length).toBeGreaterThan(0);
  });

  it("nubank_csv_parsed_with_builtin", async () => {
    const r = await loadStatement(file("Nubank_2026-10.csv", bytes("csv/__fixtures__/nubank-card.csv")), []);
    expect(r.kind).toBe("parsed");
    if (r.kind !== "parsed") return;
    expect(r.formatId).toBe("nubank-card");
    expect(r.parsed.transactions.every((t) => t.fitid?.startsWith("csv:"))).toBe(true);
    expect(r.parsed.skipped).toEqual([]);
  });

  it("unknown_csv_needs_mapping", async () => {
    const r = await loadStatement(file("inter.csv", "Data;Histórico;Valor\n01/10/2026;PIX;-10,00"), []);
    expect(r.kind).toBe("needs_mapping");
    if (r.kind !== "needs_mapping") return;
    expect(r.guess).toEqual({ date: 0, description: 1, amount: 2 });
    expect(r.table.rows).toHaveLength(1);
  });

  it("extension_is_case_insensitive", async () => {
    const r = await loadStatement(file("EXTRATO.CSV", bytes("csv/__fixtures__/nubank-checking.csv")), []);
    expect(r.kind).toBe("parsed");
  });

  it("other_extension_rejected", async () => {
    await expect(loadStatement(file("fatura.pdf", "%PDF"), [])).rejects.toBeInstanceOf(UnsupportedFileError);
  });

  it("empty_csv_message", async () => {
    await expect(loadStatement(file("vazio.csv", "Data,Valor\n"), [])).rejects.toBeInstanceOf(CsvEmptyError);
  });
});

describe("loadStatementFromPath", () => {
  beforeEach(() => vi.clearAllMocks());

  it("read_failure_is_statement_read_error", async () => {
    readFileBytes.mockRejectedValue(new Error("gone"));
    await expect(loadStatementFromPath("/x/a.csv", [])).rejects.toBeInstanceOf(StatementReadError);
  });

  it("names_file_after_path", async () => {
    readFileBytes.mockResolvedValue(bytes("csv/__fixtures__/nubank-card.csv"));
    const r = await loadStatementFromPath("C:\\Users\\a\\Nubank.csv", []);
    expect(r.file.name).toBe("Nubank.csv");
  });

  // Whatever marks a watched file `invalid` (permanent) relies on this: junk
  // content must not look like a transient read failure.
  it("content_failure_is_not_a_read_error", async () => {
    readFileBytes.mockResolvedValue(new TextEncoder().encode("<OFX>"));
    await expect(loadStatementFromPath("/tmp/lixo.ofx", [])).rejects.toThrow();
    await expect(loadStatementFromPath("/tmp/lixo.ofx", [])).rejects.not.toBeInstanceOf(StatementReadError);
  });
});
