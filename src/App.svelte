<script lang="ts">
  import { onMount } from "svelte";
  import Router, { push } from "svelte-spa-router";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Sidebar from "$lib/components/shell/Sidebar.svelte";
  import AboutDialog from "$lib/components/shell/AboutDialog.svelte";
  import ActivityCenter from "$lib/components/shell/ActivityCenter.svelte";
  import LanguageGate from "$lib/components/shell/LanguageGate.svelte";
  import { openOfxPath } from "$lib/ofx/open";
  import { takePendingOfx } from "$lib/api/files";
  import { locale, LOCALE_CHOSEN_KEY } from "$lib/i18n/locale.svelte";
  import { getAppSetting } from "$lib/api/watch";
  import { update } from "$lib/stores/update.svelte";
  import { watch } from "$lib/stores/watch.svelte";
  import { routes } from "./routes/routes";

  // Sync the UI language from the backend's persisted choice on boot.
  void locale.init();

  const GITHUB_URL = "https://github.com/MateusFonseK/finoinc";

  let aboutOpen = $state(false);

  // `null` while the durable flag is still loading — the gate is the safe
  // default (shown, not skipped) both then and if the read fails, since a
  // false negative here would let a fresh install straight past onboarding.
  let localeChosen = $state<boolean | null>(null);

  onMount(async () => {
    try {
      localeChosen = (await getAppSetting(LOCALE_CHOSEN_KEY)) === "1";
    } catch {
      localeChosen = false;
    }
  });

  // Runs exactly once, the moment the gate first comes down (never again,
  // since the flag is never unset) — draining a pending "Open with Finoinc" and
  // running the watch scan only make sense once the DB is no longer pristine.
  $effect(() => {
    if (!localeChosen) return;
    void handleOpenedOfx();
    void watch.loadEnabled().then(() => watch.refresh({ force: true }));
    // Off by default: reads one setting and, unless the user opted in, stops.
    void update.start();
  });

  // Scroll state of the content pane. Each screen's header is translucent
  // material pinned to the top; the rule separating it from the content only
  // appears when content passes beneath — never as permanent decoration.
  let scroller: HTMLElement | undefined = $state();
  let scrolled = $state(false);

  function onMainScroll() {
    const next = (scroller?.scrollTop ?? 0) > 2;
    if (next !== scrolled) scrolled = next;
  }

  /** Switching screens always starts at the top, like a navigation push. */
  function resetScroll() {
    scroller?.scrollTo({ top: 0 });
    scrolled = false;
  }

  // "Open with Finoinc": drains the .ofx opened via Finder, loads them and hands
  // them to the Import screen through the same stash it already reads.
  async function handleOpenedOfx() {
    const paths = await takePendingOfx();
    for (const path of paths) await openOfxPath(path);
  }

  const SHORTCUTS: Record<string, () => void> = {
    "1": () => push("/dashboard"),
    "2": () => push("/transactions"),
    "3": () => push("/calendar"),
    "4": () => push("/import"),
    "5": () => push("/categories"),
    "6": () => push("/rules"),
    "7": () => push("/suggestions"),
    "8": () => push("/mcp"),
  };

  function onKeydown(e: KeyboardEvent) {
    if (!localeChosen) return;
    if (!(e.metaKey || e.ctrlKey)) return;
    const target = e.target as HTMLElement | null;
    const inEditable =
      target !== null &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable);

    const k = e.key.toLowerCase();

    if (k === "f") {
      e.preventDefault();
      const wasOnTransactions = window.location.hash === "#/transactions";
      if (!wasOnTransactions) push("/transactions");
      setTimeout(() => {
        const input = document.querySelector<HTMLInputElement>("[data-search-input]");
        input?.focus();
        input?.select();
      }, 40);
      return;
    }

    if (k === "o") {
      e.preventDefault();
      push("/import");
      return;
    }

    if (inEditable) return;

    if (SHORTCUTS[e.key]) {
      e.preventDefault();
      SHORTCUTS[e.key]();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });

  onMount(() => {
    const unlisten: Array<() => void> = [];
    // The native menu and the "open with Finoinc" flow reach the webview
    // regardless of what it renders, so each handler re-checks the gate
    // itself instead of trusting that registration implies it's down.
    listen("menu:about", () => localeChosen && (aboutOpen = true)).then((u) => unlisten.push(u));
    listen<string>("menu:navigate", (e) => localeChosen && push(e.payload)).then((u) =>
      unlisten.push(u),
    );
    listen("menu:github", () => void openUrl(GITHUB_URL)).then((u) => unlisten.push(u));
    listen("open-ofx", () => localeChosen && void handleOpenedOfx()).then((u) =>
      unlisten.push(u),
    );
    // MCP writes are handled per screen, not here: a screen that can safely
    // refresh its own data listens for `MCP_DATA_CHANGED_EVENT` itself, so a
    // mid-edit form or an in-progress OFX review is never at the mercy of a
    // change happening somewhere else in the app. One with nothing safe to
    // refresh just picks the change up on its next navigation.

    // Focus is what makes a filesystem watcher unnecessary — the user sends
    // the file from their phone and then comes to look at the Mac.
    const onFocus = () => localeChosen && void watch.refresh();
    window.addEventListener("focus", onFocus);
    unlisten.push(() => window.removeEventListener("focus", onFocus));

    return () => unlisten.forEach((u) => u());
  });
</script>

{#if localeChosen}
  <div class="h-screen grid grid-cols-[236px_1fr] overflow-hidden">
    <Sidebar onAbout={() => (aboutOpen = true)} />
    <!-- `data-scrolled` feeds the sticky header's edge effect: the separating
         rule only exists when content passes beneath it. -->
    <main
      bind:this={scroller}
      onscroll={onMainScroll}
      data-scrolled={scrolled}
      class="bg-bg overflow-y-auto"
    >
      <Router {routes} onRouteLoaded={resetScroll} />
    </main>
  </div>

  <ActivityCenter />

  <AboutDialog open={aboutOpen} onClose={() => (aboutOpen = false)} />
{:else if localeChosen === false}
  <!-- No Sidebar, no Router, no ActivityCenter: the initial-setup screen
       replaces the shell entirely rather than sitting disabled inside it. -->
  <LanguageGate onConfirm={() => (localeChosen = true)} />
{/if}
