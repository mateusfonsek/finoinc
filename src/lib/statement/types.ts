export interface ParsedStatement {
  account: ParsedAccount;
  transactions: ParsedTransaction[];
  summary: ParsedSummary;
}

export interface ParsedAccount {
  /** Normalized bank: 'itau' | 'nubank' | 'bradesco' | 'unknown' */
  bank: string;
  ofxAcctid: string | null;
  ofxBankid: string | null;
  ofxFid: string | null;
  displayName: string;
  /** `checking` = checking account (BANKMSGSRSV1) · `credit_card` = credit card statement (CREDITCARDMSGSRSV1) */
  type: "checking" | "credit_card";
}

export interface ParsedTransaction {
  fitid: string | null;
  /** ISO 8601 YYYY-MM-DD */
  date: string;
  /** Decimal as a string (e.g. "-123.45") */
  amount: string;
  description: string;
}

export interface ParsedSummary {
  totalIn: string;
  totalOut: string;
  net: string;
  earliest: string | null;
  latest: string | null;
}
