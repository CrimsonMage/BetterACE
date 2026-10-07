-- Candidate allocation holds this row lock until commit, so revision order is commit-safe.
CREATE TABLE content_clock (singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), revision bigint NOT NULL);
INSERT INTO content_clock VALUES (true, 0);
CREATE TABLE content_publications (
    revision bigint PRIMARY KEY,
    transaction_id xid8 NOT NULL UNIQUE,
    status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
    rejection text,
    candidate_count integer NOT NULL DEFAULT 0 CHECK(candidate_count <= 100000),
    payload_bytes bigint NOT NULL DEFAULT 0 CHECK(payload_bytes <= 268435456),
    CHECK ((status = 'rejected') = (rejection IS NOT NULL))
);
CREATE INDEX content_pending_revision ON content_publications(revision) WHERE status='pending';
CREATE TABLE content_candidates (
    revision bigint NOT NULL REFERENCES content_publications(revision),
    wcid bigint NOT NULL CHECK(wcid BETWEEN 1 AND 4294967295),
    class_name text NOT NULL CHECK(octet_length(class_name) BETWEEN 1 AND 255),
    weenie_type integer NOT NULL,
    payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 17825792),
    PRIMARY KEY(revision, wcid)
);
CREATE FUNCTION journal_content_candidate() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE published bigint;
BEGIN
    SELECT revision INTO published FROM content_publications WHERE transaction_id = pg_current_xact_id();
    IF published IS NULL THEN
        UPDATE content_clock SET revision = revision + 1 WHERE singleton RETURNING revision INTO published;
        INSERT INTO content_publications(revision, transaction_id) VALUES (published, pg_current_xact_id());
        PERFORM pg_notify('ace_content', published::text);
    END IF;
    UPDATE content_publications SET candidate_count=candidate_count+1, payload_bytes=payload_bytes+octet_length(NEW.payload) WHERE revision=published;
    NEW.revision := published;
    RETURN NEW;
END $$;
CREATE TRIGGER content_candidate_insert BEFORE INSERT ON content_candidates FOR EACH ROW EXECUTE FUNCTION journal_content_candidate();
CREATE FUNCTION immutable_content_candidate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'content candidates are immutable; insert a new revision'; END $$;
CREATE TRIGGER content_candidate_immutable BEFORE UPDATE OR DELETE ON content_candidates FOR EACH ROW EXECUTE FUNCTION immutable_content_candidate();
CREATE TABLE content_heads (
    wcid bigint PRIMARY KEY,
    revision bigint NOT NULL,
    class_name text NOT NULL,
    UNIQUE(class_name) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY(revision,wcid) REFERENCES content_candidates(revision,wcid)
);
CREATE TABLE entity_snapshots (
    object_id bigint PRIMARY KEY CHECK(object_id BETWEEN 1 AND 4294967295),
    version bigint NOT NULL CHECK(version > 0),
    payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 17825792)
);
CREATE TABLE durable_operations (
    operation_id text PRIMARY KEY CHECK(length(operation_id) BETWEEN 1 AND 128),
    -- Adapter computes a deterministic fingerprint of every participant and proposed state.
    request_fingerprint bytea NOT NULL CHECK(octet_length(request_fingerprint) = 32)
);
