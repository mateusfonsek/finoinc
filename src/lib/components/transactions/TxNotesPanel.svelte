<script lang="ts">
  import { tick } from "svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { Button } from "$lib/components/ui/button";
  import { locale } from "$lib/i18n/locale.svelte";
  import { formatMoney } from "$lib/format/money";
  import { scrim, sheet } from "$lib/motion";
  import TxRuleSection from "./TxRuleSection.svelte";
  import type { Category, Transaction } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    transaction: Transaction;
    categories: Category[];
    onClose: () => void;
    onSave: (transactionId: number, notes: string | null) => Promise<void>;
    onCreateRule: (data: { pattern: string; categoryId: number; priority: number }) => Promise<void>;
    onRulesChanged: () => Promise<void>;
  };

  let { transaction, categories, onClose, onSave, onCreateRule, onRulesChanged }: Props = $props();

  let draft = $state("");
  let busy = $state(false);
  let textarea: HTMLTextAreaElement | undefined = $state();

  /** The rule mini-form is open: Esc closes it before the panel. */
  let ruleOpen = $state(false);

  let category = $derived(categories.find((c) => c.id === transaction.category_id));

  function fmtDate(iso: string): string {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(locale.dateLocale, {
      day: "numeric",
      month: "long",
      year: "numeric",
    });
  }

  $effect(() => {
    draft = transaction.notes ?? "";
  });

  async function save() {
    busy = true;
    try {
      const value = draft.trim();
      await onSave(transaction.id, value === "" ? null : value);
      onClose();
    } finally {
      busy = false;
    }
  }

  async function focusTextarea() {
    await tick();
    textarea?.focus();
  }

  $effect(() => {
    void focusTextarea();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (ruleOpen) ruleOpen = false;
      else onClose();
    }
    // Cmd+Enter saves — the standard macOS confirm shortcut in edit panels.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window {onkeydown} />

<!-- Side panel: always enters from the right and leaves to the right. The
     scrim is light because the task is focused, not blocking. -->
<button
  type="button"
  aria-label={t("common.close")}
  onclick={onClose}
  transition:scrim
  class="fixed inset-0 z-50 bg-black/25"
></button>

<div
  transition:sheet={{ side: "right" }}
  class="fixed right-0 top-0 bottom-0 z-60 w-[372px] bg-surface border-l border-border-subtle
         flex flex-col shadow-[var(--shadow-sheet)]"
  role="dialog"
  aria-label={t("tx_notes.detail")}
>
  <header
    data-tauri-drag-region="deep"
    class="flex items-center justify-between px-4 pb-3 border-b border-border-subtle"
    style="padding-top: max(12px, var(--titlebar-h))"
  >
    <span class="section-title">{t("tx_notes.detail")}</span>
    <button
      type="button"
      onclick={onClose}
      aria-label={t("common.close")}
      class="press w-6 h-6 grid place-items-center rounded-full text-fg-faint
             hover:text-fg hover:bg-hover transition-colors duration-[var(--dur-fast)]"
    >
      <Icon name="x" size={13} stroke={2} />
    </button>
  </header>

  <div class="flex-1 overflow-y-auto">
    <!-- The amount is the panel's subject: it opens the read, the rest gives
         context. -->
    <div class="px-4 pt-4 pb-3 flex flex-col gap-1">
      <span
        class="text-display font-semibold tabular {Number(transaction.amount) >= 0
          ? 'text-pos'
          : 'text-fg'}"
      >
        {formatMoney(transaction.amount)}
      </span>
      <span class="text-sub text-fg-subtle tabular">{fmtDate(transaction.date)}</span>
    </div>

    <div class="px-4 pb-4 flex flex-col gap-3">
      <div class="flex flex-col gap-1">
        <span class="text-foot text-fg-subtle">{t("tx_table.description")}</span>
        <span class="text-callout text-fg leading-relaxed selectable">{transaction.description}</span>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-foot text-fg-subtle">{t("tx_table.category")}</span>
        {#if category}
          <span class="chip text-fg">
            <span
              class="w-2 h-2 rounded-full"
              style="background: var({category.color_token ?? '--color-cat-outros'})"
            ></span>
            {category.name}
          </span>
        {:else}
          <span class="text-foot text-fg-subtle">{t("category_picker.no_category")}</span>
        {/if}
      </div>

      {#if transaction.ofx_fitid}
        <div class="flex items-baseline justify-between gap-3">
          <span class="text-foot text-fg-subtle">FITID</span>
          <span class="font-mono text-cap text-fg-subtle truncate selectable">
            {transaction.ofx_fitid}
          </span>
        </div>
      {/if}
    </div>

    <div class="hairline"></div>

    <TxRuleSection {transaction} {categories} {onCreateRule} {onRulesChanged} bind:formOpen={ruleOpen} />

    <div class="hairline"></div>

    <div class="px-4 pt-3 pb-4">
      <label class="section-title mb-1.5 block" for="tx-notes">
        {t("tx_notes.notes")}
      </label>
      <textarea
        id="tx-notes"
        bind:this={textarea}
        bind:value={draft}
        placeholder={t("tx_notes.notes_placeholder")}
        rows="6"
        class="field w-full resize-none leading-relaxed"
      ></textarea>
    </div>
  </div>

  <footer class="px-4 py-3 border-t border-border-subtle flex justify-end gap-2">
    <Button variant="ghost" onclick={onClose}>{t("common.cancel")}</Button>
    <Button onclick={save} disabled={busy}>
      {busy ? t("categories.saving") : t("categories.save")}
    </Button>
  </footer>
</div>
