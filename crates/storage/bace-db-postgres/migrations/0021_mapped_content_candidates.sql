-- Explicit world-row and ClothingBase changes, plus removals of any mapped
-- source record. The accepted pack remains the authority for preexisting rows.
ALTER TABLE content_publications ADD COLUMN expected_manifest_hash bytea
    CHECK(expected_manifest_hash IS NULL OR octet_length(expected_manifest_hash) = 32);
CREATE TABLE mapped_content_candidates (
    revision bigint NOT NULL REFERENCES content_publications(revision),
    namespace integer NOT NULL CHECK(namespace = 1 OR namespace BETWEEN 16 AND 47 OR namespace = 50),
    content_id bigint NOT NULL CHECK(content_id BETWEEN 0 AND 4294967295),
    schema_version integer NOT NULL CHECK(schema_version = 1),
    payload bytea CHECK(payload IS NULL OR octet_length(payload) BETWEEN 1 AND 16777216),
    CHECK(payload IS NULL OR namespace BETWEEN 16 AND 45 OR namespace = 50),
    PRIMARY KEY(revision, namespace, content_id)
);

CREATE FUNCTION journal_mapped_content_candidate() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE published bigint;
BEGIN
    SELECT revision INTO published FROM content_publications WHERE transaction_id = pg_current_xact_id();
    IF published IS NULL THEN
        UPDATE content_clock SET revision = revision + 1 WHERE singleton RETURNING revision INTO published;
        INSERT INTO content_publications(revision, transaction_id) VALUES (published, pg_current_xact_id());
        PERFORM pg_notify('ace_content', published::text);
    END IF;
    UPDATE content_publications
       SET candidate_count = candidate_count + 1,
           payload_bytes = payload_bytes + COALESCE(octet_length(NEW.payload), 0)
     WHERE revision = published;
    NEW.revision := published;
    RETURN NEW;
END $$;
CREATE TRIGGER mapped_content_candidate_insert BEFORE INSERT ON mapped_content_candidates
FOR EACH ROW EXECUTE FUNCTION journal_mapped_content_candidate();
CREATE TRIGGER mapped_content_candidate_immutable BEFORE UPDATE OR DELETE ON mapped_content_candidates
FOR EACH ROW EXECUTE FUNCTION immutable_content_candidate();

CREATE TABLE mapped_content_heads (
    namespace integer NOT NULL,
    content_id bigint NOT NULL,
    revision bigint NOT NULL,
    PRIMARY KEY(namespace,content_id),
    FOREIGN KEY(revision,namespace,content_id)
      REFERENCES mapped_content_candidates(revision,namespace,content_id)
);
