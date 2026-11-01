/**
 * Wraps calls so only the most recent one counts. Each call still runs; one
 * that finishes after a newer call started comes back `{ current: false }`,
 * so its result is dropped instead of overwriting fresher data.
 */
export function latestOnly() {
  let seq = 0;
  return async function <T>(fn: () => Promise<T>): Promise<{ current: true; value: T } | { current: false }> {
    const mine = ++seq;
    const value = await fn();
    return mine === seq ? { current: true, value } : { current: false };
  };
}
