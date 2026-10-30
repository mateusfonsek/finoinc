import { describe, expect, it } from "vitest";
import { detectPlatform, modKeyLabel } from "./platform";

describe("detectPlatform", () => {
  it("detect_platform_from_user_agent", () => {
    expect(
      detectPlatform(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko)",
      ),
    ).toBe("mac");
    expect(
      detectPlatform(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 Edg/120.0",
      ),
    ).toBe("windows");
    expect(
      detectPlatform("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)"),
    ).toBe("linux");
    expect(detectPlatform("something else")).toBe("linux");
  });
});

describe("modKeyLabel", () => {
  it("mod_key_label_per_platform", () => {
    expect(modKeyLabel("mac")).toBe("⌘");
    expect(modKeyLabel("windows")).toBe("Ctrl+");
    expect(modKeyLabel("linux")).toBe("Ctrl+");
  });
});
