import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { commands, type UpdateInstallKind } from "../bindings";

export type { DownloadEvent, Update };

/** `app_settings` key for automatic update checks. Unset means OFF: checking
 *  contacts GitHub, and nothing in Finoinc talks to the network unasked. */
export const AUTO_CHECK_KEY = "update_auto_check";

/** The only network request of this feature: reads `latest.json` from the
 *  latest GitHub release. `null` when this is already the newest version. */
export function checkForUpdate(): Promise<Update | null> {
  return check();
}

export function installKind(): Promise<UpdateInstallKind> {
  return commands.updateInstallKind();
}

export function prepareForUpdate(): Promise<void> {
  return commands.prepareForUpdate();
}

export function relaunchApp(): Promise<void> {
  return relaunch();
}

export async function readAutoCheck(): Promise<boolean> {
  const r = await commands.getAppSetting(AUTO_CHECK_KEY);
  return r.status === "ok" && r.data === "1";
}

export async function writeAutoCheck(on: boolean): Promise<void> {
  const r = await commands.setAppSetting(AUTO_CHECK_KEY, on ? "1" : "0");
  if (r.status === "error") throw new Error(r.error);
}
