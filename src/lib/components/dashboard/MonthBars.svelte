<script lang="ts">
  import { locale } from "$lib/i18n/locale.svelte";
  import { formatMoney } from "$lib/format/money";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import type { MonthSummary } from "$lib/bindings";

  const t = locale.t;

  type Props = { months: MonthSummary[] };
  let { months }: Props = $props();

  let hovered = $state<string | null>(null);

  function shortLabel(yyyymm: string): string {
    const [, mo] = yyyymm.split("-");
    return locale.monthsShort[Number(mo) - 1] ?? mo;
  }

  function yearLabel(yyyymm: string): string {
    return yyyymm.slice(2, 4);
  }

  /** Marks the year boundary — without it, twelve month labels never say when
   *  the year changed. */
  function isYearStart(i: number): boolean {
    if (i === 0) return true;
    return months[i].month.slice(0, 4) !== months[i - 1].month.slice(0, 4);
  }

  let maxValue = $derived(
    months.reduce((acc, m) => {
      const i = Number(m.income);
      const e = Number(m.expense);
      return Math.max(acc, isFinite(i) ? i : 0, isFinite(e) ? e : 0);
    }, 0),
  );

  function pct(amountStr: string): number {
    if (maxValue <= 0) return 0;
    const v = Number(amountStr);
    if (!isFinite(v) || v <= 0) return 0;
    return Math.min(100, (v / maxValue) * 100);
  }
</script>

{#if maxValue <= 0}
  <EmptyState icon="trendingUp" title={t("dashboard.no_data_12m")} compact />
{:else}
  <div class="flex flex-col gap-2">
    <div class="grid grid-cols-12 gap-1.5 items-end h-[132px]">
      {#each months as m, i (m.month)}
        <!-- Two bars per month side by side: inflow and outflow comparable at a
             glance, rather than stacked (which only shows the total). -->
        <div
          role="presentation"
          onmouseenter={() => (hovered = m.month)}
          onmouseleave={() => (hovered = null)}
          class="flex flex-col items-center gap-1.5 h-full"
          title={`${m.month} · ${t("dashboard.legend_inflows")}: ${formatMoney(m.income)} · ${t("dashboard.legend_outflows")}: ${formatMoney(m.expense)}`}
        >
          <div class="w-full flex-1 flex items-end justify-center gap-[3px]">
            <span
              class="flex-1 max-w-[13px] rounded-t-[3px] bg-pos transition-opacity duration-[var(--dur-fast)]"
              style="height: {pct(m.income)}%; opacity: {hovered && hovered !== m.month ? 0.35 : 0.95}"
            ></span>
            <span
              class="flex-1 max-w-[13px] rounded-t-[3px] bg-neg transition-opacity duration-[var(--dur-fast)]"
              style="height: {pct(m.expense)}%; opacity: {hovered && hovered !== m.month ? 0.25 : 0.7}"
            ></span>
          </div>
          <span
            class="text-cap2 tabular transition-colors duration-[var(--dur-fast)]
                   {hovered === m.month ? 'text-fg' : 'text-fg-subtle'}"
          >
            {shortLabel(m.month)}{#if isYearStart(i)}<span class="text-fg-subtle"
                >’{yearLabel(m.month)}</span
              >{/if}
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}
