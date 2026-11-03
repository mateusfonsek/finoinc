<script lang="ts">
  import { push } from "svelte-spa-router";
  import { flip } from "svelte/animate";
  import { formatMoney } from "$lib/format/money";
  import { locale } from "$lib/i18n/locale.svelte";
  import { Button } from "$lib/components/ui/button";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import SortHeader from "$lib/components/ui/SortHeader.svelte";
  import { createSort } from "$lib/stores/sort.svelte";
  import { DUR, SNAP, reducedMotion } from "$lib/motion";
  import CategoryPicker from "./CategoryPicker.svelte";
  import type { Category, Transaction } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    transactions: Transaction[];
    categories: Category[];
    onCategoryChange: (transactionId: number, categoryId: number | null) => Promise<void>;
    onCategoryCreate: (name: string) => Promise<Category>;
    onRowClick?: (transaction: Transaction) => void;
    selectedId?: number | null;
  };

  let {
    transactions,
    categories,
    onCategoryChange,
    onCategoryCreate,
    onRowClick,
    selectedId,
  }: Props = $props();

  /** "2026-08-14" becomes "14 ago 2026". A short spelled-out date reads faster
   *  than ISO and takes nearly the same space. */
  function fmtDate(iso: string): string {
    const mo = Number(iso.slice(5, 7)) - 1;
    return `${iso.slice(8, 10)} ${(locale.monthsShort[mo] ?? "").toLowerCase()} ${iso.slice(0, 4)}`;
  }

  // ── Sorting ──────────────────────────────────────────────────────────────
  // Reorders on the client: the whole list is already in memory, so the click
  // responds in the same frame — no backend round-trip between finger and
  // result.
  type SortKey = "date" | "amount";

  /** First click per column in its most useful direction: date starts newest;
   *  amount starts with the largest outflow (expenses are negative, so the most
   *  negative comes first ascending). */
  const sort = createSort<SortKey>({ date: "desc", amount: "asc" }, { key: "date", dir: "desc" });

  let sorted = $derived.by(() => {
    const sign = sort.sign;
    // Copy: `transactions` is a prop, sorting in place would mutate the parent.
    return [...transactions].sort((a, b) => {
      let d = 0;
      if (sort.key === "date") {
        d = a.date < b.date ? -1 : a.date > b.date ? 1 : 0;
      } else {
        d = Number(a.amount) - Number(b.amount);
      }
      // Stable id tie-break in the same direction — two transactions on the
      // same day (or of the same amount) never swap places between renders.
      return d !== 0 ? sign * d : sign * (a.id - b.id);
    });
  });

  /** Describes the state the click WILL produce — the title anticipates the
   *  result rather than describing the current one. */
  function sortHint(key: SortKey, label: string): string {
    const next = sort.next(key);
    if (key === "date") {
      return t(next === "asc" ? "tx_table.sort_date_asc" : "tx_table.sort_date_desc");
    }
    return t(next === "asc" ? "tx_table.sort_asc" : "tx_table.sort_desc", { col: label });
  }

  /** Rows glide to their new place on a sort — but `animate:flip` measures
   *  and animates every row on every change, which on a few thousand rows
   *  froze typing in the search for seconds (WebView2 on Windows most of
   *  all). Past this many rows the table just updates. */
  const FLIP_MAX_ROWS = 150;
  let animateRows = $derived(sorted.length <= FLIP_MAX_ROWS);

  function rowClass(t: Transaction): string {
    return `row border-t border-border-subtle first:border-t-0 cursor-default ${
      selectedId === t.id ? "bg-accent-soft hover:bg-accent-soft" : ""
    }`;
  }

  let flipParams = $derived(
    reducedMotion() ? { duration: 0 } : { duration: DUR.base, easing: SNAP },
  );
</script>

{#snippet cells(t: Transaction)}
            <td class="px-4 py-2 text-sub text-fg-subtle tabular whitespace-nowrap" onclick={() => onRowClick?.(t)}>
              {fmtDate(t.date)}
            </td>
            <td class="px-4 py-2 text-callout text-fg min-w-0" onclick={() => onRowClick?.(t)}>
              <!-- Two lines and stop: a statement description can run 200
                   characters, and an over-tall row breaks the scan of its
                   neighbours. The `title` gives back the rest. -->
              <span class="line-clamp-2 break-words" title={t.description}>{t.description}</span>
            </td>
            <td class="px-4 py-2">
              <CategoryPicker
                {categories}
                currentId={t.category_id}
                onselect={(catId) => onCategoryChange(t.id, catId)}
                oncreate={onCategoryCreate}
              />
            </td>
            <td
              class="px-4 py-2 text-right text-callout tabular font-medium whitespace-nowrap
                     {Number(t.amount) >= 0 ? 'text-pos' : 'text-fg'}"
              onclick={() => onRowClick?.(t)}
            >
              {formatMoney(t.amount)}
            </td>
{/snippet}

<div class="card overflow-hidden">
  {#if transactions.length === 0}
    <EmptyState icon="inbox" title={t("tx_table.empty")} description={t("tx_table.empty_desc")}>
      {#snippet action()}
        <Button variant="outline" onclick={() => push("/import")}>
          {t("tx_table.import_link")}
        </Button>
      {/snippet}
    </EmptyState>
  {:else}
    <!-- Same treatment as the Rules table: `table-fixed` so a long description
         ellipsizes instead of pushing the column, and horizontal scrolling
         contained in the card instead of leaking to the page. -->
    <div class="overflow-x-auto">
      <table class="w-full table-fixed min-w-[560px]">
        <thead>
        <tr class="border-b border-border-subtle">
          <th class="w-[124px] p-0" aria-sort={sort.aria("date")}>
            <SortHeader
              label={t("tx_table.date")}
              active={sort.key === "date"}
              dir={sort.dir}
              hint={sortHint("date", t("tx_table.date"))}
              onclick={() => sort.toggle("date")}
            />
          </th>
          <th class="col-head text-left px-4 py-2">{t("tx_table.description")}</th>
          <th class="col-head text-left px-4 py-2 w-[184px]">{t("tx_table.category")}</th>
          <th class="w-[136px] p-0" aria-sort={sort.aria("amount")}>
            <SortHeader
              label={t("tx_table.amount")}
              align="right"
              active={sort.key === "amount"}
              dir={sort.dir}
              hint={sortHint("amount", t("tx_table.amount"))}
              onclick={() => sort.toggle("amount")}
            />
          </th>
        </tr>
      </thead>
      <tbody>
        {#if animateRows}
          {#each sorted as t (t.id)}
            <!-- The whole row is the click target (except the category cell,
                 which has its own control) — a large target, as in macOS lists. -->
            <tr animate:flip={flipParams} class={rowClass(t)} aria-selected={selectedId === t.id}>
              {@render cells(t)}
            </tr>
          {/each}
        {:else}
          {#each sorted as t (t.id)}
            <tr class={rowClass(t)} aria-selected={selectedId === t.id}>
              {@render cells(t)}
            </tr>
          {/each}
        {/if}
        </tbody>
      </table>
    </div>
  {/if}
</div>
