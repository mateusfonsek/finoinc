import { describe, it, expect } from "vitest";
import { detectReversalPairs } from "./reversals";
import type { ParsedTransaction } from "./types";
import type { ReversalPhase } from "./reversals";

/** The pt-BR phases, spelled out: these tests exercise the algorithms, and the
 *  phrasing is now data the pack owns. */
const PT_BR_PHASES: ReversalPhase[] = [
  { strategy: "exact_remainder", prefix: "Estorno - ", window_days: 7,
    role: "reversal", counterpart_role: "reversed" },
  { strategy: "counterparty_signature", prefix: "Reembolso recebido pelo Pix - ",
    counterpart_prefix: "Transferência enviada pelo Pix - ", window_days: 30,
    role: "refund", counterpart_role: "refunded" },
  { strategy: "quoted_merchant", prefix: "Estorno de ", window_days: 30,
    role: "reversal", counterpart_role: "reversed" },
];

function tx(
  fitid: string,
  date: string,
  amount: string,
  description: string,
): ParsedTransaction {
  return { fitid, date, amount, description };
}

describe("detectReversalPairs — reversals", () => {
  it("matches a single estorno with its original tx (same day, opposite amount)", () => {
    const a = tx("A", "2026-01-20", "-1385.43", "Transferência enviada pelo Pix - Ana");
    const b = tx("B", "2026-01-20", "1385.43", "Estorno - Transferência enviada pelo Pix - Ana");
    const map = detectReversalPairs([a, b], PT_BR_PHASES);
    expect(map.size).toBe(2);
    expect(map.get("A")).toEqual({ role: "reversed", pairFitid: "B" });
    expect(map.get("B")).toEqual({ role: "reversal", pairFitid: "A" });
  });

  it("pairs N reversals with N of N+1 originals (FIFO), leaving one original unpaired", () => {
    const o1 = tx("O1", "2026-01-20", "-1385.43", "Pix - Ana");
    const o2 = tx("O2", "2026-01-20", "-1385.43", "Pix - Ana");
    const o3 = tx("O3", "2026-01-20", "-1385.43", "Pix - Ana");
    const o4 = tx("O4", "2026-01-20", "-1385.43", "Pix - Ana");
    const e1 = tx("E1", "2026-01-20", "1385.43", "Estorno - Pix - Ana");
    const e2 = tx("E2", "2026-01-20", "1385.43", "Estorno - Pix - Ana");
    const e3 = tx("E3", "2026-01-20", "1385.43", "Estorno - Pix - Ana");

    const map = detectReversalPairs([o1, o2, o3, o4, e1, e2, e3], PT_BR_PHASES);

    expect(map.size).toBe(6);
    expect(map.has("O4")).toBe(false);
    for (const e of ["E1", "E2", "E3"]) {
      expect(map.get(e)?.role).toBe("reversal");
    }
  });

  it("does not pair estorno when descriptions differ", () => {
    const a = tx("A", "2026-01-20", "-100", "Pix - João");
    const b = tx("B", "2026-01-20", "100", "Estorno - Pix - Maria");
    expect(detectReversalPairs([a, b], PT_BR_PHASES).size).toBe(0);
  });

  it("does not pair estorno when amounts are not opposite", () => {
    const a = tx("A", "2026-01-20", "-100", "Pix - João");
    const b = tx("B", "2026-01-20", "50", "Estorno - Pix - João");
    expect(detectReversalPairs([a, b], PT_BR_PHASES).size).toBe(0);
  });

  it("does not pair estorno when dates are >7 days apart", () => {
    const a = tx("A", "2026-01-01", "-100", "Pix - João");
    const b = tx("B", "2026-01-10", "100", "Estorno - Pix - João");
    expect(detectReversalPairs([a, b], PT_BR_PHASES).size).toBe(0);
  });

  it("returns empty for txs without any Estorno prefix", () => {
    const a = tx("A", "2026-01-20", "-100", "Pix - X");
    const b = tx("B", "2026-01-20", "100", "Pix recebido - X");
    expect(detectReversalPairs([a, b], PT_BR_PHASES).size).toBe(0);
  });

  it("ignores tx without fitid", () => {
    const a: ParsedTransaction = { fitid: null, date: "2026-01-20", amount: "-100", description: "Pix - X" };
    const b = tx("B", "2026-01-20", "100", "Estorno - Pix - X");
    expect(detectReversalPairs([a, b], PT_BR_PHASES).size).toBe(0);
  });
});

