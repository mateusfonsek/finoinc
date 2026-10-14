<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "$lib/components/ui/Icon.svelte";
  import logoUrl from "$lib/assets/logo.png";
  import { healthCheck } from "$lib/api/health";
  import { locale } from "$lib/i18n/locale.svelte";
  import { dialog, scrim } from "$lib/motion";

  const t = locale.t;

  type Props = { open: boolean; onClose: () => void };
  let { open, onClose }: Props = $props();

  const GITHUB_URL = "https://github.com/MateusFonseK/finoinc";

  let closeEl: HTMLButtonElement | undefined = $state();
  let panelEl: HTMLElement | undefined = $state();

  /** The version comes from the binary, not a literal in the template — that
   *  is where the stale "v0.2.0" this window used to show came from. */
  let version = $state<string | null>(null);

  let specs = $derived(
    locale.raw<Array<{ label: string; value: string }>>("about.specs") ?? [],
  );

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
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

  $effect(() => {
    if (!open) return;
    void healthCheck()
      .then((h) => (version = h.version))
      .catch(() => (version = null));
    // Focus enters the window as soon as it materializes.
    queueMicrotask(() => closeEl?.focus());
  });
</script>

<svelte:window {onkeydown} />

{#if open}
  <!-- Modal task: the background is dimmed and pushed back. The window
       materializes in place (scale and opacity together) and dematerializes by
       the same path. -->
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
      aria-label={t("sidebar.about_title")}
      transition:dialog
      class="card pointer-events-auto relative w-[min(400px,100%)] max-h-full overflow-y-auto
             rounded-[var(--radius-2xl)] shadow-[var(--shadow-dialog)]"
    >
      <button
        bind:this={closeEl}
        type="button"
        onclick={onClose}
        aria-label={t("common.close")}
        class="press absolute right-3.5 top-3.5 z-10 w-6 h-6 grid place-items-center rounded-full
               text-fg-subtle hover:text-fg hover:bg-hover transition-colors duration-[var(--dur-fast)]"
      >
        <Icon name="x" size={13} stroke={2} />
      </button>

      <div class="flex flex-col py-9">
        <!-- hero -->
        <div class="px-10 flex flex-col items-center text-center gap-4 pb-8">
          <img
            src={logoUrl}
            alt=""
            draggable="false"
            class="w-14 h-14 rounded-[13px]"
            style="box-shadow: 0 6px 18px -8px oklch(0% 0 0 / 0.5)"
          />
          <div class="flex flex-col items-center gap-1.5">
            <h2 class="text-title1 font-semibold text-fg">Finoinc</h2>
            <p class="text-callout text-fg-muted">{t("about.tagline")}</p>
          </div>
        </div>

        <div class="hairline"></div>

        <!-- promessa -->
        <p class="px-10 py-6 text-center text-callout text-fg-muted leading-relaxed">
          {t("about.promise")}
        </p>

        <div class="hairline"></div>

        <!-- spec sheet -->
        <dl class="px-10 py-6 grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 items-baseline">
          {#each specs as s}
            <dt class="text-callout font-semibold text-accent whitespace-nowrap">{s.label}</dt>
            <dd class="text-sub text-fg-muted">{s.value}</dd>
          {/each}
        </dl>

        <div class="hairline"></div>

        <!-- colophon -->
        <div class="px-10 pt-6 flex flex-col gap-3.5">
          <div class="flex items-baseline justify-between gap-3">
            <span class="text-body font-medium text-fg">Mateus Fonseca</span>
            <span class="font-mono text-foot text-fg-subtle tabular">
              {version ? `v${version}` : "—"}
            </span>
          </div>
          <button
            type="button"
            onclick={() => void openUrl(GITHUB_URL)}
            class="self-start inline-flex items-center gap-1 text-sub text-accent hover:text-accent-hi
                   underline-offset-4 hover:underline transition-colors duration-[var(--dur-fast)]"
          >
            github.com/MateusFonseK
            <Icon name="arrowUpRight" size={11} stroke={2} />
          </button>
          <p class="text-cap text-fg-subtle leading-relaxed pt-1">
            {t("about.disclaimer")}
          </p>
        </div>
      </div>
    </div>
  </div>
{/if}
