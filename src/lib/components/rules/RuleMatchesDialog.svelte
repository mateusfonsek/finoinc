<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { Button } from "$lib/components/ui/button";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import Loading from "$lib/components/ui/Loading.svelte";
  import { locale } from "$lib/i18n/locale.svelte";
  import { formatMoney } from "$lib/format/money";
  import { dialog, scrim } from "$lib/motion";
  import { transactionsMatchingRule } from "$lib/api/rules";
  import type { Category, RuleMatches, RuleWithCount } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    rule: RuleWithCount;
    categories: Category[];
    onClose: () => void;
  };

  let { rule, categories, onClose }: Props = $props();

  let data = $state<RuleMatches | null>(null);
  let error = $state<string | null>(null);
  let panelEl: HTMLElement | undefined = $state();
  let closeEl: HTMLButtonElement | undefined = $state();

  let label = $derived(rule.display_name ?? rule.patterns[0] ?? "");

  $effect(() => {
    const id = rule.id;
    data = null;
    error = null;
    void transactionsMatchingRule(id)
      .then((r) => (data = r))
      .catch((e) => (error = e instanceof Error ? e.message : String(e)));
  });

  $effect(() => {
    queueMicrotask(() => closeEl?.focus());
  });

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

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
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
</script>

<svelte:window {onkeydown} />

<!-- This dialog opens ON TOP of the rule edit panel, which stays behind. Its
     own scrim and a higher z: the stack stays clear, and closing here returns
     you to exactly the rule you were editing. -->
<button
  type="button"
  aria-label={t("common.close")}
  onclick={onClose}
  transition:scrim
  class="fixed inset-0 z-70 bg-black/45"
  style="backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px)"
></button>

<div class="fixed inset-0 z-80 grid place-items-center p-6 pointer-events-none">
  <div
    bind:this={panelEl}
    role="dialog"
    aria-modal="true"
    aria-label={t("rule_matches.title")}
    transition:dialog
    class="card pointer-events-auto relative flex flex-col
           w-[min(680px,100%)] max-h-[min(78vh,calc(100vh_-_3rem))]
           rounded-[var(--radius-2xl)] shadow-[var(--shadow-dialog)]"
  >
    <header class="px-6 pt-5 pb-4 border-b border-border-subtle shrink-0">
      <div class="flex items-start justify-between gap-4">
        <div class="flex flex-col gap-1 min-w-0">
          <h2 class="text-title3 font-semibold text-fg truncate" title={label}>
            {t("rule_matches.title")} “{label}”
          </h2>
          {#if data}
            <!-- Count and total together: how often the rule catches, and how
                 much money that represents. -->
            <p class="text-sub text-fg-subtle tabular">
              {data.transactions.length === 1
                ? t("rule_matches.count_one")
                : t("rule_matches.count_many", { n: data.transactions.length })}
              {#if data.transactions.length > 0}
                · {t("rule_matches.total", { value: formatMoney(data.total) })}
              {/if}
            </p>
          {/if}
        </div>
        <button
          bind:this={closeEl}
          type="button"
          onclick={onClose}
          aria-label={t("common.close")}
          class="press w-6 h-6 shrink-0 grid place-items-center rounded-full
                 text-fg-subtle hover:text-fg hover:bg-hover
                 transition-colors duration-[var(--dur-fast)]"
        >
          <Icon name="x" size={13} stroke={2} />
        </button>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto">
      {#if error}
        <div class="p-6"><ErrorNote message={error} /></div>
      {:else if data === null}
        <Loading />
      {:else if data.transactions.length === 0}
        <EmptyState
          icon="inbox"
          title={t("rule_matches.empty_title")}
          description={t("rule_matches.empty_desc")}
        />
      {:else}
        <ul class="divide-y divide-border-subtle">
          {#each data.transactions as tx (tx.id)}
            {@const cat = categoryOf(tx.category_id)}
            <li class="flex items-center gap-3 px-6 py-2.5">
              <span class="text-sub text-fg-subtle tabular w-[68px] shrink-0">
                {fmtDate(tx.date)}
              </span>
              <span class="flex-1 min-w-0 text-callout text-fg truncate" title={tx.description}>
                {tx.description}
              </span>
              <!-- Each row's category is what exposes a rule catching what it
                   should not. -->
              {#if cat}
                <span class="chip text-fg-muted shrink-0">
                  <span
                    class="w-2 h-2 rounded-full"
                    style="background: var({cat.color_token ?? '--color-cat-outros'})"
                  ></span>
                  {cat.name}
                </span>
              {:else}
                <span class="text-foot text-fg-faint shrink-0">
                  {t("rule_matches.no_category")}
                </span>
              {/if}
              <span
                class="text-callout tabular font-medium w-[104px] text-right shrink-0
                       {Number(tx.amount) >= 0 ? 'text-pos' : 'text-fg'}"
              >
                {formatMoney(tx.amount)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <footer
      class="px-6 py-3.5 border-t border-border-subtle shrink-0 flex items-center justify-between gap-4"
    >
      <p class="text-cap text-fg-faint leading-snug min-w-0">{t("rule_matches.note")}</p>
      <Button variant="ghost" onclick={onClose}>{t("common.close")}</Button>
    </footer>
  </div>
</div>