describe("detectReversalPairs — refunds", () => {
  const ENVIADO = "Transferência enviada pelo Pix - AMAZON.COM.BR - 15.436.940/0001-03 - EBANX IP LTDA. (0383) Agência: 1 Conta: 1234567890-1";
  const REEMBOLSO = "Reembolso recebido pelo Pix - AMAZON SERVICOS DE VAREJO DO BRASIL LTDA. - 15.436.940/0001-03 - EBANX IP LTDA. (0383) Agência: 1 Conta: 1234567890-1";

  it("pairs reembolso with the matching Pix sent (name varies, CNPJ+account matches)", () => {
    const sent = tx("S", "2026-02-10", "-99.90", ENVIADO);
    const refund = tx("R", "2026-02-12", "99.90", REEMBOLSO);
    const map = detectReversalPairs([sent, refund], PT_BR_PHASES);
    expect(map.size).toBe(2);
    expect(map.get("S")?.role).toBe("refunded");
    expect(map.get("R")?.role).toBe("refund");
    expect(map.get("S")?.pairFitid).toBe("R");
  });

  it("pairs FIFO when there are 6 sent + 6 refunds (real Amazon Feb case)", () => {
    const sents = [];
    const refunds = [];
    for (let i = 0; i < 6; i++) {
      sents.push(tx(`S${i}`, "2026-02-10", "-99.90", ENVIADO));
      refunds.push(tx(`R${i}`, "2026-02-12", "99.90", REEMBOLSO));
    }
    const map = detectReversalPairs([...sents, ...refunds], PT_BR_PHASES);
    expect(map.size).toBe(12);
    for (let i = 0; i < 6; i++) {
      expect(map.has(`S${i}`)).toBe(true);
      expect(map.has(`R${i}`)).toBe(true);
    }
  });

  it("does not pair reembolso with sent of different CNPJ", () => {
    const sent = tx("S", "2026-02-10", "-99.90",
      "Transferência enviada pelo Pix - Outra Loja - 99.999.999/0001-99 - BCO X (0001) Agência: 1 Conta: 1-1");
    const refund = tx("R", "2026-02-12", "99.90", REEMBOLSO);
    expect(detectReversalPairs([sent, refund], PT_BR_PHASES).size).toBe(0);
  });

  it("does not pair reembolso when amounts are different", () => {
    const sent = tx("S", "2026-02-10", "-100.00", ENVIADO);
    const refund = tx("R", "2026-02-12", "50.00", REEMBOLSO);
    expect(detectReversalPairs([sent, refund], PT_BR_PHASES).size).toBe(0);
  });

  it("does not pair reembolso when window > 30 days", () => {
    const sent = tx("S", "2026-01-01", "-99.90", ENVIADO);
    const refund = tx("R", "2026-02-15", "99.90", REEMBOLSO);
    expect(detectReversalPairs([sent, refund], PT_BR_PHASES).size).toBe(0);
  });

  it("pairs reembolso even with delay up to 30 days", () => {
    const sent = tx("S", "2026-02-01", "-99.90", ENVIADO);
    const refund = tx("R", "2026-02-28", "99.90", REEMBOLSO);
    expect(detectReversalPairs([sent, refund], PT_BR_PHASES).size).toBe(2);
  });
});

describe("detectReversalPairs — combined", () => {
  it("handles a mix of reversals and refunds in the same batch", () => {
    const eOrig = tx("EO", "2026-01-20", "-100", "Pix - João");
    const e = tx("E", "2026-01-20", "100", "Estorno - Pix - João");
    const rOrig = tx("RO", "2026-02-10", "-50",
      "Transferência enviada pelo Pix - AMAZON.COM.BR - 15.436.940/0001-03 - EBANX IP LTDA. (0383) Agência: 1 Conta: 1");
    const r = tx("R", "2026-02-12", "50",
      "Reembolso recebido pelo Pix - AMAZON SERVICOS - 15.436.940/0001-03 - EBANX IP LTDA. (0383) Agência: 1 Conta: 1");
    const map = detectReversalPairs([eOrig, e, rOrig, r], PT_BR_PHASES);
    expect(map.size).toBe(4);
    expect(map.get("E")?.role).toBe("reversal");
    expect(map.get("EO")?.role).toBe("reversed");
    expect(map.get("R")?.role).toBe("refund");
    expect(map.get("RO")?.role).toBe("refunded");
  });

  it("returns no pairs when the locale declares no phases", () => {
    const a = tx("A", "2026-01-20", "-10.00", "Transferência enviada pelo Pix - Ana");
    const b = tx("B", "2026-01-20", "10.00", "Estorno - Transferência enviada pelo Pix - Ana");
    expect(detectReversalPairs([a, b], []).size).toBe(0);
  });
});

describe("detectReversalPairs — card reversals (Nubank)", () => {
  it("pairs CC estorno with original CC purchase (same merchant in quotes)", () => {
    const purchase = tx("P", "2026-03-15", "-108.14", "Acme.Ai");
    const estorno = tx("E", "2026-03-18", "108.14", `Estorno de "Acme.Ai" (Acme.Ai)`);
    const map = detectReversalPairs([purchase, estorno], PT_BR_PHASES);
    expect(map.size).toBe(2);
    expect(map.get("E")?.role).toBe("reversal");
    expect(map.get("P")?.role).toBe("reversed");
  });

  it("does not pair CC estorno with different merchant", () => {
    const purchase = tx("P", "2026-03-15", "-50", "Mana Poke");
    const estorno = tx("E", "2026-03-18", "50", `Estorno de "Acme.Ai" (Acme.Ai)`);
    expect(detectReversalPairs([purchase, estorno], PT_BR_PHASES).size).toBe(0);
  });

  it("does not pair CC estorno when window > 30 days", () => {
    const purchase = tx("P", "2026-01-01", "-108.14", "Acme.Ai");
    const estorno = tx("E", "2026-03-15", "108.14", `Estorno de "Acme.Ai" (Acme.Ai)`);
    expect(detectReversalPairs([purchase, estorno], PT_BR_PHASES).size).toBe(0);
  });

  it("matches case-insensitively (merchant casing varies)", () => {
    const purchase = tx("P", "2026-03-15", "-50", "MYCLAW.AI");
    const estorno = tx("E", "2026-03-16", "50", `Estorno de "myclaw.ai" (myclaw.ai)`);
    const map = detectReversalPairs([purchase, estorno], PT_BR_PHASES);
    expect(map.size).toBe(2);
  });
});
