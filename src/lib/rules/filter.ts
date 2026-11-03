import type { Category, RuleWithCount } from "$lib/bindings";

/** Lower case, no accents: "Farmácia" and "FARMACIA" are the same search. */
function fold(s: string): string {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/**
 * The rules a search shows: any snippet, the display name or the category's
 * name contains the query. A query of digits also matches a tax id written
 * with its punctuation (`33967103` finds `33.967.103/0001-84`).
 */
export function filterRules(rules: RuleWithCount[], categories: Category[], query: string): RuleWithCount[] {
  const q = fold(query.trim());
  if (q === "") return rules;
  const digits = /^[\d./-]+$/.test(q) ? q.replace(/\D/g, "") : "";
  const names = new Map(categories.map((c) => [c.id, fold(c.name)]));
  return rules.filter((r) => {
    const texts = [...r.patterns, r.display_name ?? ""].map(fold);
    if (texts.some((s) => s.includes(q))) return true;
    if (digits && r.patterns.some((p) => p.replace(/\D/g, "").includes(digits))) return true;
    return (names.get(r.category_id) ?? "").includes(q);
  });
}
