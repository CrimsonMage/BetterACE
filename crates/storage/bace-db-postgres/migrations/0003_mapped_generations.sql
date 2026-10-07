CREATE TABLE content_generations (
    manifest_hash bytea PRIMARY KEY CHECK(octet_length(manifest_hash)=32),
    parent_hash bytea REFERENCES content_generations(manifest_hash),
    base_hash bytea NOT NULL CHECK(octet_length(base_hash)=32),
    accepted_revision bigint NOT NULL CHECK(accepted_revision>=0),
    manifest_bytes bytea NOT NULL CHECK(octet_length(manifest_bytes) BETWEEN 1 AND 1048576)
);
CREATE TABLE content_runtime_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
    active_manifest_hash bytea REFERENCES content_generations(manifest_hash)
);
INSERT INTO content_runtime_state(singleton) VALUES(true);
