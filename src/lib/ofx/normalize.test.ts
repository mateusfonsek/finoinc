import { describe, it, expect } from "vitest";
import { detectEncoding, normalizeTransactionText } from "./normalize";

describe("detectEncoding", () => {
  it("reads the encoding of an OFX 2.x XML declaration", () => {
    expect(detectEncoding('<?xml version="1.0" encoding="UTF-8"?>\n<?OFX OFXHEADER="200"?>')).toBe("utf-8");
  });

  it("detects CHARSET:1252 as windows-1252", () => {
    const header = `OFXHEADER:100
DATA:OFXSGML
VERSION:102
CHARSET:1252`;
    expect(detectEncoding(header)).toBe("windows-1252");
  });

  it("detects CHARSET:8859-1 as iso-8859-1", () => {
    expect(detectEncoding("CHARSET:8859-1")).toBe("iso-8859-1");
  });

  it("detects ENCODING:UTF-8", () => {
    expect(detectEncoding("ENCODING:UTF-8")).toBe("utf-8");
  });

  it("falls back to windows-1252 when ENCODING:USASCII is declared", () => {
    expect(detectEncoding("ENCODING:USASCII")).toBe("windows-1252");
  });

  it("defaults to windows-1252 when no header info is present", () => {
    expect(detectEncoding("")).toBe("windows-1252");
  });
});

describe("normalizeTransactionText", () => {
  it("collapses whitespace and trims", () => {
    expect(normalizeTransactionText("  SUPER\tMERCADO   ABC  ")).toBe(
      "SUPER MERCADO ABC",
    );
  });

  it("handles empty input", () => {
    expect(normalizeTransactionText("")).toBe("");
  });

  it("preserves Portuguese accents", () => {
    expect(normalizeTransactionText("Saída de R$ 50")).toBe("Saída de R$ 50");
  });
});
