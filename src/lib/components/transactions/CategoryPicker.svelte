<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { locale } from "$lib/i18n/locale.svelte";
  import { popover } from "$lib/motion";
  import type { Category } from "$lib/bindings";

  const t = locale.t;

  type Props = {
    categories: Category[];
    currentId: number | null;
    onselect: (categoryId: number | null) => void | Promise<void>;
    /** When user wants to create a new category from the typed text. */
    oncreate: (name: string) => Promise<Category>;
  };

  let { categories, currentId, onselect, oncreate }: Props = $props();

  let open = $state(false);
  let dropUp = $state(false);
  let query = $state("");
  let highlighted = $state(0);
  let triggerEl: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let inputEl: HTMLInputElement | undefined = $state();

  let current = $derived(categories.find((c) => c.id === currentId));

  let filtered = $derived(
    categories.filter((c) => c.name.toLowerCase().includes(query.toLowerCase())),
  );

  type Option =
    | { kind: "clear" }
    | { kind: "category"; category: Category }
    | { kind: "create"; name: string };

  let options = $derived<Option[]>(buildOptions(query, filtered, currentId));

  function buildOptions(q: string, list: Category[], cur: number | null): Option[] {
    const out: Option[] = [];
    if (cur !== null) out.push({ kind: "clear" });
    for (const c of list) out.push({ kind: "category", category: c });
    const trimmed = q.trim();
    if (trimmed && !list.some((c) => c.name.toLowerCase() === trimmed.toLowerCase())) {
      out.push({ kind: "create", name: trimmed });
    }
    return out;
  }

  async function openPicker() {
    // Decide the direction before opening: if it does not fit below the trigger
    // but fits above, open upward. Keeps the menu on screen on the last rows.
    if (triggerEl) {
      const rect = triggerEl.getBoundingClientRect();
      const menuH = Math.min(options.length * 30, 240) + 48; // ul + input
      const spaceBelow = window.innerHeight - rect.bottom;
      dropUp = spaceBelow < menuH && rect.top > spaceBelow;
    }
    open = true;
    query = "";
    // Opens with the CURRENT category highlighted, not "remove category": the
    // option under Enter must be harmless, never the destructive one.
    const cur = options.findIndex(
      (o) => o.kind === "category" && o.category.id === currentId,
    );
    highlighted = cur >= 0 ? cur : 0;
    await tick();
    inputEl?.focus();
    // Scrolls the selected row into view when the list is long.
    menuEl
      ?.querySelector<HTMLElement>('[role="option"][aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }

  function closePicker() {
    open = false;
    triggerEl?.focus();
  }

  async function choose(opt: Option) {
    if (opt.kind === "clear") {
      await onselect(null);
    } else if (opt.kind === "category") {
      await onselect(opt.category.id);
    } else {
      const created = await oncreate(opt.name);
      await onselect(created.id);
    }
    closePicker();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closePicker();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      highlighted = Math.min(highlighted + 1, options.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      highlighted = Math.max(highlighted - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = options[highlighted];
      if (opt) void choose(opt);
    }
  }

  function colorStyle(token: string | null | undefined): string {
    return token ? `background: var(${token})` : "background: var(--color-cat-outros)";
  }

  onMount(() => {
    function clickOutside(e: MouseEvent) {
      if (!open) return;
      const target = e.target as Node | null;
      if (target && !triggerEl?.contains(target) && !menuEl?.contains(target)) {
        closePicker();
      }
    }
    document.addEventListener("mousedown", clickOutside);
    return () => document.removeEventListener("mousedown", clickOutside);
  });
</script>

<div class="relative inline-block">
  <!-- The category pill shows the current state AND is the control itself —
       a direct mapping, with no label to explain it. -->
  <button
    bind:this={triggerEl}
    type="button"
    onclick={openPicker}
    aria-haspopup="listbox"
    aria-expanded={open}
    class="press inline-flex items-center gap-1.5 h-6 pl-2 pr-1.5 rounded-full border
           text-foot font-medium transition-colors duration-[var(--dur-fast)]
           {current
      ? 'border-border bg-surface-2 text-fg hover:bg-hover'
      : 'border-border-subtle border-dashed bg-transparent text-fg-faint hover:bg-hover hover:text-fg-muted'}"
  >
    {#if current}
      <span class="w-2 h-2 rounded-full shrink-0" style={colorStyle(current.color_token)}></span>
      <span class="truncate max-w-[110px]">{current.name}</span>
    {:else}
      <span class="truncate max-w-[110px]">{t("category_picker.no_category")}</span>
    {/if}
    <Icon name="chevronsUpDown" size={10} stroke={2} class="opacity-50" />
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      transition:popover={{ origin: dropUp ? "bottom left" : "top left" }}
      class="material-pop absolute z-30 w-60 overflow-hidden {dropUp
        ? 'bottom-full mb-1.5'
        : 'top-full mt-1.5'}"
    >
      <div class="border-b border-border-subtle p-1.5 flex items-center gap-1.5 px-2">
        <Icon name="search" size={12} stroke={2} class="text-fg-faint" />
        <input
          bind:this={inputEl}
          bind:value={query}
          {onkeydown}
          placeholder={t("category_picker.search_or_create")}
          class="w-full bg-transparent border-0 outline-none text-callout py-1 placeholder:text-fg-faint"
        />
      </div>
      <ul class="max-h-60 overflow-y-auto p-1" role="listbox">
        {#each options as opt, i}
          <li>
            <!-- svelte-ignore a11y_interactive_supports_focus -->
            <button
              type="button"
              tabindex="-1"
              role="option"
              aria-selected={i === highlighted}
              onmouseenter={() => (highlighted = i)}
              onclick={() => choose(opt)}
              class="w-full flex items-center gap-2 px-2 h-7 rounded-[var(--radius-sm)] text-left text-callout
                     transition-colors duration-[var(--dur-instant)]
                     {i === highlighted ? 'bg-accent text-accent-on' : 'text-fg-muted'}"
            >
              {#if opt.kind === "clear"}
                <Icon name="x" size={11} stroke={2.2} class="opacity-70" />
                <span>{t("category_picker.remove_category")}</span>
                <!-- Clearing hands the transaction back to the rules: the next
                     import may file it again. -->
                <span
                  class="ml-auto text-cap shrink-0 {i === highlighted
                    ? 'opacity-70'
                    : 'text-fg-faint'}"
                >
                  {t("category_picker.remove_category_hint")}
                </span>
              {:else if opt.kind === "category"}
                <span
                  class="w-2 h-2 rounded-full shrink-0"
                  style={colorStyle(opt.category.color_token)}
                ></span>
                <span class="truncate">{opt.category.name}</span>
                <span
                  class="ml-auto text-cap shrink-0 {i === highlighted
                    ? 'opacity-70'
                    : 'text-fg-faint'}"
                >
                  {t("kind." + opt.category.kind)}
                </span>
              {:else}
                <Icon name="plus" size={11} stroke={2.4} />
                <span class="truncate">
                  {t("category_picker.create_prefix")}
                  <strong class="font-semibold">“{opt.name}”</strong>
                </span>
              {/if}
            </button>
          </li>
        {:else}
          <li class="px-2.5 py-2 text-sub text-fg-faint">{t("category_picker.empty")}</li>
        {/each}
      </ul>
    </div>
  {/if}
</div>
