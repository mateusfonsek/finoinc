-- Which rule set a transaction's category. NULL when it was set by hand, or
-- before this column existed. Without it, undoing a rule could only guess
-- what that rule had categorized, and took manual choices with it.
ALTER TABLE transactions ADD COLUMN categorized_by_rule INTEGER REFERENCES rules(id) ON DELETE SET NULL;
