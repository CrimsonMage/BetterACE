-- Allegiance data is independent of inventory object IDs. Deferred references
-- allow an entire bounded subtree mutation to commit atomically.
CREATE TABLE allegiance_nodes (
    character_id bigint PRIMARY KEY REFERENCES players(object_id),
    account_id bigint NOT NULL REFERENCES accounts(id),
    patron_id bigint REFERENCES allegiance_nodes(character_id) DEFERRABLE INITIALLY DEFERRED,
    monarch_id bigint NOT NULL REFERENCES allegiance_nodes(character_id) DEFERRABLE INITIALLY DEFERRED,
    version bigint NOT NULL CHECK(version > 0),
    mutation_revision bigint NOT NULL CHECK(mutation_revision > 0),
    payload bytea NOT NULL CHECK(octet_length(payload) <= 2097204),
    CHECK(patron_id IS NULL OR patron_id <> character_id),
    CHECK(patron_id IS NOT NULL OR monarch_id = character_id)
);
CREATE INDEX allegiance_patron ON allegiance_nodes(patron_id);
CREATE INDEX allegiance_monarch ON allegiance_nodes(monarch_id);
CREATE TABLE allegiance_metadata (
    chat_room bigint NOT NULL UNIQUE CHECK(chat_room BETWEEN 2147483648 AND 4294967294),
    monarch_id bigint PRIMARY KEY REFERENCES allegiance_nodes(character_id) DEFERRABLE INITIALLY DEFERRED,
    version bigint NOT NULL CHECK(version > 0),
    mutation_revision bigint NOT NULL CHECK(mutation_revision > 0),
    payload bytea NOT NULL CHECK(octet_length(payload) <= 2097204)
);
