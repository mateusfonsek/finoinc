<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { locale } from "$lib/i18n/locale.svelte";

  const t = locale.t;

  type Props = {
    value: string;
    placeholder?: string;
    onInput: (v: string) => void;
    ref?: HTMLInputElement | null;
  };

  let { value, placeholder, onInput, ref = $bindable(null) }: Props = $props();
</script>

<!-- macOS search field: capsule, magnifier on the left, clear button that only
     exists when there is something to clear. -->
<div
  class="group inline-flex items-center gap-1.5 h-7 pl-2 pr-1.5 rounded-full border border-border
         bg-surface-2 shadow-[var(--shadow-raised)]
         transition-[border-color,background-color,box-shadow] duration-[var(--dur-fast)] ease-[var(--ease-snap)]
         focus-within:border-accent focus-within:bg-bg focus-within:shadow-[var(--focus-ring)]"
>
  <Icon name="search" size={12.5} stroke={2} class="text-fg-faint group-focus-within:text-accent" />
  <input
    bind:this={ref}
    type="text"
    data-search-input
    data-focus-ring="none"
    {value}
    placeholder={placeholder ?? t("search.placeholder")}
    oninput={(e) => onInput((e.currentTarget as HTMLInputElement).value)}
    class="bg-transparent border-0 outline-none text-callout w-40 text-fg placeholder:text-fg-faint"
  />
  {#if value}
    <button
      type="button"
      onclick={() => onInput("")}
      aria-label={t("search.clear")}
      class="press w-4 h-4 grid place-items-center rounded-full bg-fg-faint/25 text-fg
             hover:bg-fg-faint/45 transition-colors duration-[var(--dur-fast)]"
    >
      <Icon name="x" size={9} stroke={3} />
    </button>
  {:else}
    <span class="w-4"></span>
  {/if}
</div>
