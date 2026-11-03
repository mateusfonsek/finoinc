<script lang="ts">
  import { locale } from "$lib/i18n/locale.svelte";

  const t = locale.t;
  import { onMount } from "svelte";
  import { replace, router } from "svelte-spa-router";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { MCP_DATA_CHANGED_EVENT } from "$lib/api/mcp";
  import { Button } from "$lib/components/ui/button";
  import Page from "$lib/components/ui/Page.svelte";
  import Loading from "$lib/components/ui/Loading.svelte";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import RuleApplyDialog from "$lib/components/rules/RuleApplyDialog.svelte";
  import RuleMatchesDialog from "$lib/components/rules/RuleMatchesDialog.svelte";
  import RuleForm from "$lib/components/rules/RuleForm.svelte";
  import RulePanel from "$lib/components/rules/RulePanel.svelte";
  import RulesList from "$lib/components/rules/RulesList.svelte";
  import SearchBox from "$lib/components/shell/SearchBox.svelte";
  import { filterRules } from "$lib/rules/filter";
  import { listCategories } from "$lib/api/categories";
  import {
    listRulesWithCount,
    createRule,
    updateRule,
    deleteRule,
    previewRuleApplication,
    applyRuleChoices,
  } from "$lib/api/rules";
  import type { Category, RuleChoice, RulePreviewRow, RuleWithCount } from "$lib/bindings";

  let rules = $state<RuleWithCount[]>([]);
  let categories = $state<Category[]>([]);
  let editing = $state<RuleWithCount | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let applyMsg = $state<string | null>(null);
  let applying = $state(false);
  let previewRows = $state<RulePreviewRow[]>([]);
  let previewOpen = $state(false);
  /** The rule whose transaction list is open. The edit panel stays mounted
   *  behind, so closing here returns to the rule being edited. */
  let matchesFor = $state<RuleWithCount | null>(null);
  /** Filters on the client: the whole list is in memory, so each keystroke
   *  answers in the same frame. */
  let query = $state("");
  let shown = $derived(filterRules(rules, categories, query));

  async function refresh() {
    rules = await listRulesWithCount();
  }

  // `#/rules?edit=12` — from a transaction's panel — opens that rule's
  // editor once the list is in. The parameter is dropped right after, so
  // closing the panel and going back does not reopen it.
  $effect(() => {
    const id = Number(new URLSearchParams(router.querystring ?? "").get("edit"));
    if (!id || loading) return;
    const found = rules.find((r) => r.id === id);
    if (found) editing = found;
    void replace("/rules");
  });

  onMount(async () => {
    try {
      [categories, rules] = await Promise.all([listCategories(), listRulesWithCount()]);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  // `create_rule` through MCP adds a rule behind this screen's back —
  // re-fetch the list in place rather than reload the whole app.
  onMount(() => {
    const unlisten = listen(MCP_DATA_CHANGED_EVENT, () => void refresh());
    return () => void unlisten.then((u) => u());
  });

  async function onCreate(data: {
    patterns: string[];
    categoryId: number;
    priority: number;
    dueDay: number | null;
    payLeadMonths: number;
  }) {
    await createRule({
      patterns: data.patterns,
      category_id: data.categoryId,
      priority: data.priority,
      due_day: data.dueDay,
      pay_lead_months: data.payLeadMonths,
    });
    await refresh();
  }

  async function onUpdate(
    ruleId: number,
    data: {
      patterns: string[];
      categoryId: number;
      priority: number;
      dueDay: number | null;
      displayName: string | null;
      payLeadMonths: number;
    },
  ) {
    await updateRule(ruleId, {
      patterns: data.patterns,
      category_id: data.categoryId,
      priority: data.priority,
      due_day: data.dueDay,
      display_name: data.displayName,
      pay_lead_months: data.payLeadMonths,
    });
    await refresh();
  }

  /** Native macOS alert — deleting a rule uncategorizes transactions. */
  async function onDelete(rule: RuleWithCount) {
    const label = rule.display_name ?? rule.patterns[0] ?? "";
    const ok = await confirm(t("rules_page.delete_confirm", { pattern: label }), {
      title: t("rules.delete"),
      kind: "warning",
      okLabel: t("common.delete"),
      cancelLabel: t("common.cancel"),
    });
    if (!ok) return;
    await deleteRule(rule.id);
    await refresh();
  }

  /**
   * "Review and apply" writes nothing: it queries what WOULD change and opens
   * the review. The user's choice in there is what writes.
   *
   * With nothing to change, no dialog opens — a modal that only says "nothing to
   * do" charges a click to dismiss and delivers nothing. The answer goes in the
   * usual banner, next to the button that caused it.
   */
  async function onApply() {
    applying = true;
    applyMsg = null;
    error = null;
    try {
      const rows = await previewRuleApplication(null);
      if (rows.length === 0) {
        applyMsg = t("rule_apply.nothing_to_do");
        return;
      }
      previewRows = rows;
      previewOpen = true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      applying = false;
    }
  }

  async function onConfirmApply(choices: RuleChoice[]) {
    const n = await applyRuleChoices(choices);
    previewOpen = false;
    previewRows = [];
    applyMsg =
      n === 1 ? t("rule_apply.applied_one") : t("rule_apply.applied_many", { n });
    // The table's reach counts do not change (reach is category-independent),
    // but the list is cheap and this keeps nothing stale on screen.
    await refresh();
  }
</script>

<!-- `wide`: six columns do not fit the default column without squeezing the
     snippet, which is this screen's primary information. -->
<Page title={t("nav.rules")} subtitle={t("rules_page.desc")} width="wide">
  {#snippet toolbar()}
    <SearchBox
      value={query}
      onInput={(v) => (query = v)}
      placeholder={t("rules_page.search_placeholder")}
    />
    <Button variant="outline" onclick={onApply} disabled={applying || rules.length === 0}>
      <Icon name="rotateCw" size={12.5} class={applying ? "animate-spin" : ""} />
      {applying ? t("rules_page.applying") : t("rules_page.apply")}
    </Button>
  {/snippet}

  {#if loading}
    <Loading />
  {:else}
    {#if error}
      <ErrorNote message={error} />
    {/if}

    <!-- The apply result sits next to the button that caused it, not in a
         generic banner at the top. -->
    {#if applyMsg}
      <ErrorNote message={applyMsg} tone="success" />
    {/if}

    <!-- The page form creates; editing happens in the side panel, so "new"
         never changes identity halfway through. -->
    <RuleForm {categories} onSave={onCreate} />

    <RulesList
      rules={shown}
      {categories}
      {query}
      onEdit={(r) => (editing = r)}
      onViewMatches={(r) => (matchesFor = r)}
      {onDelete}
      selectedId={editing?.id ?? null}
    />
  {/if}
</Page>

{#if previewOpen}
  <RuleApplyDialog
    rows={previewRows}
    {categories}
    onClose={() => (previewOpen = false)}
    onApply={onConfirmApply}
  />
{/if}

{#if editing}
  <RulePanel
    rule={editing}
    {categories}
    onClose={() => (editing = null)}
    onSave={onUpdate}
    onViewMatches={() => (matchesFor = editing)}
    blocked={matchesFor !== null}
  />
{/if}

{#if matchesFor}
  <RuleMatchesDialog
    rule={matchesFor}
    {categories}
    onClose={() => (matchesFor = null)}
  />
{/if}
