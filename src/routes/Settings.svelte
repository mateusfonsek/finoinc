<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import Page from "$lib/components/ui/Page.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import ErrorNote from "$lib/components/ui/ErrorNote.svelte";
  import LocalePicker from "$lib/components/shell/LocalePicker.svelte";
  import { popover } from "$lib/motion";
  import { healthCheck } from "$lib/api/health";
  import {
    confirm,
    open as openDialog,
    save as saveDialog,
    message,
  } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { desktopDir, downloadDir, homeDir, join } from "@tauri-apps/api/path";
  import { modKeyLabel, platform } from "$lib/platform";
  import { dbPath, exportBackup, restoreBackup } from "$lib/api/backup";
  import { locale } from "$lib/i18n/locale.svelte";
  import { watch } from "$lib/stores/watch.svelte";
  import {
    addWatchedFolder,
    dirExists,
    ensureDir,
    getAppSetting,
    ICLOUD_PENDING_KEY,
    LAST_SCAN_KEY,
    listWatchedFolders,
    removeWatchedFolder,
    updateWatchedFolderPath,
  } from "$lib/api/watch";
  import { enrichmentStatus, setEnrichmentEnabled } from "$lib/api/enrichment";
  import { freshnessEnabled, setFreshnessEnabled } from "$lib/api/freshness";
  import type { EnrichmentStatus, WatchedFolder } from "$lib/bindings";

  const t = locale.t;
  const GITHUB_URL = "https://github.com/MateusFonseK/finoinc";
  // OS-specific names, read through literal prefixes so every key stays visible.
  const fm = $derived(t(`os.file_manager_${platform}`));
  const mod = modKeyLabel();

  let path = $state<string | null>(null);
  let busy = $state(false);
  let info = $state<string | null>(null);
  let error = $state<string | null>(null);

  /** `null` while loading. Availability comes from the backend, never from a
   *  language comparison here. */
  let enrich = $state<EnrichmentStatus | null>(null);

  async function toggleEnrich(value: boolean) {
    await setEnrichmentEnabled(value);
    enrich = await enrichmentStatus();
  }

  let freshness = $state(true);

  async function toggleFreshness(value: boolean) {
    await setFreshnessEnabled(value);
    freshness = value;
  }

  let folders = $state<WatchedFolder[]>([]);
  let menuOpen = $state(false);
  let menuWrapperEl: HTMLDivElement | undefined = $state();
  let icloudWaiting = $state(0);
  let lastScanAt = $state<string | null>(null);
  /** The version comes from the binary — a literal in the template goes stale
   *  without warning. */
  let version = $state<string | null>(null);

  async function loadEnrich() {
    try {
      enrich = await enrichmentStatus();
    } catch {
      // No status, no setting — better hidden than showing a switch that does
      // not know its own state.
      enrich = null;
    }
  }

  async function loadFolders() {
    folders = await listWatchedFolders();
    icloudWaiting = Number((await getAppSetting(ICLOUD_PENDING_KEY)) ?? "0");
    lastScanAt = await getAppSetting(LAST_SCAN_KEY);
  }

  /** The stamp comes from SQLite in UTC with no timezone suffix; the "Z" makes
   *  `Date` read it as UTC and display it in the reader's local time. */
  function formatScan(stamp: string): string {
    const d = new Date(`${stamp.replace(" ", "T")}Z`);
    if (Number.isNaN(d.getTime())) return stamp;
    return d.toLocaleString(locale.dateLocale, { dateStyle: "short", timeStyle: "short" });
  }

  /** Rust returns developer-facing English, like every other command — none of
   *  which can reach the screen raw. This turns it into translated text, and the
   *  case users actually hit (pointing at a folder already in the list) gets its
   *  own sentence instead of "UNIQUE constraint failed". */
  function folderError(
    e: unknown,
    fallbackKey: string,
    params?: Record<string, string | number>,
  ): string {
    const raw = e instanceof Error ? e.message : String(e);
    if (raw.includes("watched_folders.path")) return t("watch.error_duplicate");
    if (raw.includes("failed to resolve path") || raw.includes("is not a directory")) {
      return t("watch.error_unreachable");
    }
    return t(fallbackKey, params);
  }

  onMount(async () => {
    try {
      path = await dbPath();
      void healthCheck().then((h) => (version = h.version)).catch(() => {});
      await loadEnrich();
      freshness = await freshnessEnabled();
      await watch.loadEnabled();
      if (watch.enabled) await loadFolders();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });

  // Closes the "add folder" menu on outside click or Esc — same pattern as
  // CategoryPicker: one listener registered on mount, a no-op via the
  // `!menuOpen` guard when already closed, removed on unmount.
  onMount(() => {
    function onDocClick(e: MouseEvent) {
      if (!menuOpen) return;
      const target = e.target as Node | null;
      if (target && !menuWrapperEl?.contains(target)) menuOpen = false;
    }
    function onKeydown(e: KeyboardEvent) {
      if (e.key === "Escape" && menuOpen) menuOpen = false;
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKeydown);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKeydown);
    };
  });

  async function reveal() {
    if (!path) return;
    try {
      await revealItemInDir(path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Every preset goes through the Finder panel with `defaultPath` pre-aimed.
   *  Not ceremony: this is how macOS TCC consent for ~/Downloads and ~/Desktop
   *  is granted through the system's own path, instead of a permission dialog
   *  appearing without context on the first scan. */
  async function pickFolder(defaultPath?: string): Promise<string | null> {
    const picked = await openDialog({
      title: t("watch.picker_title"),
      directory: true,
      multiple: false,
      defaultPath,
    });
    return typeof picked === "string" ? picked : null;
  }

  async function addFrom(defaultPath?: string) {
    menuOpen = false;
    error = null;
    const picked = await pickFolder(defaultPath);
    if (!picked) return;
    try {
      await addWatchedFolder(picked);
      await loadFolders();
      await watch.refresh({ force: true });
    } catch (e) {
      error = folderError(e, "watch.error_add");
    }
  }

  async function addICloud() {
    menuOpen = false;
    error = null;
    const home = await homeDir();
    const target = await join(home, "Library", "Mobile Documents", "com~apple~CloudDocs", "Finoinc");
    // Creating the folder is the feature's ONLY disk write, so it asks — and
    // only when the folder really is missing.
    if (!(await dirExists(target))) {
      const ok = await confirm(t("watch.create_icloud_confirm"), {
        title: t("watch.preset_icloud"),
        okLabel: t("common.create"),
        cancelLabel: t("common.cancel"),
      });
      if (!ok) return;
      try {
        await ensureDir(target);
      } catch (e) {
        error = folderError(e, "watch.error_create_icloud");
        return;
      }
    }
    await addFrom(target);
  }

  // The presets `await` `addFrom` (as the iCloud one already did): without it, a
  // failure before the picker — resolving the folder — would become an unhandled
  // rejection and the screen would show nothing.
  async function addDownloads() {
    try {
      await addFrom(await downloadDir());
    } catch (e) {
      error = folderError(e, "watch.error_add");
    }
  }

  async function addDesktop() {
    try {
      await addFrom(await desktopDir());
    } catch (e) {
      error = folderError(e, "watch.error_add");
    }
  }

  async function enableWatch() {
    error = null;
    const picked = await pickFolder();
    if (!picked) return; // cancelling activates nothing — no zombie state
    try {
      await addWatchedFolder(picked);
      await watch.setEnabled(true);
      await loadFolders();
    } catch (e) {
      error = folderError(e, "watch.error_toggle");
    }
  }

  async function disableWatch() {
    error = null;
    try {
      await watch.setEnabled(false); // keeps the folder list
    } catch (e) {
      error = folderError(e, "watch.error_toggle");
    }
  }

  async function relocate(folder: WatchedFolder) {
    error = null;
    const picked = await pickFolder();
    if (!picked) return;
    try {
      await updateWatchedFolderPath(folder.id, picked);
      await loadFolders();
      await watch.refresh({ force: true });
    } catch (e) {
      error = folderError(e, "watch.error_relocate");
    }
  }

  async function dropFolder(folder: WatchedFolder) {
    error = null;
    try {
      await removeWatchedFolder(folder.id);
      await loadFolders();
    } catch (e) {
      error = folderError(e, "watch.error_remove");
    }
  }

  /** Revealing in the file manager can fail (folder unmounted between render and click) —
   *  same care the database button already had. */
  async function revealFolder(folder: WatchedFolder) {
    error = null;
    try {
      await revealItemInDir(folder.path);
    } catch (e) {
      error = folderError(e, "watch.error_reveal", { fm });
    }
  }

  // Manual scan — the third trigger from spec §5.1, alongside app launch and
  // window focus.
  async function scanNow() {
    error = null;
    try {
      await watch.refresh({ force: true });
      await loadFolders();
    } catch (e) {
      error = folderError(e, "watch.error_scan");
    }
  }

  async function doExport() {
    info = null;
    error = null;
    const target = await saveDialog({
      title: t("settings.save_title"),
      defaultPath: `finoinc-backup-${new Date().toISOString().slice(0, 10)}.db`,
      filters: [{ name: "SQLite DB", extensions: ["db"] }],
    });
    if (!target) return;

    busy = true;
    try {
      const written = await exportBackup(target);
      info = t("settings.backup_saved", { path: written });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function doRestore() {
    info = null;
    error = null;
    const picked = await openDialog({
      title: t("settings.restore_title"),
      multiple: false,
      filters: [{ name: "SQLite DB", extensions: ["db"] }],
    });
    if (!picked || Array.isArray(picked)) return;

    // Restoring REPLACES the database — destructive and irreversible, one of the
    // few places that earns a native alert blocking the way.
    const confirmed = await confirm(t("settings.restore_confirm"), {
      title: t("settings.restore_title"),
      kind: "warning",
      okLabel: t("settings.restore_ok"),
      cancelLabel: t("common.cancel"),
    });
    if (!confirmed) return;

    busy = true;
    try {
      await restoreBackup(picked);
      await message(t("settings.restore_done_msg", { quit: t(`os.quit_${platform}`) }), {
        title: t("settings.restore_done_title"),
        kind: "info",
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Page title={t("settings.title")} width="narrow">
  <!-- ── Idioma ─────────────────────────────────────────────────────────── -->
  <Card title={t("settings.language")}>
    <p class="text-sub text-fg-muted leading-relaxed">{t("settings.language_desc")}
      {#if platform === "mac"}{t("settings.language_menu_note")}{/if}
    </p>
    <LocalePicker />
  </Card>

  <!-- ── Tax-id company lookup ──────────────────────────────────────────── -->
  <!-- Only exists when the locale pack declares both a format and a provider. -->
  {#if enrich?.available}
    <Card title={t("settings.enrich_title", { taxId: enrich.tax_id_name })}>
      <div class="flex items-start justify-between gap-4">
        <div class="flex flex-col gap-1.5 min-w-0">
          <p class="text-sub text-fg-muted leading-relaxed">
            {t("settings.enrich_desc", { taxId: enrich.tax_id_name })}
          </p>
          <!-- Who answers the lookup is visible before opting in. -->
          <p class="text-foot text-fg-subtle flex items-start gap-1.5">
            <span class="mt-px"><Icon name="cloud" size={11.5} /></span>
            <span>{t("settings.enrich_provider", { provider: enrich.provider })}</span>
          </p>
        </div>
        <div class="flex items-center gap-2.5 shrink-0">
          <span class="text-foot text-fg-subtle">
            {enrich.enabled ? t("settings.enrich_on") : t("settings.enrich_off")}
          </span>
          <Switch
            checked={enrich.enabled}
            onChange={toggleEnrich}
            label={t("settings.enrich_title", { taxId: enrich.tax_id_name })}
          />
        </div>
      </div>
    </Card>
  {/if}

  <!-- ── Data freshness notice ──────────────────────────────────────────── -->
  <Card title={t("settings.freshness_title")}>
    <div class="flex items-start justify-between gap-4">
      <p class="text-sub text-fg-muted leading-relaxed min-w-0">
        {t("settings.freshness_desc")}
      </p>
      <div class="flex items-center gap-2.5 shrink-0">
        <span class="text-foot text-fg-subtle">
          {freshness ? t("settings.freshness_on") : t("settings.freshness_off")}
        </span>
        <Switch
          checked={freshness}
          onChange={toggleFreshness}
          label={t("settings.freshness_title")}
        />
      </div>
    </div>
  </Card>

  <!-- ── Automatic import ───────────────────────────────────────────────── -->
  <Card title={t("watch.section_title")}>
    {#snippet actions()}
      {#if watch.enabled}
        <button
          type="button"
          onclick={disableWatch}
          class="text-foot text-fg-subtle hover:text-fg transition-colors duration-[var(--dur-fast)] shrink-0"
        >
          {t("watch.disable")}
        </button>
      {/if}
    {/snippet}

    {#if !watch.enabled}
      <div class="flex flex-col gap-2">
        <div class="text-title3 font-semibold text-fg">{t("watch.pitch_title")}</div>
        <p class="text-sub text-fg-muted leading-relaxed">{t("watch.pitch_body")}</p>
        <p class="text-foot text-fg-subtle leading-relaxed flex items-start gap-1.5 pt-1">
          <span class="mt-px"><Icon name="lock" size={11.5} /></span>
          <span>{t("watch.privacy_note")}</span>
        </p>
      </div>
      <div>
        <Button onclick={enableWatch}>{t("watch.enable_cta")}</Button>
      </div>
    {:else}
      <div class="text-foot text-fg-subtle">{t("watch.folders_label")}</div>
      <div class="card-inset divide-y divide-border-subtle overflow-hidden">
        {#each folders as f (f.id)}
          <div class="group p-3 flex items-start gap-2.5">
            <span
              class="w-6 h-6 shrink-0 grid place-items-center rounded-[var(--radius-sm)] mt-px
                     {f.exists ? 'bg-surface-2 text-fg-subtle' : 'bg-neg/12 text-neg'}"
            >
              <Icon name={f.exists ? "folder" : "triangleAlert"} size={13} />
            </span>
            <div class="flex-1 min-w-0">
              <div class="text-callout font-medium text-fg">{f.label}</div>
              <div class="text-cap text-fg-subtle truncate font-mono" title={f.path}>{f.path}</div>
              {#if !f.exists}
                <div class="text-foot text-neg mt-1">{t("watch.folder_missing")}</div>
                <div class="flex gap-2 mt-1.5">
                  <Button variant="outline" size="sm" onclick={() => relocate(f)}>
                    {t("watch.folder_relocate")}
                  </Button>
                  <Button variant="ghost" size="sm" onclick={() => dropFolder(f)}>
                    {t("watch.folder_remove")}
                  </Button>
                </div>
              {:else}
                <div class="text-cap text-fg-muted mt-0.5">
                  {#if f.imported_count === 0}
                    {t("watch.imported_none")}
                  {:else}
                    {f.imported_count === 1
                      ? t("watch.imported_one", { n: f.imported_count })
                      : t("watch.imported_many", { n: f.imported_count })}
                    {#if f.last_imported_at}
                      · {t("watch.imported_last", { date: f.last_imported_at.slice(0, 10) })}
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>
            {#if f.exists}
              <div
                class="flex gap-1 shrink-0 opacity-0 group-hover:opacity-100 focus-within:opacity-100
                       transition-opacity duration-[var(--dur-fast)]"
              >
                <button
                  type="button"
                  onclick={() => revealFolder(f)}
                  title={t("watch.folder_menu_reveal", { fm })}
                  aria-label={t("watch.folder_menu_reveal", { fm })}
                  class="press w-6 h-6 grid place-items-center rounded-[var(--radius-sm)] text-fg-muted
                         hover:bg-hover hover:text-fg transition-colors duration-[var(--dur-fast)]"
                >
                  <Icon name="externalLink" size={12} />
                </button>
                <button
                  type="button"
                  onclick={() => dropFolder(f)}
                  title={t("watch.folder_menu_remove")}
                  aria-label={t("watch.folder_menu_remove")}
                  class="press w-6 h-6 grid place-items-center rounded-[var(--radius-sm)] text-fg-muted
                         hover:bg-neg/12 hover:text-neg transition-colors duration-[var(--dur-fast)]"
                >
                  <Icon name="x" size={12} stroke={2.2} />
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <div class="relative" bind:this={menuWrapperEl}>
        <Button variant="outline" onclick={() => (menuOpen = !menuOpen)} aria-expanded={menuOpen}>
          <Icon name="plus" size={12} stroke={2.4} />
          {t("watch.add_folder")}
          <Icon name="chevronDown" size={11} stroke={2.2} class="opacity-60" />
        </Button>
        {#if menuOpen}
          <div
            transition:popover={{ origin: "top left" }}
            class="material-pop absolute z-10 mt-1.5 w-72 p-1 text-callout"
            role="menu"
          >
            {#each [...(platform === "mac" ? [{ icon: "cloud" as const, label: t("watch.preset_icloud"), hint: "", fn: addICloud }] : []), { icon: "download" as const, label: t("watch.preset_downloads"), hint: platform === "mac" ? t("watch.preset_downloads_hint") : "", fn: addDownloads }, { icon: "monitor" as const, label: t("watch.preset_desktop"), hint: "", fn: addDesktop }] as item}
              <button
                type="button"
                role="menuitem"
                onclick={item.fn}
                class="w-full text-left px-2 h-7 rounded-[var(--radius-sm)] flex items-center gap-2
                       text-fg-muted hover:bg-accent hover:text-accent-on
                       transition-colors duration-[var(--dur-instant)]"
              >
                <Icon name={item.icon} size={13} />
                <span class="flex-1 truncate">{item.label}</span>
                {#if item.hint}
                  <span class="text-cap opacity-70">{item.hint}</span>
                {/if}
              </button>
            {/each}
            <div class="hairline my-1"></div>
            <button
              type="button"
              role="menuitem"
              onclick={() => addFrom()}
              class="w-full text-left px-2 h-7 rounded-[var(--radius-sm)] flex items-center gap-2
                     text-fg-muted hover:bg-accent hover:text-accent-on
                     transition-colors duration-[var(--dur-instant)]"
            >
              <Icon name="folder" size={13} />
              <span class="flex-1 truncate">{t("watch.preset_other")}</span>
            </button>
          </div>
        {/if}
      </div>

      {#if icloudWaiting > 0}
        <div class="text-foot text-fg-subtle flex items-center gap-1.5">
          <Icon name="cloud" size={12} />
          {icloudWaiting === 1
            ? t("watch.icloud_waiting_one", { n: icloudWaiting })
            : t("watch.icloud_waiting_many", { n: icloudWaiting })}
        </div>
      {/if}

      <div class="flex items-center justify-between pt-2.5 border-t border-border-subtle">
        <!-- Evidence, not a generic status: the real time of the last scan, or
             an acknowledgement that none has happened yet. -->
        <span class="text-cap text-fg-subtle tabular">
          {lastScanAt
            ? t("watch.last_scan_at", { time: formatScan(lastScanAt) })
            : t("watch.last_scan_never")}
        </span>
        <Button variant="ghost" size="sm" onclick={scanNow}>
          <Icon name="rotateCw" size={11.5} />
          {t("watch.folder_menu_scan")}
        </Button>
      </div>
    {/if}
  </Card>

  <!-- ── Database ───────────────────────────────────────────────────────── -->
  <Card title={t("settings.database")}>
    <p class="text-sub text-fg-muted leading-relaxed">{t("settings.database_desc")}</p>
    <div class="card-inset font-mono text-foot text-fg break-all p-2.5 selectable">
      {path ?? t("common.loading")}
    </div>
    <div>
      <Button variant="outline" onclick={reveal} disabled={!path}>
        <Icon name="externalLink" size={12} />
        {t("settings.open_in_folder", { fm })}
      </Button>
    </div>
  </Card>

  <!-- ── Backup ─────────────────────────────────────────────────────────── -->
  <Card title={t("settings.backup")}>
    <p class="text-sub text-fg-muted leading-relaxed">{t("settings.backup_desc")}</p>
    <div class="flex gap-2">
      <Button onclick={doExport} disabled={busy}>
        <Icon name="upload" size={12} />
        {t("settings.export")}
      </Button>
      <Button variant="outline" onclick={doRestore} disabled={busy}>
        <Icon name="download" size={12} />
        {t("settings.restore")}
      </Button>
    </div>
    {#if info}
      <ErrorNote message={info} tone="success" />
    {/if}
    {#if error}
      <ErrorNote message={error} />
    {/if}
  </Card>

  <!-- ── Sobre + atalhos ────────────────────────────────────────────────── -->
  <Card title={t("settings.about")}>
    <div class="text-sub text-fg-muted">
      <strong class="text-fg font-semibold">Finoinc</strong>
      <span class="tabular">{version ? `v${version}` : ""}</span>
      — {t("settings.about_line")}
    </div>
    <!-- The Help menu's GitHub link, for Windows and Linux, which have no
         native menu. Same inline link as the About window. -->
    <button
      type="button"
      onclick={() => void openUrl(GITHUB_URL)}
      class="self-start inline-flex items-center gap-1 text-sub text-accent hover:text-accent-hi
             underline-offset-4 hover:underline transition-colors duration-[var(--dur-fast)]"
    >
      {t("menu.github")}
      <Icon name="arrowUpRight" size={11} stroke={2} />
    </button>

    <!-- Shortcuts in a grid: hunting one inside running prose is work. -->
    <div class="grid grid-cols-3 gap-x-5 gap-y-1.5 pt-1">
      {#each [[`${mod}1`, t("nav.dashboard")], [`${mod}2`, t("nav.transactions")], [`${mod}3`, t("nav.calendar")], [`${mod}4`, t("sidebar.import")], [`${mod}5`, t("nav.categories")], [`${mod}6`, t("nav.rules")], [`${mod}7`, t("nav.suggestions")], [`${mod}8`, t("nav.mcp")], [`${mod}F`, t("settings.shortcut_search")], [`${mod}O`, t("settings.shortcut_open_ofx")]] as [key, label]}
        <div class="flex items-center gap-2 min-w-0">
          <kbd
            class="shrink-0 min-w-[26px] h-5 px-1.5 grid place-items-center rounded-[5px] border border-border
                   bg-surface-2 text-cap font-mono text-fg-muted"
          >
            {key}
          </kbd>
          <span class="text-foot text-fg-subtle truncate">{label}</span>
        </div>
      {/each}
    </div>
  </Card>
</Page>
