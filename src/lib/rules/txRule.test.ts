import { describe, expect, it } from "vitest";
import type { Rule, RuleWithCount } from "$lib/bindings";
import { DEFAULT_PRIORITY, ruleWithPattern, suggestedPriority } from "./txRule";

const rule = (id: number, patterns: string[], priority = 5) =>
  ({ id, patterns, priority, category_id: 1, due_day: null, display_name: null, created_at: "", pay_lead_months: 0 }) as Rule;

describe("suggestedPriority", () => {
  it("default_without_a_rule_to_beat", () => {
    expect(suggestedPriority(null)).toBe(DEFAULT_PRIORITY);
  });

  // The new rule exists to win over the one that categorized this
  // transaction: one step above it.
  it("one_above_the_applied_rule", () => {
    expect(suggestedPriority({ priority: 7 } as RuleWithCount)).toBe(8);
    expect(suggestedPriority({ priority: 1 } as RuleWithCount)).toBe(DEFAULT_PRIORITY);
  });
});

describe("ruleWithPattern", () => {
  const rules = [rule(1, ["PAO DE ACUCAR", "EXTRA"]), rule(2, ["UBER"])];

  it("finds_an_existing_snippet_ignoring_case_and_spaces", () => {
    expect(ruleWithPattern(rules, "  pao de acucar ")?.id).toBe(1);
    expect(ruleWithPattern(rules, "extra")?.id).toBe(1);
  });

  it("a_new_snippet_is_not_a_duplicate", () => {
    expect(ruleWithPattern(rules, "PAO DE ACUCAR LOJA 12")).toBeNull();
    expect(ruleWithPattern(rules, "   ")).toBeNull();
  });
});
