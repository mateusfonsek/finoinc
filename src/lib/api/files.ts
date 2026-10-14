import { commands } from "../bindings";

function unwrap<T>(result: { status: "ok"; data: T } | { status: "error"; error: string }): T {
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}

/** Reads a file's raw bytes (used by OFX drag-and-drop). */
export async function readFileBytes(path: string): Promise<Uint8Array> {
  return new Uint8Array(unwrap(await commands.readFileBytes(path)));
}

/** Drains the .ofx paths opened via Finder ("Open with Finoinc"). */
export async function takePendingOfx(): Promise<string[]> {
  return commands.takePendingOfx();
}
