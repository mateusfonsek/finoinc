<script lang="ts">
  import { formatMoney } from "$lib/format/money";
  import { txKeyString } from "$lib/api/transactions";
  import { locale } from "$lib/i18n/locale.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { ParsedTransaction } from "$lib/statement/types";
  import type { ReversalInfo, ReversalRole } from "$lib/ofx/reversals";

  const tr = locale.t;

  type Props = {
    transactions: ParsedTransaction[];
    /** Composite `fitid|date|amount` keys already present in the DB. */
    duplicateKeys: Set<string>;
    reversalMap?: Map<string, ReversalInfo>;
    selected: Set<string>;
    ontoggle: (fitid: string) => void;
    ontoggleAll: (checked: boolean) => void;
  };

  let {
    transactions,
    duplicateKeys,
    reversalMap = new Map(),
    selected,
    ontoggle,
    ontoggleAll,
  }: Props = $props();

  let allChecked = $derived(
    transactions.length > 0 && transactions.every((t) => !t.fitid || selected.has(t.fitid)),
  );

  function reversalLabel(role: ReversalRole): string {
    return tr("import.role_" + role);
  }

  function reversalTooltip(role: ReversalRole): string {
    return tr("import.reversal_" + role);
  }

  function fmtDate(iso: string): string {
    const mo = Number(iso.slice(5, 7)) - 1;
    return `${iso.slice(8, 10)} ${(locale.monthsShort[mo] ?? "").toLowerCase()}`;
  }
</script>

<div class="card overflow-hidden">
  <table class="w-full">
    <thead>
      <tr class="border-b border-border-subtle">
        <th class="text-left pl-4 pr-2 py-2 w-9">
          <input
            type="checkbox"
            checked={allChecked}
            onchange={(e) => ontoggleAll((e.currentTarget as HTMLInputElement).checked)}
            aria-label={tr("import.toggle_all")}
            class="accent-[var(--color-accent)]"
          />
        </th>
        <th class="col-head text-left px-3 py-2 w-[76px]">{tr("tx_table.date")}</th>
        <th class="col-head text-left px-3 py-2">{tr("tx_table.description")}</th>
        <th class="col-head text-right px-4 py-2 w-[124px]">{tr("tx_table.amount")}</th>
      </tr>
    </thead>
    <tbody>
      {#each transactions as t (t.fitid ?? `${t.date}-${t.amount}-${t.description}`)}
        {@const isDup = !!(t.fitid && duplicateKeys.has(txKeyString({ ofx_fitid: t.fitid, date: t.date, amount: t.amount })))}
        {@const isSel = !!(t.fitid && selected.has(t.fitid))}
        {@const rev = t.fitid ? reversalMap.get(t.fitid) : undefined}
        <!-- A reversal gets a yellow bar on the leading edge; a duplicate fades.
             Two distinct states, tellable apart without relying on colour
             alone. -->
        <tr
          class="row border-t border-border-subtle first:border-t-0 {isDup ? 'opacity-50' : ''}"
          style={rev
            ? "box-shadow: inset 3px 0 0 var(--color-cat-amarelo); background: color-mix(in oklch, var(--color-cat-amarelo) 6%, transparent);"
            : ""}
          title={rev ? reversalTooltip(rev.role) : undefined}
        >
          <td class="pl-4 pr-2 py-2">
            <input
              type="checkbox"
              checked={isSel}
              disabled={!t.fitid}
              onchange={() => t.fitid && ontoggle(t.fitid)}
              aria-label={t.description}
              class="accent-[var(--color-accent)]"
            />
          </td>
          <td class="px-3 py-2 text-sub text-fg-subtle tabular whitespace-nowrap">
            {fmtDate(t.date)}
          </td>
          <td class="px-3 py-2 text-callout text-fg">
            <span class="inline-flex items-center gap-2 flex-wrap">
              <span>{t.description}</span>
              {#if rev}
                <span
                  class="text-cap2 font-semibold px-1.5 py-px rounded-full whitespace-nowrap"
                  style="color: var(--color-cat-amarelo); background: color-mix(in oklch, var(--color-cat-amarelo) 16%, transparent);"
                >
                  {reversalLabel(rev.role)}
                </span>
              {/if}
              {#if isDup}
                <span class="text-cap2 font-semibold text-fg-subtle px-1.5 py-px rounded-full bg-surface-2">
                  {tr("import.duplicate")}
                </span>
              {/if}
            </span>
          </td>
          <td
            class="px-4 py-2 text-right text-callout tabular font-medium whitespace-nowrap
                   {Number(t.amount) >= 0 ? 'text-pos' : 'text-fg'}"
          >
            {formatMoney(t.amount)}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  {#if reversalMap.size > 0}
    <div
      class="px-4 py-2.5 border-t border-border-subtle flex items-start gap-2 text-foot text-fg-muted leading-relaxed"
    >
      <span class="mt-px shrink-0" style="color: var(--color-cat-amarelo)">
        <Icon name="info" size={13} stroke={2} />
      </span>
      <span>
        {reversalMap.size / 2 === 1
          ? tr("import.reversal_legend_one", { n: reversalMap.size / 2 })
          : tr("import.reversal_legend_many", { n: reversalMap.size / 2 })}
      </span>
    </div>
  {/if}
</div>
