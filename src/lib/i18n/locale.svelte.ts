import { commands } from "$lib/bindings";
import type { ReversalPhase } from "$lib/ofx/reversals";

// Every folder under /locales/<code>/ is picked up at build time via glob —
// adding a new language is "drop a folder + translate JSON", no code changes.

export type Manifest = {
  code: string;
  name: string;
  flag: string;
  currency: { code: string; locale: string };
  dateLocale: string;
  firstDayOfWeek: number;
  taxId: { name: string; regex: string; provider: string };
};

export type NormRule = { key_prefix: string; badge: string; tone: string };

type Strings = Record<string, unknown>;
type Pack = {
  code: string;
  manifest: Manifest;
  strings: Strings;
  reversals: ReversalPhase[];
  normRules: NormRule[];
  cnpjKeyPrefix: string;
};

const manifestMods = import.meta.glob("/locales/*/manifest.json", { eager: true });
const stringsMods = import.meta.glob("/locales/*/strings.json", { eager: true });
const rulesMods = import.meta.glob("/locales/*/rules.json", { eager: true });

function codeFromPath(path: string): string {
  // "/locales/pt-BR/manifest.json" -> "pt-BR"
  return path.split("/")[2];
}

function pick<T>(mod: unknown): T {
  return ((mod as { default?: T }).default ?? (mod as T)) as T;
}

const packs: Record<string, Pack> = {};
for (const [path, mod] of Object.entries(manifestMods)) {
  const code = codeFromPath(path);
  packs[code] = {
    code,
    manifest: pick<Manifest>(mod),
    strings: packs[code]?.strings ?? {},
    reversals: packs[code]?.reversals ?? [],
    normRules: packs[code]?.normRules ?? [],
    cnpjKeyPrefix: packs[code]?.cnpjKeyPrefix ?? "",
  };
}
for (const [path, mod] of Object.entries(stringsMods)) {
  const code = codeFromPath(path);
  if (packs[code]) packs[code].strings = pick<Strings>(mod);
}
for (const [path, mod] of Object.entries(rulesMods)) {
  const code = codeFromPath(path);
  if (!packs[code]) continue;
  const rules = pick<{
    reversals?: { phases?: ReversalPhase[] };
    normalization?: { rules?: NormRule[]; cnpj_key_prefix?: string };
  }>(mod);
  packs[code].reversals = rules.reversals?.phases ?? [];
  packs[code].normRules = rules.normalization?.rules ?? [];
  packs[code].cnpjKeyPrefix = rules.normalization?.cnpj_key_prefix ?? "";
}

const DEFAULT_LOCALE = "pt-BR";
const STORAGE_KEY = "locale";

/** `app_settings` key gating the app shell behind the first-run language
 *  choice — durable so quitting mid-onboarding doesn't drop the gate. */
export const LOCALE_CHOSEN_KEY = "locale_chosen";

function storageGet(): string | null {
  try {
    if (typeof localStorage !== "undefined" && typeof localStorage.getItem === "function") {
      return localStorage.getItem(STORAGE_KEY);
    }
  } catch {
    // ignore — no/blocked storage
  }
  return null;
}

function storageSet(code: string): void {
  try {
    if (typeof localStorage !== "undefined" && typeof localStorage.setItem === "function") {
      localStorage.setItem(STORAGE_KEY, code);
    }
  } catch {
    // ignore — no/blocked storage
  }
}

function pickInitial(): string {
  const saved = storageGet();
  if (saved && packs[saved]) return saved;
  if (packs[DEFAULT_LOCALE]) return DEFAULT_LOCALE;
  return Object.keys(packs)[0] ?? DEFAULT_LOCALE;
}

/** Matches the webview's OS language against discovered packs: exact code
 *  first (`en-US`), then bare language prefix (`en` matches `en-US`). */
function matchNavigatorLanguage(): string | null {
  if (typeof navigator === "undefined" || !navigator.language) return null;
  const nav = navigator.language;
  if (packs[nav]) return nav;
  const prefix = nav.split("-")[0].toLowerCase();
  const match = Object.keys(packs).find((code) => code.split("-")[0].toLowerCase() === prefix);
  return match ?? null;
}

