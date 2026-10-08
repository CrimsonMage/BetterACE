-- Native loot/rare changes share the same transaction-ordered durable journal.
-- This is incremental authoring data, not a PostgreSQL copy of the world pack.
CREATE TABLE native_content_candidates (
    revision bigint NOT NULL REFERENCES content_publications(revision),
    namespace integer NOT NULL CHECK(namespace IN (46,47)),
    content_id bigint NOT NULL CHECK(content_id BETWEEN 1 AND 4294967295),
    schema_version integer NOT NULL CHECK(schema_version BETWEEN 1 AND 65535),
    payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 16777216),
    PRIMARY KEY(revision,namespace,content_id)
);
CREATE TRIGGER native_candidate_insert BEFORE INSERT ON native_content_candidates
FOR EACH ROW EXECUTE FUNCTION journal_content_candidate();
CREATE TRIGGER native_candidate_immutable BEFORE UPDATE OR DELETE ON native_content_candidates
FOR EACH ROW EXECUTE FUNCTION immutable_content_candidate();
CREATE TABLE native_content_heads (
    namespace integer NOT NULL,
    content_id bigint NOT NULL,
    revision bigint NOT NULL,
    PRIMARY KEY(namespace,content_id),
    FOREIGN KEY(revision,namespace,content_id)
      REFERENCES native_content_candidates(revision,namespace,content_id)
);
