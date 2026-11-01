<script lang="ts">
  import { locale } from "$lib/i18n/locale.svelte";

  const t = locale.t;
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { MCP_DATA_CHANGED_EVENT } from "$lib/api/mcp";
  import Page from "$lib/components/ui/Page.svelte";
  import Loading from "$lib/components/ui/Loading.svelte";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import TxTable from "$lib/components/transactions/TxTable.svelte";
  import TxFilterBar from "$lib/components/transactions/TxFilterBar.svelte";
  import TxNotesPanel from "$lib/components/transactions/TxNotesPanel.svelte";
  import { filters } from "$lib/stores/filters.svelte";
  import { listCategories, createCategory } from "$lib/api/categories";
  import { createRule, applyRulesToUncategorized } from "$lib/api/rules";
  import {
    listTransactions,
    updateTransactionCategory,
    updateTransactionNotes,
  } from "$lib/api/transactions";
  import type { Category, Transaction } from "$lib/bindings";
  import { latestOnly } from "$lib/latest";

  let transactions = $state<Transaction[]>([]);
  let categories = $state<Category[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selectedTx = $state<Transaction | null>(null);

  let searchInputRef = $state<HTMLInputElement | null>(null);

  // Filters change faster than the backend answers (one query per keystroke):
  // only the newest reply may replace the list.
  const latest = latestOnly();

  async function refresh() {
    try {
      const reply = await latest(() =>
        listTransactions({
          account_id: null,
          month: filters.month,
          category_id: filters.categoryId,
          q: filters.q === "" ? null : filters.q,
          limit: null,
        }),
      );
      if (!reply.current) return;
      transactions = reply.value;
      if (selectedTx) {
        const fresh = transactions.find((t) => t.id === selectedTx?.id);
        selectedTx = fresh ?? null;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(async () => {
    try {
      categories = await listCategories();
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  // An MCP write tool (categorize, settle) touched what this screen lists —
  // re-fetch in place rather than reload the whole app.
  onMount(() => {
    const unlisten = listen(MCP_DATA_CHANGED_EVENT, () => void refresh());
    return () => void unlisten.then((u) => u());
  });

  async function onCategoryChange(transactionId: number, categoryId: number | null) {
    await updateTransactionCategory(transactionId, categoryId);
    transactions = transactions.map((t) =>
      t.id === transactionId ? { ...t, category_id: categoryId } : t,
    );
    if (filters.categoryId !== null && filters.categoryId !== categoryId) {
      await refresh();
    }
  }

  async function onCategoryCreate(name: string): Promise<Category> {
    const created = await createCategory({
      name,
      color_token: "--color-cat-outros",
      kind: "expense",
    });
    categories = [...categories, created];
    return created;
  }

  async function onMonthChange(m: string | null) {
    filters.month = m;
    await refresh();
  }
  async function onCategoryFilterChange(id: number | null) {
    filters.categoryId = id;
    await refresh();
  }

  /** Typing shows at once; the query waits for a pause in it, so a word is
   *  one round trip and one table update, not one per letter. Clearing the
   *  field answers at once. */
  const SEARCH_PAUSE_MS = 200;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  function onQueryChange(v: string) {
    filters.q = v;
    clearTimeout(searchTimer);
    if (v === "") void refresh();
    else searchTimer = setTimeout(() => void refresh(), SEARCH_PAUSE_MS);
  }

  onMount(() => () => clearTimeout(searchTimer));

  function onRowClick(t: Transaction) {
    selectedTx = t;
  }
  function closePanel() {
    selectedTx = null;
  }
  async function onSaveNotes(transactionId: number, notes: string | null) {
    await updateTransactionNotes(transactionId, notes);
    transactions = transactions.map((t) =>
      t.id === transactionId ? { ...t, notes } : t,
    );
  }
  async function onCreateRule(data: { pattern: string; categoryId: number }) {
    await createRule({
      patterns: [data.pattern],
      category_id: data.categoryId,
      priority: 5,
      due_day: null,
    });
    await applyRulesToUncategorized(null);
    await refresh();
  }
</script>

<Page title={t("nav.transactions")}>
  {#snippet toolbar()}
    <span class="text-sub text-fg-subtle tabular">
      {transactions.length === 1
        ? t("transactions_page.count_one", { n: transactions.length })
        : t("transactions_page.count_many", { n: transactions.length })}
    </span>
  {/snippet}

  <TxFilterBar
    {categories}
    month={filters.month}
    categoryId={filters.categoryId}
    q={filters.q}
    {onMonthChange}
    onCategoryChange={onCategoryFilterChange}
    {onQueryChange}
    bind:searchInputRef
  />

  {#if loading}
    <Loading />
  {:else if error}
    <ErrorNote message={error} />
  {:else}
    <TxTable
      {transactions}
      {categories}
      {onCategoryChange}
      {onCategoryCreate}
      {onRowClick}
      selectedId={selectedTx?.id ?? null}
    />
  {/if}
</Page>

{#if selectedTx}
  <TxNotesPanel
    transaction={selectedTx}
    {categories}
    onClose={closePanel}
    onSave={onSaveNotes}
    {onCreateRule}
  />
{/if}
