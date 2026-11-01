import { describe, expect, it } from "vitest";
import { CsvEmptyError, readCsv } from "./read";

const utf8 = (s: string) => new TextEncoder().encode(s);

/** windows-1252 bytes for a Latin-1 string (every char here is < 0x100). */
const cp1252 = (s: string) => Uint8Array.from([...s].map((c) => c.charCodeAt(0)));

describe("readCsv", () => {
  it("detects_comma_semicolon_and_tab", () => {
    for (const src of ["a,b\n1,2", "a;b\n1;2", "a\tb\n1\t2"]) {
      const t = readCsv(utf8(src));
      expect(t.header).toEqual(["a", "b"]);
      expect(t.rows).toEqual([["1", "2"]]);
    }
  });

  it("quoted_fields_keep_delimiters_and_newlines", () => {
    const t = readCsv(utf8('d;desc\n01/01/2026;"PIX; JOÃO\nSILVA"\n02/01/2026;"diz ""oi"""'));
    expect(t.rows[0][1]).toBe("PIX; JOÃO\nSILVA");
    expect(t.rows[1][1]).toBe('diz "oi"');
  });

  it("strips_utf8_bom", () => {
    const bytes = new Uint8Array([0xef, 0xbb, 0xbf, ...utf8("Data,Valor\n1,2")]);
    expect(readCsv(bytes).header[0]).toBe("Data");
  });

  it("falls_back_to_windows_1252", () => {
    const t = readCsv(cp1252("Data;Descrição;Valor\n01/01/2026;Café;-1,00"));
    expect(t.header).toContain("Descrição");
    expect(t.rows[0][1]).toBe("Café");
  });

  it("skips_preamble_lines", () => {
    const t = readCsv(
      utf8(
        "Extrato Conta\nPeríodo: 10/2026\n\nData;Histórico;Valor\n01/10/2026;PIX;-10,00\n02/10/2026;TED;5,00",
      ),
    );
    expect(t.preamble).toBe(3);
    expect(t.header[1]).toBe("Histórico");
    expect(t.rows).toHaveLength(2);
  });

  it("drops_blank_lines_and_crlf", () => {
    const t = readCsv(utf8("a,b\r\n1,2\r\n\r\n"));
    expect(t.rows).toEqual([["1", "2"]]);
  });

  it("empty_or_header_only_throws", () => {
    expect(() => readCsv(utf8(""))).toThrow(CsvEmptyError);
    expect(() => readCsv(utf8("a,b\n"))).toThrow(CsvEmptyError);
  });

  // Some exports end every data row with the delimiter but not the header.
  it("header_without_trailing_delimiter_of_data_rows", () => {
    const t = readCsv(utf8("Data;Valor\r\n01/01/2026;-10,50;\r\n02/01/2026;5,00;\r\n"));
    expect(t.header).toEqual(["Data", "Valor"]);
    expect(t.rows).toHaveLength(2);
    expect(t.rows[0][0]).toBe("01/01/2026");
  });
});
