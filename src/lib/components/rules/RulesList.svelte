<script lang="ts">
  import { flip } from "svelte/animate";
  import Icon from "$lib/components/ui/Icon.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import SortHeader from "$lib/components/ui/SortHeader.svelte";
  import type { IconName } from "$lib/components/ui/icons";
  import { createSort, compareText, nullsLast } from "$lib/stores/sort.svelte";
  import { DUR, SNAP, reducedMotion } from "$lib/motion";
  import { locale } from "$lib/i18n/locale.svelte";
  import type { Category, RuleWithCount } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    rules: RuleWithCount[];
    categories: Category[];
    onEdit: (rule: RuleWithCount) => void;
    onDelete: (rule: RuleWithCount) => Promise<void>;
    /** Opens the transactions the rule reaches (its count is the button). */
    onViewMatches: (rule: RuleWithCount) => void;
    selectedId?: number | null;
    /** The search that produced `rules`; an empty result then says so,
     *  instead of "no rules yet". */
    query?: string;
  };

  let { rules, categories, onEdit, onDelete, onViewMatches, selectedId = null, query = "" }: Props = $props();

  function categoryName(id: number): string {
    return categories.find((c) => c.id === id)?.name ?? "?";
  }
  function categoryToken(id: number): string {
    return categories.find((c) => c.id === id)?.color_token ?? "--color-cat-outros";
  }

  /** What the first column shows, and therefore what it sorts by. Sorting by an
   *  invisible value would make the table look shuffled. */
  function label(r: RuleWithCount): string {
    return r.display_name ?? r.patterns[0] ?? "";
  }

  // ── Sorting ──────────────────────────────────────────────────────────────
  type SortKey = "pattern" | "category" | "due" | "priority" | "count";

  /** First click per column in its most useful direction: text starts A-Z, due
   *  date at day 1, and both numeric columns start highest (the priority that
   *  wins and the rules that catch the most). */
  const sort = createSort<SortKey>(
    { pattern: "asc", category: "asc", due: "asc", priority: "desc", count: "desc" },
    // Mirrors the backend's `ORDER BY priority DESC, created_at DESC`: the
    // table opens in exactly the order the data arrived.
    { key: "priority", dir: "desc" },
  );

  let sorted = $derived.by(() => {
    const sign = sort.sign;
    const code = locale.code;
    return [...rules].sort((a, b) => {
      let d = 0;
      switch (sort.key) {
        case "pattern":
          d = compareText(label(a), label(b), code);
          break;
        case "category":
          d = compareText(categoryName(a.category_id), categoryName(b.category_id), code);
          break;
        case "due": {
          // A rule with no due date goes last in both directions. When NEITHER
          // has one, `nullsLast` returns 0 and the decision falls to the id
          // tie-break below instead of leaving here with an unstable order.
          const empty = nullsLast(a.due_day, b.due_day);
          if (empty !== null && empty !== 0) return empty;
          d = empty === 0 ? 0 : (a.due_day ?? 0) - (b.due_day ?? 0);
          break;
        }
        case "priority":
          d = a.priority - b.priority;
          break;
        case "count":
          d = a.transaction_count - b.transaction_count;
          break;
      }
      // Stable id tie-break — two tied rules never swap places on their own
      // between renders.
      return d !== 0 ? sign * d : sign * (a.id - b.id);
    });
  });

  function hint(key: SortKey, col: string): string {
    return t(sort.next(key) === "asc" ? "rules.sort_asc" : "rules.sort_desc", { col });
  }

  /** Same limit as the transactions table: `animate:flip` measures every row
   *  on every change, and the search changes the list on each keystroke. */
  const FLIP_MAX_ROWS = 150;
  let animateRows = $derived(sorted.length <= FLIP_MAX_ROWS);

  function rowClass(r: RuleWithCount): string {
    return `row group border-t border-border-subtle first:border-t-0 cursor-default ${
      selectedId === r.id ? "bg-accent-soft hover:bg-accent-soft" : ""
    }`;
  }

  let flipParams = $derived(
    reducedMotion() ? { duration: 0 } : { duration: DUR.base, easing: SNAP },
  );

  /**
   * Column widths.
   *
   * The table is `table-fixed`. Without it the browser sizes by content, and a
   * long snippet — a whole statement line, exactly the case this screen must
   * support — pushes the first column without limit until it overflows the
   * card. With `table-fixed` these widths hold and the overflow ellipsizes.
   *
   * Deliberately generous: with `table-fixed` these values apply even in a wide
   * window (all slack goes to the snippet column), so trimming them to the
   * label's minimum would ellipsize the header all the time, not only when
   * space is short. Being generous costs only the snippet column, which has
   * slack.
   *
   * Fixed columns total 476px. With the table's `min-w`, the snippet column
   * never drops below ~180px, where it still says something.
   */
  const COLUMNS: Array<{
    key: SortKey;
    labelKey: string;
    align: "left" | "right";
    width: string;
    dense?: boolean;
    symbol?: IconName;
  }> = [
    { key: "pattern", labelKey: "rules.col_pattern", align: "left", width: "" },
    { key: "category", labelKey: "rules.col_category", align: "left", width: "w-[156px]" },
    { key: "count", labelKey: "rules.col_transactions", align: "right", width: "w-[124px]", dense: true },
    // Due date and priority become symbols: the two narrowest columns and the
    // ones that vary least per row, so spelling them out cost width the snippet
    // column uses better.
    {
      key: "due",
      labelKey: "rules.col_due",
      align: "right",
      width: "w-[76px]",
      dense: true,
      symbol: "calendar",
    },
    {
      key: "priority",
      labelKey: "rules.col_priority",
      align: "right",
      width: "w-[76px]",
      dense: true,
      symbol: "arrowUpNarrowWide",
    },
  ];
