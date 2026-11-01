import { describe, expect, it } from "vitest";
import type { CsvFormat } from "$lib/csv/formats";
import { formatToRow, rowToFormat } from "./csvFormats";

const f: CsvFormat = {
  id: "x",
  name: "Inter",
  bank: "inter",
  kind: "checking",
  signature: "data|historico|valor",
  columns: { date: 0, description: 1, amount: 2 },
  columnNames: { date: "Data", description: "Histórico", amount: "Valor" },
  dateFormat: "dd/mm/yyyy",
  invertSign: false,
  builtin: false,
};

describe("csv format rows", () => {
  it("row_spec_roundtrip", () => {
    const row = formatToRow(f);
    expect(JSON.parse(row.spec)).toEqual({
      columns: f.columns,
      columnNames: f.columnNames,
      dateFormat: f.dateFormat,
      invertSign: false,
    });
    expect(rowToFormat({ ...row, created_at: "2026-11-01" })).toEqual(f);
  });
});
