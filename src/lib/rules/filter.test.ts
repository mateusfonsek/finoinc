import { describe, expect, it } from "vitest";
import type { Category, RuleWithCount } from "$lib/bindings";
import { filterRules } from "./filter";

const cat = (id: number, name: string) => ({ id, name, color_token: null, kind: "expense" }) as unknown as Category;
const rule = (id: number, patterns: string[], category_id: number, display_name: string | null = null) =>
  ({ id, patterns, category_id, display_name, priority: 0, due_day: null, pay_lead_months: 0, created_at: "", transaction_count: 0 }) as unknown as RuleWithCount;

const categories = [cat(1, "Saúde"), cat(2, "Mercado")];
const rules = [
  rule(1, ["FARMÁCIA SÃO JOÃO", "DROGASIL"], 1),
  rule(2, ["PAO DE ACUCAR"], 2, "Pão de Açúcar"),
  rule(3, ["33.967.103/0001-84"], 2, "Demerge"),
];

describe("filterRules", () => {
  it("empty_query_keeps_everything", () => {
    expect(filterRules(rules, categories, "  ")).toEqual(rules);
  });

  it("matches_any_pattern_ignoring_case_and_accents", () => {
    expect(filterRules(rules, categories, "farmacia").map((r) => r.id)).toEqual([1]);
    expect(filterRules(rules, categories, "drogasil").map((r) => r.id)).toEqual([1]);
  });

  it("matches_display_name", () => {
    expect(filterRules(rules, categories, "demerge").map((r) => r.id)).toEqual([3]);
  });

  it("matches_category_name", () => {
    expect(filterRules(rules, categories, "saude").map((r) => r.id)).toEqual([1]);
    expect(filterRules(rules, categories, "mercado").map((r) => r.id)).toEqual([2, 3]);
  });

  it("matches_tax_id_digits_without_punctuation", () => {
    expect(filterRules(rules, categories, "33967103").map((r) => r.id)).toEqual([3]);
  });
});