</script>

<!-- The row shows the first snippet; the rest become a counter. Listing them
     all would grow the column without saying more. -->
{#snippet cells(r: RuleWithCount)}
            <!-- `min-w-0` is what makes `truncate` work inside a table cell:
                 without it the box grows with the text and nothing ellipsizes.
                 The `title` gives back what the ellipsis hid. -->
            <td class="px-4 py-2 min-w-0" onclick={() => onEdit(r)}>
              {#if r.display_name}
                <div class="text-callout text-fg font-medium truncate" title={r.display_name}>
                  {r.display_name}
                </div>
                <div class="text-cap text-fg-subtle font-mono truncate" title={r.patterns.join(" · ")}>
                  {r.patterns[0] ?? ""}{@render extra(r.patterns.length)}
                </div>
              {:else}
                <div class="font-mono text-sub text-fg truncate" title={r.patterns.join(" · ")}>
                  {r.patterns[0] ?? ""}{@render extra(r.patterns.length)}
                </div>
              {/if}
            </td>
            <td class="px-4 py-2 min-w-0" onclick={() => onEdit(r)}>
              <span
                class="flex items-center gap-1.5 text-sub text-fg min-w-0"
                title={categoryName(r.category_id)}
              >
                <span
                  class="w-2 h-2 rounded-full shrink-0"
                  style="background: var({categoryToken(r.category_id)})"
                ></span>
                <span class="truncate">{categoryName(r.category_id)}</span>
              </span>
            </td>
            <!-- Zero is information, not absence: the rule exists and catches
                 nothing, so it is dimmed rather than turned into a dash. The
                 padding matches the header's (`dense`) to stay aligned. -->
            <td class="px-3 py-1.5 text-right text-sub tabular">
              <!-- The count is the way into the rule's transactions: the rest
                   of the row edits, this cell shows what the rule catches. -->
              {#if r.transaction_count === 0}
                <span class="px-1.5 text-fg-faint" title={t("rules.reach_none")}>0</span>
              {:else}
                <button
                  type="button"
                  onclick={() => onViewMatches(r)}
                  title={t("rules.reach_title")}
                  aria-label={t("rules.reach_open_label", { n: r.transaction_count, rule: label(r) })}
                  class="press inline-flex items-center gap-1 h-6 px-1.5 rounded-[var(--radius-sm)] text-fg-muted
                         hover:bg-hover hover:text-accent transition-colors duration-[var(--dur-fast)]"
                >
                  {r.transaction_count}
                  <Icon name="chevronRight" size={10} stroke={2.2} class="opacity-60" />
                </button>
              {/if}
            </td>
            <td
              class="px-3 py-2 text-right text-sub tabular text-fg-muted"
              onclick={() => onEdit(r)}
            >
              {r.due_day ? t("rules.due_day", { day: r.due_day }) : "—"}
            </td>
            <td
              class="px-3 py-2 text-right text-sub tabular text-fg-muted"
              onclick={() => onEdit(r)}
            >
              {r.priority}
            </td>
            <td class="px-3 py-2">
              <div
                class="flex gap-1 justify-end opacity-0 group-hover:opacity-100 focus-within:opacity-100
                       transition-opacity duration-[var(--dur-fast)]"
              >
                <button
                  type="button"
                  onclick={() => onDelete(r)}
                  title={t("rules.delete")}
                  aria-label={`${t("rules.delete")} ${label(r)}`}
                  class="press w-6 h-6 grid place-items-center rounded-[var(--radius-sm)] text-fg-muted
                         hover:bg-neg/12 hover:text-neg transition-colors duration-[var(--dur-fast)]"
                >
                  <Icon name="trash2" size={12.5} />
                </button>
              </div>
            </td>
{/snippet}

{#snippet extra(total: number)}
  {#if total > 1}
    <span
      class="ml-1.5 text-cap2 text-fg-faint font-sans tabular"
      title={t("rules.more_patterns_title", { n: total - 1 })}
    >
      {t("rules.more_patterns", { n: total - 1 })}
    </span>
  {/if}
{/snippet}

<div class="card overflow-hidden">
  {#if rules.length === 0 && query.trim() !== ""}
    <EmptyState icon="search" title={t("rules.search_empty_title")} description={t("rules.search_empty", { q: query.trim() })} />
  {:else if rules.length === 0}
    <EmptyState icon="wandSparkles" title={t("rules.empty_title")} description={t("rules.empty")} />
  {:else}
    <!-- When the window is too narrow for six columns, the table scrolls inside
         the card — never the page, and never clipped by the `overflow-hidden`
         that rounds the corners. -->
    <div class="overflow-x-auto">
      <table class="w-full table-fixed min-w-[660px]">
        <thead>
        <tr class="border-b border-border-subtle">
          {#each COLUMNS as c (c.key)}
            <th class="{c.width} p-0" aria-sort={sort.aria(c.key)}>
              <SortHeader
                label={t(c.labelKey)}
                align={c.align}
                dense={c.dense}
                symbol={c.symbol}
                active={sort.key === c.key}
                dir={sort.dir}
                hint={hint(c.key, t(c.labelKey))}
                onclick={() => sort.toggle(c.key)}
              />
            </th>
          {/each}
          <th class="px-3 py-2 w-[44px]"><span class="sr-only">{t("common.actions")}</span></th>
        </tr>
      </thead>
      <tbody>
        {#if animateRows}
          {#each sorted as r (r.id)}
            <!-- The whole row opens the edit panel, same gesture as the
                 transactions table. The count and the actions escape it. -->
            <tr animate:flip={flipParams} class={rowClass(r)} aria-selected={selectedId === r.id}>
              {@render cells(r)}
            </tr>
          {/each}
        {:else}
          {#each sorted as r (r.id)}
            <tr class={rowClass(r)} aria-selected={selectedId === r.id}>
              {@render cells(r)}
            </tr>
          {/each}
        {/if}
        </tbody>
      </table>
    </div>
  {/if}
</div>
