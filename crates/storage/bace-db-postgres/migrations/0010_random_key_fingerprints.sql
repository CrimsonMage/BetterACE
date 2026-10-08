-- Public key fingerprints bind persisted RNG ordinals to immutable local key versions.
-- Secret master keys never enter this table.
CREATE TABLE random_key_fingerprints(
 version bigint PRIMARY KEY CHECK(version BETWEEN 1 AND 4294967295),
 sha256 bytea NOT NULL CHECK(octet_length(sha256)=32)
);
CREATE FUNCTION reject_random_key_rewrite() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'random key fingerprints are immutable' USING ERRCODE='23514'; END; $$;
CREATE TRIGGER random_key_immutable BEFORE UPDATE OR DELETE ON random_key_fingerprints FOR EACH ROW EXECUTE FUNCTION reject_random_key_rewrite();
