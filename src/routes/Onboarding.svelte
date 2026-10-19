<script lang="ts">
  import { locale } from "$lib/i18n/locale.svelte";

  const t = locale.t;
  import DropZone from "$lib/components/import/DropZone.svelte";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import logoUrl from "$lib/assets/logo.png";
  import type { ParsedOfx } from "$lib/ofx/types";
  import { push } from "svelte-spa-router";

  let error = $state<string | null>(null);

  function onparsed(detail: { file: File; parsed: ParsedOfx }) {
    // Hand off to /import via window. Svelte stores would be cleaner;
    // acceptable for MVP, refactor in fase 5.
    (window as unknown as { __finoincPending?: typeof detail }).__finoincPending = detail;
    push("/import");
  }

  function onerror(msg: string) {
    error = msg;
  }
</script>

<!-- The app's first screen: one promise, one target, nothing else. The drag
     area at the top exists because there is no page header here. -->
<div data-tauri-drag-region class="w-full" style="height: var(--titlebar-h)"></div>

<section class="px-8 pb-12 max-w-xl mx-auto flex flex-col gap-7">
  <header class="text-center flex flex-col items-center gap-3 pt-6">
    <img
      src={logoUrl}
      alt=""
      draggable="false"
      class="w-16 h-16 rounded-[15px] mb-1"
      style="box-shadow: 0 8px 22px -10px oklch(0% 0 0 / 0.5)"
    />
    <h1 class="text-display font-semibold text-fg text-balance">
      {t("onboarding.title")}
    </h1>
    <p class="text-body text-fg-muted max-w-md leading-relaxed text-balance">
      {t("onboarding.subtitle")}
    </p>
  </header>

  <DropZone {onparsed} {onerror} />

  {#if error}
    <ErrorNote message={t("onboarding.read_error", { msg: error })} />
  {/if}

  <p class="text-foot text-fg-subtle text-center flex items-center justify-center gap-1.5 flex-wrap">
    <Icon name="lock" size={11} />
    {t("onboarding.data_location")}
    <span class="font-mono text-fg-subtle selectable">
      ~/Library/Application Support/app.finoinc/finoinc.db
    </span>
  </p>
</section>
