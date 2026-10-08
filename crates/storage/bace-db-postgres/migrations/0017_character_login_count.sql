-- ACE Character.TotalLogins is a signed Int32. New native characters start at 0;
-- a successful fenced Loading -> Online transition increments exactly once.
-- Existing native snapshots did not store this counter, so migration starts a
-- new native sequence history instead of deriving it from unrelated lease epochs.
ALTER TABLE character_ownership
    ADD COLUMN total_logins integer NOT NULL DEFAULT 0 CHECK (total_logins >= 0);
