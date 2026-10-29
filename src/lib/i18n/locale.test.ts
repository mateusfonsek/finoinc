import { describe, it, expect, vi, beforeEach } from "vitest";

const commands = vi.hoisted(() => ({
  getActiveLocale: vi.fn(),
  setActiveLocale: vi.fn(),
  getAppSetting: vi.fn(),
}));
vi.mock("$lib/bindings", () => ({ commands }));

import { locale } from "./locale.svelte";

describe("locale.init", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    });
    vi.clearAllMocks();
    commands.setActiveLocale.mockResolvedValue({ status: "ok", data: null });
    vi.spyOn(navigator, "language", "get").mockReturnValue("en-US");
  });

  // The webview's storage is empty after the app identifier changed, but the
  // language the user picked is still in the database.
  it("keeps a language the user already chose even when webview storage is empty", async () => {
    commands.getActiveLocale.mockResolvedValue("pt-BR");
    commands.getAppSetting.mockResolvedValue({ status: "ok", data: "1" });

    await locale.init();

    expect(locale.code).toBe("pt-BR");
    expect(commands.setActiveLocale).not.toHaveBeenCalled();
  });

  it("pre-selects the OS language on a genuine first run", async () => {
    commands.getActiveLocale.mockResolvedValue("pt-BR");
    commands.getAppSetting.mockResolvedValue({ status: "ok", data: null });

    await locale.init();

    expect(locale.code).toBe("en-US");
  });
});
