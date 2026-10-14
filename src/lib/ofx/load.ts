import { OfxReadError } from "./errors";
import { decodeOfxFile } from "./normalize";
import { parseOfx } from "./parse";
import { readFileBytes } from "$lib/api/files";
import type { ParsedOfx } from "./types";

/**
 * Reads an .ofx from disk by path and returns `{ file, parsed }`, reusing the
 * same decode/parse as the file picker. Used by "Open with Finoinc".
 *
 * Fails at two very different stages, and the caller needs to know which: the
 * disk read (`OfxReadError` — transient, retry later) and the parse (a plain
 * error — the content is not OFX, and time will not fix it). Decoding happens
 * over bytes already in memory, so it counts as parse.
 */
export async function loadOfxFromPath(path: string): Promise<{ file: File; parsed: ParsedOfx }> {
  let bytes: Uint8Array;
  try {
    bytes = await readFileBytes(path);
  } catch (e) {
    throw new OfxReadError(path, e);
  }
  const name = path.split(/[\\/]/).pop() || "extrato.ofx";
  const file = new File([bytes as BlobPart], name);
  const content = await decodeOfxFile(file);
  return { file, parsed: parseOfx(content) };
}
