-- Checkpoints describe source-defined ACE continuations, not whole-conversation
-- transactions. Committed earlier stages are retained when a later stage fails.
CREATE TABLE npc_workflows (
    invocation bytea PRIMARY KEY CHECK(octet_length(invocation)=16),
    source_id bigint NOT NULL CHECK(source_id BETWEEN 1 AND 4294967295),
    version bigint NOT NULL CHECK(version>0),
    stage bigint NOT NULL CHECK(stage>=0),
    program_hash bytea NOT NULL CHECK(octet_length(program_hash)=32),
    content_generation bytea NOT NULL CHECK(octet_length(content_generation)=32),
    key_version bigint NOT NULL REFERENCES random_key_fingerprints(version),
    completed boolean NOT NULL,
    checkpoint bytea NOT NULL CHECK(octet_length(checkpoint) BETWEEN 52 AND 2097204)
);
CREATE INDEX npc_workflow_pending ON npc_workflows(source_id,invocation) WHERE NOT completed;
