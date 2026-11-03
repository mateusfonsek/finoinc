<script lang="ts">
  import { push } from "svelte-spa-router";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { Button } from "$lib/components/ui/button";
  import { locale } from "$lib/i18n/locale.svelte";
  import { rise } from "$lib/motion";
  import { suggestPatternFor } from "$lib/api/suggestions";
  import { deleteRuleWithCleanup, listRules, rulesForTransaction } from "$lib/api/rules";
  import { ruleWithPattern, suggestedPriority } from "$lib/rules/txRule";
  import type { Category, Rule, RuleWithCount, Transaction, TransactionRules } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    transaction: Transaction;
    categories: Category[];
    onCreateRule: (data: { pattern: string; categoryId: number; priority: number }) => Promise<void>;
    /** A rule was deleted here: categories may have changed under the list. */
    onRulesChanged: () => Promise<void>;
    /** Whether the mini-form is open — the panel's Esc closes it first. */
    formOpen?: boolean;
  };

  let { transaction, categories, onCreateRule, onRulesChanged, formOpen = $bindable(false) }: Props = $props();

  let info = $state<TransactionRules | null>(null);
  let loadError = $state<string | null>(null);
  let allRules = $state<Rule[]>([]);

  let pattern = $state("");
  let categoryId = $state<number | null>(null);
  let priority = $state(5);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // A different transaction in the same panel: its own rules, a closed form.
  $effect(() => {
    const id = transaction.id;
    formOpen = false;
    void load(id);
  });

  async function load(id: number) {
    loadError = null;
    try {
      const r = await rulesForTransaction(id);
      if (id === transaction.id) info = r;
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }

  let applied = $derived(info?.applied ?? null);
  let matching = $derived(info?.matching ?? []);
  /** Exactly this snippet already exists: creating would duplicate that rule. */
  let duplicate = $derived(ruleWithPattern(allRules, pattern));

  function categoryOf(id: number): Category | undefined {
    return categories.find((c) => c.id === id);
  }
  function labelOf(r: Pick<Rule, "display_name" | "patterns">): string {
    return r.display_name ?? r.patterns[0] ?? "";
  }

  async function openForm() {
    error = null;
    categoryId = transaction.category_id ?? null;
    priority = suggestedPriority(applied);
    const [suggested, rules] = await Promise.all([suggestPatternFor(transaction.description), listRules()]);
    pattern = suggested;
    allRules = rules;
    formOpen = true;
  }

  async function create() {
    error = null;
    const p = pattern.trim();
    if (!p) return void (error = t("rule_form.pattern_required"));
    if (categoryId === null) return void (error = t("rule_form.category_required"));
    if (duplicate) return;
    busy = true;
    try {
      await onCreateRule({ pattern: p, categoryId, priority });
      formOpen = false;
      await load(transaction.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function edit(r: Pick<Rule, "id">) {
    void push(`/rules?edit=${r.id}`);
  }

  async function remove(r: RuleWithCount) {
    const ok = await confirm(t("tx_rule.delete_confirm", { rule: labelOf(r) }), {
      title: t("rules.delete"),
      kind: "warning",
      okLabel: t("common.delete"),
      cancelLabel: t("common.cancel"),
    });
    if (!ok) return;
    busy = true;
    error = null;
    try {
      await deleteRuleWithCleanup(r.id);
      await onRulesChanged();
      await load(transaction.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#snippet ruleMeta(r: RuleWithCount)}
  {@const cat = categoryOf(r.category_id)}
  <div class="flex items-center gap-2 min-w-0">
    <span class="text-callout font-medium text-fg truncate" title={r.patterns.join(" · ")}>{labelOf(r)}</span>
    {#if cat}
      <span class="chip text-fg-muted shrink-0 ml-auto">
        <span class="w-2 h-2 rounded-full" style="background: var({cat.color_token ?? '--color-cat-outros'})"></span>
        {cat.name}
      </span>
    {/if}
  </div>
  <div class="text-cap text-fg-subtle tabular truncate">
    {#if r.display_name}<span class="font-mono">{r.patterns[0]}</span> · {/if}
    {r.patterns.length > 1 ? t("rules.more_patterns", { n: r.patterns.length - 1 }) + " · " : ""}
    {t("tx_rule.priority", { n: r.priority })} ·
    {r.transaction_count === 1
      ? t("tx_rule.reach_one", { n: r.transaction_count })
      : t("tx_rule.reach_many", { n: r.transaction_count })}
  </div>
{/snippet}

<section class="px-4 py-3 flex flex-col gap-2.5" aria-labelledby="tx-rule-title">
  <span id="tx-rule-title" class="section-title">{t("tx_rule.title")}</span>

  {#if loadError}
    <div class="text-foot text-neg">{loadError}</div>
  {:else if info === null}
    <div class="h-8"></div>
  {:else}
    {#if applied}
      <!-- What set this category, with its two actions right on it: the
           question "why is this here?" and its answer in one place. -->
      <div class="card-inset p-3 flex flex-col gap-1.5">
        <span class="text-cap font-semibold text-accent flex items-center gap-1">
          <Icon name="wandSparkles" size={11} stroke={2.2} />
          {t("tx_rule.applied")}
        </span>
        {@render ruleMeta(applied)}
        <div class="flex gap-1.5 pt-1">
          <Button variant="outline" size="sm" onclick={() => edit(applied!)} disabled={busy}>
            <Icon name="pencil" size={11} />
            {t("tx_rule.edit")}
          </Button>
          <Button variant="ghost" size="sm" onclick={() => remove(applied!)} disabled={busy} class="hover:text-neg">
            {t("common.delete")}
          </Button>
        </div>
      </div>
    {/if}

    {#if matching.length > 0}
      <div class="flex flex-col gap-1">
        <span class="text-foot text-fg-subtle">
          {applied ? t("tx_rule.also_matching") : t("tx_rule.matching")}
        </span>
        {#each matching as r (r.id)}
          <div class="flex items-center gap-2 rounded-[var(--radius-md)] px-2 py-1.5 hover:bg-hover">
            <div class="flex flex-col min-w-0 flex-1">{@render ruleMeta(r)}</div>
            <Button variant="ghost" size="icon-sm" onclick={() => edit(r)} aria-label={t("tx_rule.edit_named", { rule: labelOf(r) })} title={t("tx_rule.edit")}>
              <Icon name="pencil" size={11} />
            </Button>
          </div>
        {/each}
      </div>
    {/if}

    {#if !formOpen}
      <Button variant={applied || matching.length > 0 ? "ghost" : "outline"} onclick={openForm} class="w-full justify-center">
        <Icon name="plus" size={12} />
        {applied || matching.length > 0 ? t("tx_rule.create_another") : t("tx_notes.create_rule_cta")}
      </Button>
    {:else}
      <div class="card-inset p-3 flex flex-col gap-2.5" in:rise>
        <span class="section-title">{t("tx_notes.new_rule")}</span>
        {#if applied}
          <!-- Overriding is the usual reason for a second rule: say how. -->
          <p class="text-foot text-fg-muted leading-relaxed">
            {t("tx_rule.override_hint", { rule: labelOf(applied) })}
          </p>
        {/if}
        <label class="flex flex-col gap-1">
          <span class="text-foot text-fg-subtle">{t("rule_form.pattern_label")}</span>
          <input bind:value={pattern} class="field font-mono" title={t("tx_notes.pattern_title")} />
        </label>
        {#if duplicate}
          <div class="text-foot text-fg-muted flex items-start gap-1.5">
            <Icon name="circleAlert" size={12} stroke={2} class="mt-px text-neg shrink-0" />
            <span>
              {t("tx_rule.duplicate", { rule: labelOf(duplicate) })}
              <button type="button" class="text-accent hover:underline" onclick={() => edit(duplicate!)}>
                {t("tx_rule.duplicate_edit")}
              </button>
            </span>
          </div>
        {/if}
        <div class="grid grid-cols-[1fr_88px] gap-2">
          <label class="flex flex-col gap-1">
            <span class="text-foot text-fg-subtle">{t("rule_form.category")}</span>
            <select
              value={categoryId === null ? "" : String(categoryId)}
              onchange={(e) => {
                const v = (e.currentTarget as HTMLSelectElement).value;
                categoryId = v === "" ? null : Number(v);
              }}
              class="field"
            >
              <option value="">{t("rule_form.select_placeholder")}</option>
              {#each categories as c}
                <option value={String(c.id)}>{c.name}</option>
              {/each}
            </select>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-foot text-fg-subtle">{t("tx_rule.priority_label")}</span>
            <input type="number" min="0" bind:value={priority} class="field tabular" />
          </label>
        </div>
        {#if error}
          <div class="text-foot text-neg flex items-start gap-1.5">
            <Icon name="circleAlert" size={12} stroke={2} class="mt-px" />
            <span>{error}</span>
          </div>
        {/if}
        <div class="flex justify-end gap-2">
          <Button variant="ghost" onclick={() => (formOpen = false)} disabled={busy}>{t("common.cancel")}</Button>
          <Button onclick={create} disabled={busy || !!duplicate}>
            {busy ? t("tx_notes.creating") : t("tx_notes.create_apply")}
          </Button>
        </div>
      </div>
    {/if}
  {/if}
</section>
