<script lang="ts">
  import { link, router } from "svelte-spa-router";
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { IconName } from "$lib/components/ui/icons";
  import logoUrl from "$lib/assets/logo.png";
  import { locale } from "$lib/i18n/locale.svelte";
  import { watch } from "$lib/stores/watch.svelte";

  const t = locale.t;

  let { onAbout }: { onAbout: () => void } = $props();

  type NavItem = { href: string; labelKey: string; sectionKey: string; icon: IconName };

  // One symbol per destination: in a macOS sidebar the icon is what the eye
  // finds first; the label confirms.
  const navItems: NavItem[] = [
    { sectionKey: "overview", href: "/dashboard", labelKey: "dashboard", icon: "chartPie" },
    { sectionKey: "overview", href: "/transactions", labelKey: "transactions", icon: "arrowLeftRight" },
    { sectionKey: "overview", href: "/calendar", labelKey: "calendar", icon: "calendar" },
    { sectionKey: "import", href: "/import", labelKey: "import", icon: "fileDown" },
    { sectionKey: "organize", href: "/categories", labelKey: "categories", icon: "tags" },
    { sectionKey: "organize", href: "/rules", labelKey: "rules", icon: "wandSparkles" },
    { sectionKey: "organize", href: "/suggestions", labelKey: "suggestions", icon: "lightbulb" },
  ];

  const sections = ["overview", "import", "organize"];

  function isActive(href: string, current: string): boolean {
    if (href === "/dashboard" && (current === "/" || current === "/dashboard")) return true;
    return current === href;
  }

  let settingsActive = $derived(router.location === "/settings");
  let mcpActive = $derived(router.location === "/mcp");
</script>

<!-- The top strip is the window drag area: with `titleBarStyle: Overlay` the
     macOS traffic lights float over this column, as in Mail, Notes and Finder.
     The top padding is the space reserved for them. -->
<aside
  data-tauri-drag-region="deep"
  class="bg-sidebar border-r border-border-subtle flex flex-col px-2.5 pb-3 select-none overflow-hidden"
  style="padding-top: var(--titlebar-h)"
>
  <button
    type="button"
    onclick={onAbout}
    title={t("sidebar.about_title")}
    aria-label={t("sidebar.about_title")}
    class="press-sm flex items-center gap-2.5 px-2 py-1.5 mb-1 rounded-[var(--radius-md)] text-left
           hover:bg-hover transition-colors duration-[var(--dur-fast)]"
  >
    <img
      src={logoUrl}
      alt=""
      class="w-7 h-7 rounded-[7px] shrink-0 shadow-[var(--shadow-raised)]"
      draggable="false"
    />
    <span class="flex flex-col min-w-0">
      <span class="text-callout font-semibold tracking-[-0.012em] text-fg truncate">Finoinc</span>
      <span class="text-cap2 text-fg-subtle truncate">{t("sidebar.tagline")}</span>
    </span>
  </button>

  <nav class="flex-1 overflow-y-auto -mx-0.5 px-0.5">
    {#each sections as section}
      <div class="mt-3.5 first:mt-1.5 flex flex-col gap-px">
        <h2 class="text-cap font-semibold text-fg-subtle px-2 pb-1">
          {t("sidebar." + section)}
        </h2>
        {#each navItems.filter((i) => i.sectionKey === section) as item}
          {@const active = isActive(item.href, router.location)}
          <a
            use:link
            href={item.href}
            aria-current={active ? "page" : undefined}
            class="group flex items-center gap-2.5 px-2 h-7 rounded-[var(--radius-md)] text-callout font-medium
                   transition-colors duration-[var(--dur-fast)] ease-[var(--ease-snap)]
                   {active
              ? 'bg-accent text-accent-on'
              : 'text-fg-muted hover:bg-hover hover:text-fg'}"
          >
            <Icon
              name={item.icon}
              size={15}
              stroke={active ? 2 : 1.7}
              class={active ? "" : "opacity-75 group-hover:opacity-100"}
            />
            <span class="flex-1 truncate">{t("nav." + item.labelKey)}</span>
            {#if item.labelKey === "import" && watch.pendingCount > 0}
              {@const badgeLabel =
                watch.pendingCount === 1
                  ? t("watch.badge_pending_one", { n: watch.pendingCount })
                  : t("watch.badge_pending_many", { n: watch.pendingCount })}
              <!-- Alone, the number says nothing to a screen reader: just a
                   loose digit in the middle of the menu. -->
              <span
                class="min-w-[17px] h-[17px] px-1 rounded-full grid place-items-center tabular text-cap2 font-semibold
                       {active ? 'bg-accent-on/25 text-accent-on' : 'bg-accent text-accent-on'}"
                aria-label={badgeLabel}
                title={badgeLabel}
              >
                {watch.pendingCount}
              </span>
            {/if}
          </a>
        {/each}
      </div>
    {/each}
  </nav>

  <a
    use:link
    href="/mcp"
    aria-current={mcpActive ? "page" : undefined}
    class="group flex items-center gap-2.5 px-2 h-7 mt-2 rounded-[var(--radius-md)] text-callout font-medium
           transition-colors duration-[var(--dur-fast)] ease-[var(--ease-snap)]
           {mcpActive ? 'bg-accent text-accent-on' : 'text-fg-muted hover:bg-hover hover:text-fg'}"
  >
    <Icon
      name="plug"
      size={15}
      stroke={mcpActive ? 2 : 1.7}
      class={mcpActive ? "" : "opacity-75 group-hover:opacity-100"}
    />
    <span class="flex-1 truncate">{t("nav.mcp")}</span>
  </a>

  <a
    use:link
    href="/settings"
    aria-current={settingsActive ? "page" : undefined}
    class="group flex items-center gap-2.5 px-2 h-7 rounded-[var(--radius-md)] text-callout font-medium
           transition-colors duration-[var(--dur-fast)] ease-[var(--ease-snap)]
           {settingsActive ? 'bg-accent text-accent-on' : 'text-fg-muted hover:bg-hover hover:text-fg'}"
  >
    <Icon
      name="settings"
      size={15}
      stroke={settingsActive ? 2 : 1.7}
      class={settingsActive ? "" : "opacity-75 group-hover:opacity-100"}
    />
    <span class="flex-1 truncate">{t("nav.settings")}</span>
  </a>
</aside>
