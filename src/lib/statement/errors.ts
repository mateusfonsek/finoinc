/** Failure to *read the bytes* of a statement (`.ofx`/`.csv`) from disk — distinct from "the
 *  bytes are not a valid statement".
 *
 *  The distinction matters because the two have opposite consequences in a
 *  watched folder: invalid content is permanent (the file is marked `invalid`
 *  and never notified again), while failing to read is transient — an iCloud
 *  placeholder evicted back to a stub, a file moved between the scan and the
 *  read, permission denied. Treating the second as the first would bury the
 *  statement forever, since the key is the content hash.
 *
 *  Lives in its own module (not `load.ts`) so tests mocking `$lib/ofx/load`
 *  still compare against the real class.
 */
export class StatementReadError extends Error {
  readonly path: string;

  constructor(path: string, cause: unknown) {
    super(cause instanceof Error ? cause.message : String(cause));
    this.name = "StatementReadError";
    this.path = path;
  }
}
