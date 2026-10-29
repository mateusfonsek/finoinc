import { Ofx } from "ofx-data-extractor";
import { t } from "$lib/i18n/locale.svelte";
import type {
  ParsedAccount,
  ParsedOfx,
  ParsedSummary,
  ParsedTransaction,
} from "./types";

/**
 * Parse raw OFX text into our normalized shape. Throws on malformed input.
 *
 * Supports two layouts:
 *   - Checking: BANKMSGSRSV1 / STMTTRNRS / STMTRS / BANKACCTFROM
 *   - Credit card: CREDITCARDMSGSRSV1 / CCSTMTTRNRS / CCSTMTRS / CCACCTFROM
 *
 * From `ofx-data-extractor@1.5.0`:
 *   - `.getBankTransferList()`        — checking-account transactions
 *   - `.getCreditCardTransferList()`  — credit-card transactions
 */
export function parseOfx(content: string): ParsedOfx {
  const ofx = new Ofx(content);
  const structure = ofx.getContent();

  const ofxNode = structure?.OFX ?? {};
  const signon = ofxNode?.SIGNONMSGSRSV1?.SONRS ?? {};
  const fi = (signon?.FI ?? {}) as Record<string, unknown>;
  const fid = (fi["FID"] as string | undefined) ?? null;
  const org = (fi["ORG"] as string | undefined) ?? null;

  // Detects a card by the presence of CREDITCARDMSGSRSV1. If both ever exist,
  // BANK wins (expected case: Nubank OFX carries only one of them).
  const ccNode =
    (ofxNode as Record<string, unknown>)?.CREDITCARDMSGSRSV1 as
      | Record<string, unknown>
      | undefined;
  const bankNode = ofxNode?.BANKMSGSRSV1?.STMTTRNRS?.STMTRS ?? {};
  const hasBank = !!bankNode?.BANKACCTFROM;
  const isCreditCard = !hasBank && !!ccNode;

  let acctid: string | null = null;
  let branchid: string | null = null;
  let bankid: string | null = null;
  let rawTxs: Array<Record<string, unknown>> = [];

  if (isCreditCard) {
    const ccStmt =
      ((ccNode as { CCSTMTTRNRS?: { CCSTMTRS?: Record<string, unknown> } })
        ?.CCSTMTTRNRS?.CCSTMTRS as Record<string, unknown> | undefined) ?? {};
    const ccAcct =
      (ccStmt?.CCACCTFROM as Record<string, unknown> | undefined) ?? {};
    acctid = (ccAcct["ACCTID"] as string | undefined) ?? null;
    // A card has neither branchid nor bankid in CCACCTFROM.
    try {
      rawTxs = ofx.getCreditCardTransferList() as Array<Record<string, unknown>>;
    } catch {
      rawTxs = [];
    }
  } else {
    const bankAcct = (bankNode?.BANKACCTFROM ?? {}) as Record<string, unknown>;
    branchid = (bankAcct["BRANCHID"] as string | undefined) ?? null;
    bankid = (bankAcct["BANKID"] as string | undefined) ?? null;
    acctid = (bankAcct["ACCTID"] as string | undefined) ?? null;
    rawTxs = ofx.getBankTransferList() as Array<Record<string, unknown>>;
  }

  const bank = detectBank({ fid, org, bankid });
  const type: "checking" | "credit_card" = isCreditCard ? "credit_card" : "checking";
  const displayName = formatDisplayName(bank, branchid, acctid, type);

  const account: ParsedAccount = {
    bank,
    ofxAcctid: acctid,
    ofxBankid: bankid,
    ofxFid: fid,
    displayName,
    type,
  };

  // Some banks reuse one FITID across distinct transactions. Nubank does it
  // within a day (the IOF fee and its purchase); others repeat a constant
  // FITID on every row. Each row still needs an id unique in the file (the
  // keyed `{#each}`, the selection) and the SAME id in every statement that
  // contains it, or re-importing an overlapping statement inserts it twice.
  // So repeats are numbered within their day ("#2", "#3" — what Nubank rows
  // have always been stored as), and a FITID that recurs on other days also
  // carries its date.
  const dated = rawTxs.map((t) => {
    const rawDate = (t.DTPOSTED as string | undefined) ?? "";
    return { t, date: isIsoDate(rawDate) ? rawDate : parseOfxDate(rawDate) };
  });
  const daysOf = new Map<string, Set<string>>();
  for (const { t, date } of dated) {
    const raw = t.FITID as string | undefined;
    if (raw) daysOf.set(raw, (daysOf.get(raw) ?? new Set()).add(date));
  }
  const seenThatDay = new Map<string, number>();
  const transactions: ParsedTransaction[] = dated.map(({ t, date }) => {
    const rawFitid = (t.FITID as string | undefined) ?? null;
    let fitid = rawFitid;
    if (rawFitid) {
      if (daysOf.get(rawFitid)!.size > 1) fitid = `${rawFitid}#${date}`;
      const dayKey = `${rawFitid}|${date}`;
      const count = seenThatDay.get(dayKey) ?? 0;
      if (count > 0) fitid = `${fitid}#${count + 1}`;
      seenThatDay.set(dayKey, count + 1);
    }
    return {
      fitid,
      date,
      amount: String(t.TRNAMT ?? "0"),
      description: String(t.MEMO ?? t.NAME ?? ""),
    };
  });

  const summary = computeSummary(transactions);

  return { account, transactions, summary };
}

