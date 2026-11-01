<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { Button } from "$lib/components/ui/button";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import { locale } from "$lib/i18n/locale.svelte";
  import { formatMoney } from "$lib/format/money";
  import { dialog, scrim } from "$lib/motion";
  import type { Category, RuleChoice, RulePreviewRow } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    rows: RulePreviewRow[];
    categories: Category[];
    onClose: () => void;
    onApply: (choices: RuleChoice[]) => Promise<void>;
  };

  let { rows, categories, onClose, onApply }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let panelEl: HTMLElement | undefined = $state();
  let closeEl: HTMLButtonElement | undefined = $state();

  /** Two classes of change, and they are not equivalent: filling a blank undoes
   *  nobody's decision; replacing an existing category does. */
  let uncategorized = $derived(rows.filter((r) => r.current_category_id === null));
  let overrides = $derived(rows.filter((r) => r.current_category_id !== null));

  /**
   * Selection by transaction id. Starts with the uncategorized ones ticked —
   * exactly what the button always did — and the replacements unticked: the
   * fast path must not be the destructive one.
   */
  let selected = $state(new Set<number>());
  $effect(() => {
    selected = new Set(uncategorized.map((r) => r.transaction_id));
  });

  function toggle(id: number) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  function setGroup(group: RulePreviewRow[], on: boolean) {
    const next = new Set(selected);
    for (const r of group) {
      if (on) next.add(r.transaction_id);
      else next.delete(r.transaction_id);
    }
    selected = next;
  }

  function allSelected(group: RulePreviewRow[]): boolean {
    return group.length > 0 && group.every((r) => selected.has(r.transaction_id));
  }

  function categoryOf(id: number | null): Category | undefined {
    return id === null ? undefined : categories.find((c) => c.id === id);
  }

  /** "2026-08-14" becomes "14 ago". The year only appears when it is not the
   *  current one: in a long list it repeats without helping identify the row. */
  function fmtDate(iso: string): string {
    const mo = Number(iso.slice(5, 7)) - 1;
    const year = iso.slice(0, 4);
    const short = `${iso.slice(8, 10)} ${(locale.monthsShort[mo] ?? "").toLowerCase()}`;
    return year === String(new Date().getFullYear()) ? short : `${short} ${year}`;
  }

  async function apply() {
    error = null;
    busy = true;
    try {
      const choices: RuleChoice[] = rows
        .filter((r) => selected.has(r.transaction_id))
        .map((r) => ({
          transaction_id: r.transaction_id,
          category_id: r.new_category_id,
        }));
      await onApply(choices);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (!busy) onClose();
      return;
    }
    // Cmd+Enter confirms — same shortcut as the edit panels.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (!busy && selected.size > 0) void apply();
      return;
    }
    // Modal task: focus does not escape the dialog while it is open.
    if (e.key === "Tab" && panelEl) {
      const focusables = panelEl.querySelectorAll<HTMLElement>(
        "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
      );
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  $effect(() => {
    queueMicrotask(() => closeEl?.focus());
  });
</script>

<svelte:window {onkeydown} />

<!-- Modal task: the background dims and is pushed back. The window
     materializes in place (scale and opacity together) and dematerializes by
     the same path. -->
<button
  type="button"
  aria-label={t("common.cancel")}
  onclick={() => !busy && onClose()}
  transition:scrim
  class="fixed inset-0 z-70 bg-black/45"
  style="backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px)"
></button>

<div class="fixed inset-0 z-80 grid place-items-center p-6 pointer-events-none">
  <div
    bind:this={panelEl}
    role="dialog"
    aria-modal="true"
    aria-label={t("rule_apply.title")}
    transition:dialog
    class="card pointer-events-auto relative flex flex-col
           w-[min(720px,100%)] max-h-[min(78vh,calc(100vh_-_3rem))]
           rounded-[var(--radius-2xl)] shadow-[var(--shadow-dialog)]"
  >
    <header class="px-6 pt-5 pb-4 border-b border-border-subtle shrink-0">
      <div class="flex items-start justify-between gap-4">
        <div class="flex flex-col gap-1 min-w-0">
          <h2 class="text-title3 font-semibold text-fg">{t("rule_apply.title")}</h2>
          <p class="text-sub text-fg-subtle">
            {rows.length === 1
              ? t("rule_apply.subtitle_one")
              : t("rule_apply.subtitle_many", { n: rows.length })}
          </p>
        </div>
        <button
          bind:this={closeEl}
          type="button"
          onclick={onClose}
          disabled={busy}
          aria-label={t("common.close")}
          class="press w-6 h-6 shrink-0 grid place-items-center rounded-full
                 text-fg-subtle hover:text-fg hover:bg-hover
                 transition-colors duration-[var(--dur-fast)]"
        >
          <Icon name="x" size={13} stroke={2} />
        </button>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-5">
      {#if uncategorized.length > 0}
        {@render group(
          t("rule_apply.group_uncategorized"),
          uncategorized,
          undefined,
          false,
        )}
      {/if}

      {#if overrides.length > 0}
        {@render group(
          t("rule_apply.group_override"),
          overrides,
          t("rule_apply.group_override_note"),
          true,
        )}
      {/if}

      {#if error}
        <ErrorNote message={error} />
      {/if}
    </div>

    <footer
      class="px-6 py-4 border-t border-border-subtle shrink-0 flex items-center justify-between gap-4"
    >
      <!-- Left: how much of the total is ticked. Button: what it will do. The
           button states the NUMBER, so "apply all" is never ambiguous — there is
           no implicit "all" anywhere. -->
      <span class="text-foot text-fg-subtle tabular">
        {selected.size === 0
          ? t("rule_apply.selected_none")
          : t("rule_apply.selected_count", { n: selected.size, total: rows.length })}
      </span>
      <div class="flex gap-2">
        <Button variant="ghost" onclick={onClose} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button onclick={apply} disabled={busy || selected.size === 0}>
          {busy
            ? t("rule_apply.applying")
            : selected.size === 1
              ? t("rule_apply.apply_one")
              : t("rule_apply.apply_many", { n: selected.size })}
        </Button>
      </div>
    </footer>
  </div>
</div>

{#snippet group(
  title: string,
  items: RulePreviewRow[],
  note: string | undefined,
  warn: boolean,
)}
  <section class="flex flex-col gap-2">
    <div class="flex items-center justify-between gap-3">
      <!-- The palette only has `pos` and `neg`; painting this section red would
           read as an error, and it is a legitimate choice. The symbol plus the
           text carry the caution — which is what works for anyone who cannot
           distinguish the colours anyway. -->
      <div class="flex items-center gap-2 min-w-0">
        {#if warn}
          <Icon name="triangleAlert" size={12.5} stroke={2} class="text-fg-muted shrink-0" />
        {/if}
        <span class="section-title">{title}</span>
        <span class="text-foot text-fg-faint tabular">{items.length}</span>
      </div>
      <!-- Per-section "all" rather than one global: the two classes carry
           different risk, so bulk-ticking is a decision per class. -->
      <label class="flex items-center gap-1.5 text-foot text-fg-muted cursor-default
                    hover:text-fg transition-colors duration-[var(--dur-fast)]">
        <input
          type="checkbox"
          checked={allSelected(items)}
          onchange={(e) => setGroup(items, e.currentTarget.checked)}
          
        />
        {t("rule_apply.select_all")}
      </label>
    </div>

    {#if note}
      <p class="text-cap text-fg-faint leading-snug">{note}</p>
    {/if}

    <ul class="card-inset divide-y divide-border-subtle">
      {#each items as r (r.transaction_id)}
        {@const from = categoryOf(r.current_category_id)}
        {@const to = categoryOf(r.new_category_id)}
        {@const on = selected.has(r.transaction_id)}
        <li>
          <!-- The whole row toggles the tick: a large target, as in macOS
               lists. The highlight starts on press, not on release. -->
          <label
            class="press-sm flex items-start gap-3 px-3 py-2.5 cursor-default
                   transition-colors duration-[var(--dur-fast)]
                   {on ? 'bg-accent-soft' : 'hover:bg-hover'}"
          >
            <input
              type="checkbox"
              checked={on}
              onchange={() => toggle(r.transaction_id)}
              class="mt-0.5 shrink-0"
            />
            <span class="flex-1 min-w-0 flex flex-col gap-0.5">
              <span class="text-callout text-fg truncate" title={r.description}>
                {r.description}
              </span>
              <span class="text-cap text-fg-faint truncate">
                {fmtDate(r.date)} · <span class="tabular">{formatMoney(r.amount)}</span>
                {#if r.rule_label}
                  · {t("rule_apply.by_rule", { rule: r.rule_label })}
                {/if}
              </span>
            </span>
            <!-- The change reads left to right: from where it is to where it
                 goes. The arrow carries the meaning, not the colour. -->
            <span class="flex items-center gap-2 shrink-0 text-sub">
              {#if from}
                <span class="chip text-fg-muted">
                  <span
                    class="w-2 h-2 rounded-full"
                    style="background: var({from.color_token ?? '--color-cat-outros'})"
                  ></span>
                  {from.name}
                </span>
              {:else}
                <span class="text-foot text-fg-faint">{t("rule_apply.no_category")}</span>
              {/if}
              <Icon name="arrowRight" size={12} stroke={2} class="text-fg-faint" />
              <span class="chip text-fg font-medium">
                <span
                  class="w-2 h-2 rounded-full"
                  style="background: var({to?.color_token ?? '--color-cat-outros'})"
                ></span>
                {to?.name ?? "?"}
              </span>
            </span>
          </label>
        </li>
      {/each}
    </ul>
  </section>
{/snippet}
