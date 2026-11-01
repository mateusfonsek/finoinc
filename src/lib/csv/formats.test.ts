import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { withSyntheticFitids } from "./fitid";
import {
  BUILTIN_FORMATS,
  type CsvFormat,
  guessColumns,
  guessDateFormat,
  matchFormat,
  signatureOf,
} from "./formats";
import { parseCsv } from "./parse";
import { readCsv } from "./read";

const fixture = (name: string) => readCsv(new Uint8Array(readFileSync(join(__dirname, "__fixtures__", name))));

const saved = (header: string[], over: Partial<CsvFormat> = {}): CsvFormat => ({
  id: "mine",
  name: "Meu",
  bank: null,
  kind: "checking",
  signature: signatureOf(header),
  columns: { date: 0, description: 2, amount: 1 },
  columnNames: { date: header[0], amount: header[1], description: header[2] },
  dateFormat: "dd/mm/yyyy",
  invertSign: false,
  builtin: false,
  ...over,
});

describe("signatureOf", () => {
  it("signature_ignores_order_case_accents", () => {
    expect(signatureOf(["Data", "Descrição", "Valor"])).toBe(signatureOf(["valor", "DATA", "descricao"]));
  });

  it("signature_ignores_bom_and_empty_trailing_columns", () => {
    expect(signatureOf(["﻿Data", "Valor", ""])).toBe(signatureOf(["Data", "Valor"]));
  });
});

describe("built-in formats", () => {
  it("nubank_fixtures_match_builtins", async () => {
    const checking = fixture("nubank-checking.csv");
    const fc = matchFormat(checking.header, []);
    expect(fc?.id).toBe("nubank-checking");
    const pc = parseCsv(checking, fc!);
    const txc = await withSyntheticFitids(pc.transactions);
    expect(txc).toHaveLength(5);
    expect(txc[0]).toMatchObject({ fitid: "6a1f0c2e-0000-4000-8000-000000000001", amount: "5000.00", date: "2026-10-01" });
    expect(txc[1].amount).toBe("-120.50");

    const card = fixture("nubank-card.csv");
    const fk = matchFormat(card.header, []);
    expect(fk?.id).toBe("nubank-card");
    expect(fk?.kind).toBe("credit_card");
    const txk = await withSyntheticFitids(parseCsv(card, fk!).transactions);
    expect(txk).toHaveLength(6);
    expect(txk[0].amount).toBe("-23.45");
    expect(txk[3].amount).toBe("1500.00");
    expect(txk.every((t) => t.fitid?.startsWith("csv:"))).toBe(true);
  });

  it("builtins_are_nubank", () => {
    expect(BUILTIN_FORMATS.map((f) => f.id)).toEqual(["nubank-checking", "nubank-card"]);
    expect(BUILTIN_FORMATS.every((f) => f.builtin && f.bank === "nubank")).toBe(true);
  });
});

describe("matchFormat", () => {
  it("saved_format_wins_over_builtin", () => {
    const header = ["date", "title", "amount"];
    const mine = saved(header, { id: "my-card", columnNames: { date: "date", description: "title", amount: "amount" } });
    expect(matchFormat(header, [mine])?.id).toBe("my-card");
  });

  it("match_remaps_columns_to_file_order", () => {
    const mine = saved(["Data", "Valor", "Desc"]);
    const f = matchFormat(["Desc", "Data", "Valor"], [mine]);
    expect(f?.columns).toMatchObject({ date: 1, amount: 2, description: 0 });
  });

  // "Valor (R$)" and "Valor (US$)" both normalize to "valor": the saved
  // index is the only thing that tells them apart.
  it("ambiguous_names_keep_stored_index", () => {
    const header = ["Data", "Descrição", "Valor (R$)", "Valor (US$)"];
    const mine = saved(header, {
      columns: { date: 0, description: 1, amount: 3 },
      columnNames: { date: "Data", description: "Descrição", amount: "Valor (US$)" },
    });
    expect(matchFormat(header, [mine])?.columns.amount).toBe(3);
  });

  it("unknown_header_matches_nothing", () => {
    expect(matchFormat(["foo", "bar"], [])).toBeNull();
  });
});

describe("guessing", () => {
  it("guess_columns_pt_and_en", () => {
    expect(guessColumns(["Data", "Histórico", "Valor"])).toEqual({ date: 0, description: 1, amount: 2 });
    expect(guessColumns(["date", "title", "amount"])).toEqual({ date: 0, description: 1, amount: 2 });
    expect(guessColumns(["Data", "Lançamento", "Crédito", "Débito"])).toEqual({
      date: 0,
      description: 1,
      inflow: 2,
      outflow: 3,
    });
    expect(guessColumns(["Data", "Valor (R$)", "Identificador", "Descrição"])).toEqual({
      date: 0,
      amount: 1,
      bankId: 2,
      description: 3,
    });
  });

  it("guess_date_format", () => {
    expect(guessDateFormat(["31/10/2026"])).toBe("dd/mm/yyyy");
    expect(guessDateFormat(["2026-10-31"])).toBe("yyyy-mm-dd");
    expect(guessDateFormat(["10/31/2026"])).toBe("mm/dd/yyyy");
    expect(guessDateFormat(["01/02/2026"])).toBe("dd/mm/yyyy");
  });
});