function lookup(obj: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((o, k) => {
    if (o && typeof o === "object") return (o as Record<string, unknown>)[k];
    return undefined;
  }, obj);
}

function createLocale() {
  let code = $state(pickInitial());

  const stringsOf = (c: string): Strings => packs[c]?.strings ?? {};

  /** Translate a dot-path key with optional `{var}` interpolation.
   *  Falls back to the default locale, then to the key itself. */
  function t(key: string, params?: Record<string, string | number>): string {
    let val = lookup(stringsOf(code), key);
    if (typeof val !== "string" && code !== DEFAULT_LOCALE) {
      val = lookup(stringsOf(DEFAULT_LOCALE), key);
    }
    if (typeof val !== "string") return key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        val = (val as string).replaceAll(`{${k}}`, String(v));
      }
    }
    return val as string;
  }

  return {
    get code() {
      return code;
    },
    get manifest(): Manifest | undefined {
      return packs[code]?.manifest;
    },
    get currencyLocale(): string {
      return packs[code]?.manifest.currency.locale ?? DEFAULT_LOCALE;
    },
    get currencyCode(): string {
      return packs[code]?.manifest.currency.code ?? "BRL";
    },
    get dateLocale(): string {
      return packs[code]?.manifest.dateLocale ?? DEFAULT_LOCALE;
    },
    /** Weekday the calendar grid starts on (`Date.getDay()` value). Sunday when
     *  the pack does not say — `weekdays_short` is Sunday-indexed regardless. */
    get firstDayOfWeek(): number {
      return packs[code]?.manifest.firstDayOfWeek ?? 0;
    },
    /** Reversal-detection phases for the active locale; empty disables it. */
    get reversals(): ReversalPhase[] {
      return packs[code]?.reversals ?? [];
    },
    /** Normalization rules (badge/tone per key_prefix) for the active locale. */
    get normRules(): NormRule[] {
      return packs[code]?.normRules ?? [];
    },
    /** The pack's tax-id key prefix (e.g. `cnpj`, `taxid`) — structural, not a rule. */
    get cnpjKeyPrefix(): string {
      return packs[code]?.cnpjKeyPrefix ?? "";
    },
    get months(): string[] {
      return (lookup(stringsOf(code), "months") as string[]) ?? [];
    },
    get monthsShort(): string[] {
      return (lookup(stringsOf(code), "months_short") as string[]) ?? [];
    },
    get weekdaysShort(): string[] {
      return (lookup(stringsOf(code), "weekdays_short") as string[]) ?? [];
    },
    /** Raw (non-string) value at a dot-path — for arrays/objects like spec lists.
     *  Falls back to the default locale. */
    raw<T>(key: string): T | undefined {
      let val = lookup(stringsOf(code), key);
      if (val === undefined && code !== DEFAULT_LOCALE) {
        val = lookup(stringsOf(DEFAULT_LOCALE), key);
      }
      return val as T | undefined;
    },
    /** All available locales, for a language picker. */
    list(): Manifest[] {
      return Object.values(packs).map((p) => p.manifest);
    },
    t,
    /** Change the active locale: updates the UI now and persists (so the native
     *  menu picks it up on next launch). */
    async set(next: string): Promise<void> {
      if (!packs[next]) return;
      code = next;
      storageSet(next);
      try {
        await commands.setActiveLocale(next);
      } catch {
        // backend not available (e.g. web preview) — localStorage still holds it
      }
    },
    /** Sync from the backend's persisted choice on boot; on a genuine first
     *  run (nothing stored yet), pre-select from the OS language instead of
     *  always landing on the default. Never overrides a stored choice. */
    async init(): Promise<void> {
      const firstRun = storageGet() === null;

      try {
        const active = await commands.getActiveLocale();
        if (active && packs[active]) {
          code = active;
          storageSet(active);
        }
      } catch {
        // ignore — fall back to localStorage/default already chosen
      }

      if (firstRun) {
        const guess = matchNavigatorLanguage();
        if (guess && guess !== code) {
          await this.set(guess);
        }
      }
    },
  };
}

export const locale = createLocale();

/** Convenience: bare `t()` for terse imports. */
export const t = locale.t;
