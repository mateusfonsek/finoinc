import { describe, expect, it } from "vitest";
import { signatureOf } from "./formats";
import { draftToFormat, mappingDraft, previewRows, validateDraft } from "./mapping";
import type { CsvTable } from "./read";

const table: CsvTable = {
  header: ["Data", "Histórico", "Valor", "Saldo"],
  preamble: 0,
  rows: [
    ["01/10/2026", "PIX JOAO", "-10,00", "90,00"],
    ["02/10/2026", "SALARIO", "100,00", "190,00"],
    ["03/10/2026", "PADARIA", "-5,50", "184,50"],
    ["04/10/2026", "MERCADO", "-20,00", "164,50"],
  ],
};

describe("mappingDraft", () => {
  it("draft_prefills_from_guess", () => {
    const d = mappingDraft(table, "extrato-inter.csv");
    expect(d.name).toBe("extrato-inter");
    expect(d.roles).toEqual(["date", "description", "amount", "ignore"]);
    expect(d.dateFormat).toBe("dd/mm/yyyy");
    expect(d.kind).toBe("checking");
    expect(d.invertSign).toBe(false);
  });
});

describe("validateDraft", () => {
  it("validate_requires_date_description_amount", () => {
    const d = { ...mappingDraft(table, "x.csv"), roles: ["date", "ignore", "ignore", "ignore"] as const };
    const codes = validateDraft({ ...d, roles: [...d.roles] }, table).map((e) => e.message);
    expect(codes).toContain("import.mapping.err_need_description");
    expect(codes).toContain("import.mapping.err_need_amount");
  });

  it("validate_rejects_amount_with_inflow", () => {
    const d = mappingDraft(table, "x.csv");
    const errs = validateDraft({ ...d, roles: ["date", "description", "amount", "inflow"] }, table);
    expect(errs).toContainEqual({ column: -1, message: "import.mapping.err_amount_or_split" });
  });

  it("validate_rejects_repeated_role", () => {
    const d = mappingDraft(table, "x.csv");
    const errs = validateDraft({ ...d, roles: ["date", "description", "amount", "amount"] }, table);
    expect(errs).toContainEqual({ column: 3, message: "import.mapping.err_repeated" });
  });

  it("validate_flags_column_with_no_dates", () => {
    const d = mappingDraft(table, "x.csv");
    const errs = validateDraft({ ...d, roles: ["description", "date", "amount", "ignore"] }, table);
    expect(errs).toContainEqual({ column: 1, message: "import.mapping.err_no_dates" });
  });

  it("valid_draft_has_no_errors", () => {
    expect(validateDraft(mappingDraft(table, "x.csv"), table)).toEqual([]);
  });
});

describe("previewRows", () => {
  it("preview_updates_with_invert_sign", () => {
    const d = mappingDraft(table, "x.csv");
    expect(previewRows(d, table).map((t) => t.amount)).toEqual(["-10.00", "100.00", "-5.50"]);
    expect(previewRows({ ...d, invertSign: true }, table).map((t) => t.amount)).toEqual(["10.00", "-100.00", "5.50"]);
  });

  it("preview_of_invalid_draft_is_empty", () => {
    const d = mappingDraft(table, "x.csv");
    expect(previewRows({ ...d, roles: ["ignore", "ignore", "ignore", "ignore"] }, table)).toEqual([]);
  });
});

describe("draftToFormat", () => {
  it("draft_to_format_signature_matches_table", () => {
    const f = draftToFormat(mappingDraft(table, "Inter.csv"), table);
    expect(f.signature).toBe(signatureOf(table.header));
    expect(f.columns).toEqual({ date: 0, description: 1, amount: 2 });
    expect(f.columnNames).toEqual({ date: "Data", description: "Histórico", amount: "Valor" });
    expect(f.builtin).toBe(false);
    expect(f.name).toBe("Inter");
    expect(f.id).toMatch(/[0-9a-f-]{36}/);
  });
});
