import type { UpdateInstallKind } from "$lib/bindings";

/** What the Updates section shows. `available` carries the last install
 *  error so a failed download falls back to the same buttons, with the
 *  reason next to them. */
export type UpdateView =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up_to_date" }
  | { kind: "available"; version: string; installable: boolean; error: string | null }
  | { kind: "downloading"; version: string; progress: number | null }
  | { kind: "error"; message: string };

export function viewFromCheck(
  found: { version: string } | null,
  kind: UpdateInstallKind,
): UpdateView {
  if (!found) return { kind: "up_to_date" };
  return {
    kind: "available",
    version: found.version,
    installable: kind === "installable",
    error: null,
  };
}

/** 0..1, or null when the server sent no length — the bar then shows motion
 *  without pretending to know how far along it is. */
export function downloadProgress(downloaded: number, total: number | null): number | null {
  if (!total) return null;
  return Math.min(1, downloaded / total);
}

export function releaseUrl(version: string): string {
  return `https://github.com/MateusFonseK/finoinc/releases/tag/v${version}`;
}
