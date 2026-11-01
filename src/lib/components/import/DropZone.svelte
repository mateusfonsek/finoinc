<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { decodeOfxFile } from "$lib/ofx/normalize";
  import { parseOfx } from "$lib/ofx/parse";
  import { readFileBytes } from "$lib/api/files";
  import { locale } from "$lib/i18n/locale.svelte";
  import type { ParsedStatement } from "$lib/statement/types";
  import { push } from "svelte-spa-router";
  import { watch } from "$lib/stores/watch.svelte";
  import { getAppSetting, setAppSetting, WATCH_HINT_DISMISSED_KEY } from "$lib/api/watch";

  const t = locale.t;

  type Props = {
    onparsed?: (result: { file: File; parsed: ParsedStatement }) => void;
    onerror?: (message: string) => void;
  };

  let { onparsed, onerror }: Props = $props();

  let active = $state(false);
  let busy = $state(false);
  let fileInput: HTMLInputElement | undefined = $state();

  let hintDismissed = $state(true); // pessimistic until loaded, avoids a flash

  onMount(() => {
    void getAppSetting(WATCH_HINT_DISMISSED_KEY).then((v) => (hintDismissed = v === "1"));
  });

  async function dismissHint() {
    hintDismissed = true;
    try {
      await setAppSetting(WATCH_HINT_DISMISSED_KEY, "1");
    } catch {
      // This component has no error surface (it is not an import flow already
      // showing `error`), and dismissing the hint is cosmetic — not worth a
      // toast. But if the write fails silently, a local `hintDismissed` would
      // lie: the hint returns on the next boot (it was never saved) while the
      // user believes it was dismissed. Undoing the optimistic update keeps the
      // UI honest with disk without inventing a new error surface.
      hintDismissed = false;
    }
  }

  // Native drag-and-drop: Tauri intercepts the Finder drop and delivers only
  // the path (not a File), via a webview event. The bytes are read and a File is
  // built from them so the file-picker pipeline is reused.
  async function handlePath(path: string) {
    try {
      const bytes = await readFileBytes(path);
      const name = path.split(/[\\/]/).pop() || "extrato.ofx";
      await handleFile(new File([bytes as BlobPart], name));
    } catch (e) {
      onerror?.(e instanceof Error ? e.message : String(e));
    }
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") {
          active = true;
        } else if (p.type === "leave") {
          active = false;
        } else if (p.type === "drop") {
          active = false;
          const ofx = p.paths.find((path) => path.toLowerCase().endsWith(".ofx"));
          if (ofx) void handlePath(ofx);
          else if (p.paths.length > 0) onerror?.(t("import.drop_ofx_error"));
        }
      })
      .then((un) => (unlisten = un));
    return () => unlisten?.();
  });

  async function handleFile(file: File) {
    busy = true;
    try {
      const content = await decodeOfxFile(file);
      const parsed = parseOfx(content);
      onparsed?.({ file, parsed });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      onerror?.(msg);
    } finally {
      busy = false;
    }
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
    active = true;
  }

  function onDragLeave(e: DragEvent) {
    e.preventDefault();
    active = false;
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    active = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) await handleFile(file);
  }

  async function onFilePicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) await handleFile(file);
  }
</script>

<!-- The zone reacts to the drag BEFORE the drop: the frame lights up, the icon
     rises and the surface takes the accent. The target confirms it accepts
     before you let go. -->
<div
  role="button"
  tabindex="0"
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
  onclick={() => fileInput?.click()}
  onkeydown={(e) => (e.key === "Enter" || e.key === " ") && fileInput?.click()}
  class="group rounded-[var(--radius-2xl)] border border-dashed p-10 flex flex-col items-center gap-3 text-center
         transition-[background-color,border-color,transform] duration-[var(--dur)] ease-[var(--ease-snap)]
         active:scale-[0.995]
         {active
    ? 'border-accent bg-accent-soft'
    : 'border-border bg-surface hover:bg-surface-2 hover:border-fg-faint'}"
>
  <div
    class="w-14 h-14 rounded-[var(--radius-xl)] grid place-items-center border
           transition-[transform,color,background-color,border-color] duration-[var(--dur)] ease-[var(--ease-snap)]
           {active
      ? 'bg-accent text-accent-on border-transparent -translate-y-1 scale-105'
      : 'bg-surface-2 border-border text-accent group-hover:-translate-y-0.5'}"
  >
    <Icon name="download" size={26} stroke={1.6} />
  </div>

  <h3 class="text-title3 font-semibold text-fg">
    {t("import.dropzone_title")}
  </h3>
  <p class="text-sub text-fg-muted max-w-sm leading-relaxed">
    {t("import.dropzone_desc_1")}
    <strong class="text-fg font-semibold">.ofx</strong>
    {t("import.dropzone_desc_2")}
  </p>

  {#if busy}
    <p class="text-sub text-fg-faint mt-1 flex items-center gap-2">
      <Spinner size={13} />
      {t("import.reading")}
    </p>
  {/if}

  <input
    bind:this={fileInput}
    type="file"
    accept=".ofx,.OFX,application/x-ofx,text/plain"
    onchange={onFilePicked}
    class="hidden"
  />
</div>

{#if !watch.enabled && !hintDismissed}
  <!-- Outside the dropzone: the element above has an onclick for the file
       picker, so the hint must be a sibling, not a child, or clicking it would
       open the picker. -->
  <div class="flex items-center justify-center gap-1 mt-3 text-sub text-fg-subtle">
    <button
      type="button"
      onclick={() => push("/settings")}
      class="inline-flex items-center gap-1.5 px-2 py-1 rounded-[var(--radius-sm)] text-left
             hover:text-fg hover:bg-hover transition-colors duration-[var(--dur-fast)]"
    >
      <Icon name="zap" size={12} class="shrink-0" />
      <span>{t("watch.hint_dropzone")}</span>
      <Icon name="chevronRight" size={11} stroke={2} class="shrink-0" />
    </button>
    <button
      type="button"
      onclick={dismissHint}
      title={t("watch.hint_dismiss")}
      aria-label={t("watch.hint_dismiss")}
      class="press w-5 h-5 grid place-items-center rounded-full hover:text-fg hover:bg-hover
             transition-colors duration-[var(--dur-fast)]"
    >
      <Icon name="x" size={10} stroke={2.4} />
    </button>
  </div>
{/if}
