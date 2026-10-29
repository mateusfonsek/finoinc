import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { parseOfx } from "./parse";

const __dirname = dirname(fileURLToPath(import.meta.url));
const itauFixture = readFileSync(
  join(__dirname, "__fixtures__/itau-minimal.ofx"),
  "utf-8",
);

describe("parseOfx — Itaú minimal fixture", () => {
  const result = parseOfx(itauFixture);

  it("detects Itaú as the bank", () => {
    expect(result.account.bank).toBe("itau");
  });

  it("extracts ACCTID and BANKID", () => {
    expect(result.account.ofxAcctid).toBe("56789-0");
    expect(result.account.ofxBankid).toBe("0341");
  });

  it("formats a sensible display name", () => {
    expect(result.account.displayName).toContain("Itau");
    expect(result.account.displayName).toContain("1234");
    expect(result.account.displayName).toContain("56789-0");
  });

  it("extracts 3 transactions", () => {
    expect(result.transactions).toHaveLength(3);
  });

  it("each transaction has fitid, date, amount, description", () => {
    for (const t of result.transactions) {
      expect(t.fitid).toMatch(/^ITAU\d+$/);
      expect(t.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      expect(typeof t.amount).toBe("string");
      expect(t.description.length).toBeGreaterThan(0);
    }
  });

  it("summary aggregates in/out/net correctly", () => {
    expect(result.summary.totalIn).toBe("3500.00");
    expect(result.summary.totalOut).toBe("79.90");
    expect(result.summary.net).toBe("3420.10");
  });

  it("summary captures date range", () => {
    expect(result.summary.earliest).toBe("2026-03-05");
    expect(result.summary.latest).toBe("2026-03-15");
  });
});

describe("parseOfx — FITID duplicated within one file (Nubank card)", () => {
  const ofx = `OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE
<OFX>
<SIGNONMSGSRSV1>
<SONRS>
<STATUS>
<CODE>0</CODE>
<SEVERITY>INFO</SEVERITY>
</STATUS>
<DTSERVER>20260518132022[0:GMT]</DTSERVER>
<LANGUAGE>POR</LANGUAGE>
<FI>
<ORG>NU PAGAMENTOS S.A.</ORG>
<FID>260</FID>
</FI>
</SONRS>
</SIGNONMSGSRSV1>
<CREDITCARDMSGSRSV1>
<CCSTMTTRNRS>
<TRNUID>1001</TRNUID>
<STATUS>
<CODE>0</CODE>
<SEVERITY>INFO</SEVERITY>
</STATUS>
<CCSTMTRS>
<CURDEF>BRL</CURDEF>
<CCACCTFROM>
<ACCTID>00000000-0000-4000-8000-000000000001</ACCTID>
</CCACCTFROM>
<BANKTRANLIST>
<DTSTART>20260318000000[-3:BRT]</DTSTART>
<DTEND>20260418000000[-3:BRT]</DTEND>
<STMTTRN>
<TRNTYPE>CREDIT</TRNTYPE>
<DTPOSTED>20260318000000[-3:BRT]</DTPOSTED>
<TRNAMT>3.75</TRNAMT>
<FITID>SHARED-FITID</FITID>
<MEMO>IOF de "Acme.Ai"</MEMO>
</STMTTRN>
<STMTTRN>
<TRNTYPE>CREDIT</TRNTYPE>
<DTPOSTED>20260318000000[-3:BRT]</DTPOSTED>
<TRNAMT>108.14</TRNAMT>
<FITID>SHARED-FITID</FITID>
<MEMO>Estorno de "Acme.Ai" (Acme.Ai)</MEMO>
</STMTTRN>
<STMTTRN>
<TRNTYPE>DEBIT</TRNTYPE>
<DTPOSTED>20260318000000[-3:BRT]</DTPOSTED>
<TRNAMT>-47.45</TRNAMT>
<FITID>UNIQUE-FITID</FITID>
<MEMO>Hubla Parcela 3/12</MEMO>
</STMTTRN>
</BANKTRANLIST>
<LEDGERBAL>
<BALAMT>0.00</BALAMT>
<DTASOF>20260418000000[-3:BRT]</DTASOF>
</LEDGERBAL>
</CCSTMTRS>
</CCSTMTTRNRS>
</CREDITCARDMSGSRSV1>
</OFX>`;

  const r = parseOfx(ofx);

  it("keeps every transaction sharing a duplicated FITID", () => {
    expect(r.transactions).toHaveLength(3);
  });

  it("the first occurrence keeps the original FITID; the rest get a #N suffix", () => {
    expect(r.transactions[0].fitid).toBe("SHARED-FITID");
    expect(r.transactions[1].fitid).toBe("SHARED-FITID#2");
    expect(r.transactions[2].fitid).toBe("UNIQUE-FITID");
  });

  it("every FITID ends up unique, which the keyed {#each} and the DB UNIQUE both require", () => {
    const ids = r.transactions.map((t) => t.fitid);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

/** A card statement whose rows all carry the same FITID, as some banks emit. */
function constantFitidStatement(rows: [string, string][]): string {
  const txs = rows
    .map(([date, amount]) => `<STMTTRN>
<TRNTYPE>DEBIT</TRNTYPE>
<DTPOSTED>${date.replaceAll("-", "")}000000[-3:BRT]</DTPOSTED>
<TRNAMT>${amount}</TRNAMT>
<FITID>0</FITID>
<MEMO>PADARIA</MEMO>
</STMTTRN>`)
    .join("\n");
  return `OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE
<OFX>
<CREDITCARDMSGSRSV1>
<CCSTMTTRNRS>
<CCSTMTRS>
<CURDEF>BRL</CURDEF>
<CCACCTFROM>
<ACCTID>00000000-0000-4000-8000-000000000002</ACCTID>
</CCACCTFROM>
<BANKTRANLIST>
<DTSTART>20260301000000[-3:BRT]</DTSTART>
<DTEND>20260331000000[-3:BRT]</DTEND>
${txs}
</BANKTRANLIST>
</CCSTMTRS>
</CCSTMTTRNRS>
</CREDITCARDMSGSRSV1>
</OFX>`;
}

describe("parseOfx — FITID reused across days", () => {
  // The same row must get the same FITID in any statement that contains it,
  // or re-importing an overlapping statement inserts it twice.
  it("gives a row the same FITID in two overlapping statements", () => {
    const a = parseOfx(constantFitidStatement([["2026-03-03", "-10.00"], ["2026-03-09", "-20.00"]]));
    const b = parseOfx(constantFitidStatement([["2026-03-09", "-20.00"], ["2026-03-12", "-30.00"]]));
    const inA = a.transactions.find((t) => t.date === "2026-03-09")!.fitid;
    const inB = b.transactions.find((t) => t.date === "2026-03-09")!.fitid;
    expect(inA).toBe(inB);
  });

  it("still keeps every FITID in the file unique", () => {
    const r = parseOfx(constantFitidStatement([
      ["2026-03-03", "-10.00"], ["2026-03-09", "-20.00"], ["2026-03-09", "-20.00"],
    ]));
    const ids = r.transactions.map((t) => t.fitid);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