/** Returns true if the string is already in YYYY-MM-DD format. */
function isIsoDate(raw: string): boolean {
  return /^\d{4}-\d{2}-\d{2}/.test(raw);
}

/** Parse raw OFX timestamp (YYYYMMDDHHmmss...) → YYYY-MM-DD. */
function parseOfxDate(raw: string): string {
  const m = raw.match(/^(\d{4})(\d{2})(\d{2})/);
  if (!m) return "";
  return `${m[1]}-${m[2]}-${m[3]}`;
}

function detectBank(meta: {
  fid: string | null;
  org: string | null;
  bankid: string | null;
}): string {
  const fingerprint = [meta.fid, meta.org, meta.bankid]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  if (/itau|341/.test(fingerprint)) return "itau";
  if (/bradesco|237/.test(fingerprint)) return "bradesco";
  if (/nubank|260|nu pagamentos|nu_pagamentos/.test(fingerprint)) return "nubank";
  if (/santander|033/.test(fingerprint)) return "santander";
  if (/inter|077/.test(fingerprint)) return "inter";
  if (/c6\b|c6 bank|336/.test(fingerprint)) return "c6";
  return "unknown";
}

function formatDisplayName(
  bank: string,
  branchid: string | null,
  acctid: string | null,
  type: "checking" | "credit_card",
): string {
  const bankLabel = bank === "unknown" ? t("import.account_generic") : capitalize(bank);
  if (type === "credit_card") {
    // A card only has ACCTID (a long UUID). Shows "Bank · card · last XXXX".
    const tail = acctid ? acctid.slice(-4) : null;
    const parts = [bankLabel, t("import.account_card")];
    if (tail) parts.push(t("import.account_last4", { v: tail }));
    return parts.join(" · ");
  }
  const parts = [bankLabel];
  if (branchid) parts.push(t("import.account_branch", { v: branchid }));
  if (acctid) parts.push(t("import.account_number", { v: acctid }));
  return parts.join(" · ");
}

function capitalize(s: string): string {
  return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
}

function computeSummary(txs: ParsedTransaction[]): ParsedSummary {
  let totalIn = 0;
  let totalOut = 0;
  let earliest: string | null = null;
  let latest: string | null = null;
  for (const t of txs) {
    const n = Number(t.amount);
    if (Number.isFinite(n)) {
      if (n >= 0) totalIn += n;
      else totalOut += -n;
    }
    if (t.date) {
      if (earliest === null || t.date < earliest) earliest = t.date;
      if (latest === null || t.date > latest) latest = t.date;
    }
  }
  return {
    totalIn: totalIn.toFixed(2),
    totalOut: totalOut.toFixed(2),
    net: (totalIn - totalOut).toFixed(2),
    earliest,
    latest,
  };
}
