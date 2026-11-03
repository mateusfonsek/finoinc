import type { Rule, RuleWithCount } from "$lib/bindings";

/** Priority of a rule created from a transaction when no rule covers it. */
export const DEFAULT_PRIORITY = 5;

/** A rule made to override the one that categorized the transaction must
 *  outrank it; otherwise the usual default. */
export function suggestedPriority(applied: Pick<RuleWithCount, "priority"> | null): number {
  return applied ? Math.max(DEFAULT_PRIORITY, applied.priority + 1) : DEFAULT_PRIORITY;
}

/** The rule that already has this exact snippet — creating another one would
 *  be a duplicate. Case and surrounding spaces don't count. */
export function ruleWithPattern<R extends Pick<Rule, "patterns">>(rules: R[], pattern: string): R | null {
  const p = pattern.trim().toLocaleLowerCase();
  if (p === "") return null;
  return rules.find((r) => r.patterns.some((x) => x.trim().toLocaleLowerCase() === p)) ?? null;
}
