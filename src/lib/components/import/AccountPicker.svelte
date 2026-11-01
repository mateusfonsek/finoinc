<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { locale } from "$lib/i18n/locale.svelte";
  import type { Account } from "$lib/bindings";
  import { defaultPick, NEW_ACCOUNT, type AccountChoice } from "$lib/import/account";

  const t = locale.t;


  type Props = {
    accounts: Account[];
    /** The statement's account type; matching accounts are listed first. */
    kind: string;
    /** The statement's bank, when the format names one. */
    bank: string | null;
    /** Pre-selected account, when the user is changing a remembered one. */
    initial: number | null;
    /** Name suggested for a new account (the format's name). */
    suggestedName: string;
    busy?: boolean;
    onconfirm: (choice: AccountChoice) => void;
    oncancel?: () => void;
  };

  let { accounts, kind, bank, initial, suggestedName, busy = false, onconfirm, oncancel }: Props = $props();

  // Accounts of the statement's own type come first: a card bill usually goes
  // into a card account. The rest stay reachable — the user knows best.
  let ordered = $derived([
    ...accounts.filter((a) => a.kind === kind),
    ...accounts.filter((a) => a.kind !== kind),
  ]);

  const NEW = NEW_ACCOUNT;
  // svelte-ignore state_referenced_locally — props seed the initial pick only.
  let picked = $state<number>(defaultPick(accounts, kind, bank, initial));
  // svelte-ignore state_referenced_locally
  let newName = $state(suggestedName);

  let canConfirm = $derived(picked !== NEW || newName.trim() !== "");

  function confirm() {
    if (!canConfirm || busy) return;
    onconfirm(picked === NEW ? { newName: newName.trim() } : { accountId: picked });
  }
</script>

<div class="card p-4 flex flex-col gap-3">
  <div class="flex flex-col gap-0.5">
    <h3 class="text-title3 font-semibold text-fg">{t("import.account_pick_title")}</h3>
    <p class="text-sub text-fg-muted leading-relaxed">{t("import.account_pick_hint")}</p>
  </div>

  <div role="radiogroup" aria-label={t("import.account_pick_title")} class="flex flex-col gap-1">
    {#each ordered as a (a.id)}
      <label
        class="flex items-center gap-2.5 px-2.5 py-2 rounded-[var(--radius-md)] cursor-pointer
               transition-colors duration-[var(--dur)] {picked === a.id ? 'bg-accent-soft' : 'hover:bg-surface-2'}"
      >
        <input type="radio" name="import-account" value={a.id} bind:group={picked} class="accent-[var(--color-accent)]" />
        <Icon name={a.kind === "credit_card" ? "creditCard" : "landmark"} size={14} />
        <span class="text-callout text-fg truncate">{a.name}</span>
        <span class="text-cap text-fg-subtle ml-auto shrink-0">
          {a.kind === "credit_card" ? t("import.credit_card") : t("import.checking")}
        </span>
      </label>
    {/each}

    <label
      class="flex items-center gap-2.5 px-2.5 py-2 rounded-[var(--radius-md)] cursor-pointer
             transition-colors duration-[var(--dur)] {picked === NEW ? 'bg-accent-soft' : 'hover:bg-surface-2'}"
    >
      <input type="radio" name="import-account" value={NEW} bind:group={picked} class="accent-[var(--color-accent)]" />
      <Icon name="plus" size={14} />
      <span class="text-callout text-fg shrink-0">{t("import.account_new")}</span>
      {#if picked === NEW}
        <!-- svelte-ignore a11y_autofocus — the field appears because the user just asked for it -->
        <input
          class="field ml-2 flex-1 min-w-0"
          bind:value={newName}
          placeholder={t("import.account_new_placeholder")}
          aria-label={t("import.account_new_placeholder")}
          autofocus
          onkeydown={(e) => e.key === "Enter" && confirm()}
        />
      {/if}
    </label>
  </div>

  <div class="flex justify-end gap-2">
    {#if oncancel}
      <Button variant="ghost" onclick={oncancel} disabled={busy}>{t("common.cancel")}</Button>
    {/if}
    <Button onclick={confirm} disabled={!canConfirm || busy}>{t("import.account_use")}</Button>
  </div>
</div>
