<script lang="ts">
  import { push } from "svelte-spa-router";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { Button } from "$lib/components/ui/button";
  import { locale } from "$lib/i18n/locale.svelte";
  import { toast as toastMotion } from "$lib/motion";
  import { update } from "$lib/stores/update.svelte";

  const t = locale.t;

  function view() {
    update.dismissFound();
    void push("/settings");
  }
</script>

<!-- Left by an automatic check: a reminder, not a prompt. Installing stays in
     Settings, where the version, the notes and the button live together. -->
<div
  transition:toastMotion
  class="material-pop w-[326px] p-3.5 flex items-center gap-2.5 rounded-[var(--radius-xl)]"
  role="status"
>
  <span
    class="w-7 h-7 shrink-0 grid place-items-center rounded-[var(--radius-md)] bg-accent-soft text-accent"
  >
    <Icon name="download" size={13} stroke={2.2} />
  </span>
  <span class="flex-1 min-w-0 text-callout font-semibold text-fg truncate">
    {t("update.item_title", { v: update.found ?? "" })}
  </span>
  <Button variant="ghost" size="sm" onclick={view}>{t("update.item_view")}</Button>
  <button
    type="button"
    onclick={() => update.dismissFound()}
    title={t("update.item_dismiss")}
    aria-label={t("update.item_dismiss")}
    class="press w-5 h-5 grid place-items-center rounded-full text-fg-faint
           hover:text-fg hover:bg-hover transition-colors duration-[var(--dur-fast)]"
  >
    <Icon name="x" size={11} stroke={2.4} />
  </button>
</div>
