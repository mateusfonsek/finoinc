-- CSV import. A CSV carries neither an account number nor transaction ids, so
-- how to read each bank's layout ("format") and which account it imports into
-- are remembered here. Built-in formats live in the app; only the user's own
-- mappings are stored, but any format id (built-in too) can remember an account.

CREATE TABLE csv_formats (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  bank TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('checking','credit_card')),
  signature TEXT NOT NULL UNIQUE,
  spec TEXT NOT NULL,          -- JSON: columns, columnNames, dateFormat, invertSign
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE csv_format_accounts (
  format_id TEXT PRIMARY KEY,  -- built-in ids too, e.g. 'nubank-checking'
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE
);
