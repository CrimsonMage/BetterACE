-- ACE Character.IsPlussed is independent of game account access and privileges.
ALTER TABLE players ADD COLUMN is_plussed boolean NOT NULL DEFAULT false;
