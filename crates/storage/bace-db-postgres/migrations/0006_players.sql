-- Official ACE ObjectGuid player range. IDs are never recycled by this allocator.
CREATE SEQUENCE player_object_ids AS bigint MINVALUE 1342177281 MAXVALUE 1610612735 NO CYCLE;
CREATE TABLE players (
    object_id bigint PRIMARY KEY REFERENCES character_ownership(character_id),
    account_id bigint NOT NULL REFERENCES accounts(id),
    name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 100),
    canonical_name text COLLATE "C" NOT NULL UNIQUE,
    slot smallint NOT NULL CHECK(slot BETWEEN 0 AND 99),
    UNIQUE(account_id,slot)
);
CREATE INDEX player_account ON players(account_id,slot);
