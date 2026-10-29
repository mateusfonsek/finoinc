/**
 * Decode an OFX file's bytes into a string using the encoding declared in the
 * OFX header (or sensible fallback for Brazilian banks).
 */
export async function decodeOfxFile(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const headerAscii = new TextDecoder("ascii", { fatal: false }).decode(
    buf.slice(0, Math.min(buf.byteLength, 512)),
  );
  const encoding = detectEncoding(headerAscii);
  return new TextDecoder(encoding, { fatal: false }).decode(buf);
}

/**
 * Inspect the OFX header lines to find the declared CHARSET.
 * Returns a TextDecoder-compatible encoding name.
 */
export function detectEncoding(header: string): string {
  const charsetMatch = header.match(/^CHARSET:\s*(\S+)/im);
  const encodingMatch = header.match(/^ENCODING:\s*(\S+)/im);

  const charset = charsetMatch?.[1]?.toLowerCase();
  const encoding = encodingMatch?.[1]?.toLowerCase();

  // OFX 2.x is XML: its declaration carries the encoding, and XML defaults
  // to UTF-8 when it says nothing.
  const xml = header.match(/<\?xml[^>]*>/i);
  if (xml) {
    const declared = xml[0].match(/encoding\s*=\s*["']([^"']+)["']/i)?.[1]?.toLowerCase();
    if (!declared || declared === "utf-8") return "utf-8";
    if (declared === "windows-1252" || declared === "iso-8859-1") return declared;
  }

  if (charset === "utf-8" || encoding === "utf-8") return "utf-8";
  if (charset === "1252" || charset === "windows-1252") return "windows-1252";
  if (charset === "8859-1" || charset === "iso-8859-1") return "iso-8859-1";
  if (encoding === "usascii" || encoding === "us-ascii") return "windows-1252";

  // Brazilian banks frequently lie or omit; windows-1252 is the safest
  // superset of ASCII for Latin Portuguese accents.
  return "windows-1252";
}

/**
 * Apply bank-specific text fixups after parsing.
 * Currently a passthrough — extension point for fase 2+ when concrete real-world
 * quirks surface.
 */
export function normalizeTransactionText(description: string): string {
  return description.replace(/\s+/g, " ").trim();
}
