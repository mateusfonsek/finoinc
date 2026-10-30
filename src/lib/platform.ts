export type Platform = "mac" | "windows" | "linux";

// The webview's user agent names the OS on every platform Tauri supports, so
// no OS plugin is needed. Anything that is neither macOS nor Windows is
// treated as Linux, the only other desktop target.
export function detectPlatform(userAgent: string): Platform {
  if (/Mac/.test(userAgent)) return "mac";
  if (/Windows/.test(userAgent)) return "windows";
  return "linux";
}

export const platform: Platform = detectPlatform(
  typeof navigator === "undefined" ? "" : navigator.userAgent,
);

/** How shortcut hints name the modifier: the shortcuts accept both ⌘ and Ctrl,
 *  but each OS should read its own. */
export function modKeyLabel(p: Platform = platform): string {
  return p === "mac" ? "⌘" : "Ctrl+";
}
