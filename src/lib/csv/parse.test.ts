import { describe, expect, it } from "vitest";
import type { CsvFormat } from "./formats";
import { CsvMappingError, parseAmount, parseCsv, parseDate } from "./parse";
import type { CsvTable } from "./read";

const format = (over: Partial<CsvFormat> = {}): CsvFormat => ({
  id: "f",
  name: "Meu banco",
  bank: "meubanco",
  kind: "checking",
  signature: "data|descricao|valor",
  columns: { date: 0, description: 1, amount: 2 },
  columnNames: {},
  dateFormat: "dd/mm/yyyy",
  invertSign: false,
  builtin: false,
  ...over,
});

const table = (rows: string[][], preamble = 0): CsvTable => ({
  header: ["Data", "Descrição", "Valor"],
  rows,
  preamble,
});

describe("parseAmount", () => {
  it("parses_brazilian_and_plain_amounts", () => {
    expect(parseAmount("1.234,56")).toBe("1234.56");
    expect(parseAmount("1234.56")).toBe("1234.56");
    expect(parseAmount("-R$ 10,00")).toBe("-10.00");
    expect(parseAmount("R$ -10,00")).toBe("-10.00");
    expect(parseAmount("(10,00)")).toBe("-10.00");
    expect(parseAmount("1,234.56")).toBe("1234.56");
    expect(parseAmount("5")).toBe("5.00");
    expect(parseAmount("abc")).toBeNull();
    expect(parseAmount("")).toBeNull();
  });

  it("amount_lone_dot_is_decimal", () => {
    expect(parseAmount("1.234")).toBe("1.234");
    expect(parseAmount("1.234.567")).toBe("1234567.00");
  });
});

describe("parseDate", () => {
  it("parses_three_date_formats", () => {
    expect(parseDate("31/10/2026", "dd/mm/yyyy")).toBe("2026-10-31");
    expect(parseDate("2026-10-31", "yyyy-mm-dd")).toBe("2026-10-31");
    expect(parseDate("10/31/2026", "mm/dd/yyyy")).toBe("2026-10-31");
    expect(parseDate("31/02/2026", "dd/mm/yyyy")).toBeNull();
    expect(parseDate("Saldo", "dd/mm/yyyy")).toBeNull();
  });
});

describe("parseCsv", () => {
  it("combines_inflow_and_outflow", () => {
    const f = format({ columns: { date: 0, description: 1, inflow: 2, outflow: 3 } });
    const p = parseCsv(
      { header: ["Data", "Desc", "Crédito", "Débito"], preamble: 0, rows: [
        ["01/10/2026", "SALARIO", "5,00", ""],
        ["02/10/2026", "MERCADO", "", "10,00"],
        ["03/10/2026", "TAXA", "", "-2,00"],
      ] },
      f,
    );
    expect(p.transactions.map((t) => t.amount)).toEqual(["5.00", "-10.00", "-2.00"]);
  });

  it("invert_sign_flips_card_purchases", () => {
    const p = parseCsv(table([["01/10/2026", "LOJA", "50.00"], ["02/10/2026", "PAGAMENTO", "-80.00"]]), format({ invertSign: true }));
    expect(p.transactions.map((t) => t.amount)).toEqual(["-50.00", "80.00"]);
  });

  it("footer_rows_are_skipped", () => {
    const p = parseCsv(
      table([["01/10/2026", "PIX", "-10,00"], ["", "Saldo final", "1.000,00"], ["02/10/2026", "X", "abc"]], 2),
      format(),
    );
    expect(p.transactions).toHaveLength(1);
    // preamble 2 + header line + 1-based row index
    expect(p.skipped).toEqual([
      { line: 5, reason: "date" },
      { line: 6, reason: "amount" },
    ]);
  });

  it("all_rows_failing_throws_mapping_error", () => {
    const run = () => parseCsv(table([["x", "PIX", "-10,00"], ["y", "TED", "5,00"]]), format());
    expect(run).toThrow(CsvMappingError);
    try {
      run();
    } catch (e) {
      expect((e as CsvMappingError).column).toBe("date");
    }
  });

  it("summary_matches_transactions", () => {
    const p = parseCsv(table([["02/10/2026", "A", "100,00"], ["01/10/2026", "B", "-30,00"]]), format());
    expect(p.summary).toEqual({
      totalIn: "100.00",
      totalOut: "30.00",
      net: "70.00",
      earliest: "2026-10-01",
      latest: "2026-10-02",
    });
  });

  it("account_comes_from_format", () => {
    const p = parseCsv(table([["01/10/2026", "  PIX   JOAO ", "1,00"]]), format({ kind: "credit_card" }));
    expect(p.account).toMatchObject({ bank: "meubanco", type: "credit_card", displayName: "Meu banco", ofxAcctid: null });
    expect(p.transactions[0]).toMatchObject({ description: "PIX JOAO", fitid: null });
  });

  it("bank_id_column_becomes_fitid", () => {
    const f = format({ columns: { date: 0, description: 1, amount: 2, bankId: 3 } });
    const p = parseCsv({ header: ["Data", "D", "V", "Id"], preamble: 0, rows: [["01/10/2026", "PIX", "1,00", "abc-1"]] }, f);
    expect(p.transactions[0].fitid).toBe("abc-1");
  });
});
