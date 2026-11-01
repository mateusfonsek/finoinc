export type ColumnRole = "date" | "description" | "amount" | "inflow" | "outflow" | "bankId";

/**
 * How to read one bank's CSV. `columns` holds indexes into the header of the
 * file at hand; `columnNames` keeps the header names those indexes came from,
 * so a saved format still lines up when the bank reorders its columns.
 */
export type CsvFormat = {
  id: string;
  name: string;
  bank: string | null;
  kind: "checking" | "credit_card";
  signature: string;
  columns: { date: number; description: number; amount?: number; inflow?: number; outflow?: number; bankId?: number };
  columnNames: Partial<Record<ColumnRole, string>>;
  dateFormat: "dd/mm/yyyy" | "yyyy-mm-dd" | "mm/dd/yyyy";
  /** Card bills list purchases as positive; flip them to spending. */
  invertSign: boolean;
  builtin: boolean;
};
