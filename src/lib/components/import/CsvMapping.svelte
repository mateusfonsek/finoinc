<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { formatMoney } from "$lib/format/money";
  import { locale } from "$lib/i18n/locale.svelte";
  import { mappingDraft, previewRows, validateDraft, type MappingDraft, type Role } from "$lib/csv/mapping";
  import type { CsvFormat } from "$lib/csv/formats";
  import type { CsvTable } from "$lib/csv/read";

  const t = locale.t;

  type Props = {
    table: CsvTable;
    fileName: string;
    busy?: boolean;
    onsave: (draft: MappingDraft) => void;
    oncancel: () => void;
  };

  let { table, fileName, busy = false, onsave, oncancel }: Props = $props();

  // svelte-ignore state_referenced_locally — the file seeds the draft once; edits are the user's.
  let draft = $state<MappingDraft>(mappingDraft(table, fileName));

  const ROLES: Role[] = ["ignore", "date", "description", "amount", "inflow", "outflow", "bankId"];
  const DATE_FORMATS: CsvFormat["dateFormat"][] = ["dd/mm/yyyy", "yyyy-mm-dd", "mm/dd/yyyy"];

  // Validation and preview follow every change, so a wrong pick shows up
  // next to the column that caused it — not after pressing save.
  let errors = $derived(validateDraft(draft, table));
  let columnError = $derived(new Map(errors.filter((e) => e.column >= 0).map((e) => [e.column, e.message])));
  let globalErrors = $derived(errors.filter((e) => e.column < 0));
  let preview = $derived(previewRows(draft, table));
  let sampleRows = $derived(table.rows.slice(0, 5));

  function setRole(i: number, role: Role) {
    draft.roles[i] = role;
  }

  function save() {
    if (errors.length === 0 && !busy) onsave($state.snapshot(draft));
  }
</script>

<div class="card p-4 flex flex-col gap-4">
  <div class="flex flex-col gap-0.5">
    <h3 class="text-title3 font-semibold text-fg">{t("import.mapping.title")}</h3>
    <p class="text-sub text-fg-muted leading-relaxed">{t("import.mapping.hint")}</p>
  </div>

  <div class="overflow-x-auto -mx-4 px-4">
    <table class="w-full min-w-max text-sub">
      <thead>
        <tr>
          {#each table.header as name, i (i)}
            <th class="text-left align-top px-2 pb-2 font-normal min-w-[120px]">
              <div class="text-cap font-semibold text-fg-subtle truncate mb-1" title={name}>{name || "—"}</div>
              <select
                class="field w-full"
                value={draft.roles[i]}
                aria-label={t("import.mapping.column_role", { name: name || String(i + 1) })}
                aria-invalid={columnError.has(i)}
                onchange={(e) => setRole(i, (e.currentTarget as HTMLSelectElement).value as Role)}
              >
                {#each ROLES as role}
                  <option value={role}>{t("import.mapping.role_" + role)}</option>
                {/each}
              </select>
              {#if columnError.has(i)}
                <div class="text-cap text-neg mt-1 whitespace-normal">{t(columnError.get(i)!)}</div>
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each sampleRows as row, r (r)}
          <tr class="border-t border-border-subtle">
            {#each table.header as _, i (i)}
              <td
                class="px-2 py-1.5 tabular truncate max-w-[220px]
                       {draft.roles[i] === 'ignore' ? 'text-fg-faint' : 'text-fg'}"
              >
                {row[i] ?? ""}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <div class="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3 items-end">
    <label class="flex flex-col gap-1">
      <span class="text-cap text-fg-subtle">{t("import.mapping.name")}</span>
      <input class="field" bind:value={draft.name} />
    </label>
    <label class="flex flex-col gap-1">
      <span class="text-cap text-fg-subtle">{t("import.mapping.kind")}</span>
      <select class="field" bind:value={draft.kind}>
        <option value="checking">{t("import.checking")}</option>
        <option value="credit_card">{t("import.credit_card")}</option>
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="text-cap text-fg-subtle">{t("import.mapping.date_format")}</span>
      <select class="field" bind:value={draft.dateFormat}>
        {#each DATE_FORMATS as f}
          <option value={f}>{t("import.mapping.date_" + f.replaceAll("/", "_").replaceAll("-", "_"))}</option>
        {/each}
      </select>
    </label>
    <div class="flex items-center gap-2 h-7">
      <Switch
        checked={draft.invertSign}
        onChange={(v) => (draft.invertSign = v)}
        label={t("import.mapping.invert_sign")}
      />
      <span class="text-sub text-fg" title={t("import.mapping.invert_sign_title")}>
        {t("import.mapping.invert_sign")}
      </span>
    </div>
  </div>

  <div class="rounded-[var(--radius-md)] bg-surface-2 px-3 py-2.5 flex flex-col gap-1.5">
    <div class="text-cap font-semibold text-fg-subtle">{t("import.mapping.preview")}</div>
    {#if preview.length > 0}
      {#each preview as tx, i (i)}
        <div class="flex items-center gap-3 text-sub">
          <span class="tabular text-fg-subtle w-[84px] shrink-0">{tx.date}</span>
          <span class="text-fg truncate flex-1">{tx.description}</span>
          <span class="tabular font-medium {Number(tx.amount) >= 0 ? 'text-pos' : 'text-fg'}">
            {formatMoney(tx.amount)}
          </span>
        </div>
      {/each}
    {:else}
      {#each globalErrors as e (e.message)}
        <div class="flex items-center gap-1.5 text-sub text-fg-muted">
          <Icon name="circleAlert" size={13} />
          {t(e.message)}
        </div>
      {:else}
        <div class="text-sub text-fg-muted">{t("import.mapping.preview_empty")}</div>
      {/each}
    {/if}
  </div>

  <div class="flex justify-end gap-2">
    <Button variant="ghost" onclick={oncancel} disabled={busy}>{t("common.cancel")}</Button>
    <Button onclick={save} disabled={errors.length > 0 || busy}>{t("import.mapping.save")}</Button>
  </div>
</div>
