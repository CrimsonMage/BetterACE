-- V3 archives have a 4 MiB payload plus the frozen 52-byte envelope.
ALTER TABLE npc_workflows DROP CONSTRAINT npc_workflows_checkpoint_check;
ALTER TABLE npc_workflows ADD CONSTRAINT npc_workflows_checkpoint_check
    CHECK(octet_length(checkpoint) BETWEEN 52 AND 4194356);

-- Invocation rows remain the operation journal. This row is the canonical
-- complete VM checkpoint for a source, ordered by explicit compare-and-swap.
CREATE TABLE npc_source_heads (
    source_id bigint PRIMARY KEY CHECK(source_id BETWEEN 1 AND 4294967295),
    version bigint NOT NULL CHECK(version>0),
    invocation bytea NOT NULL REFERENCES npc_workflows(invocation),
    workflow_version bigint NOT NULL CHECK(workflow_version>0),
    world_epoch bigint NOT NULL CHECK(world_epoch>0),
    content_generation bytea NOT NULL CHECK(octet_length(content_generation)=32),
    source_cell bigint CHECK(source_cell BETWEEN 1 AND 4294967295),
    completed boolean NOT NULL,
    archived boolean NOT NULL,
    checkpoint bytea NOT NULL CHECK(octet_length(checkpoint) BETWEEN 52 AND 4194356)
);
CREATE INDEX npc_source_heads_archived ON npc_source_heads(source_id) WHERE archived;
CREATE INDEX npc_source_heads_pending ON npc_source_heads(source_id) WHERE NOT completed;
CREATE INDEX npc_source_heads_content_pin ON npc_source_heads(content_generation) WHERE NOT completed;
CREATE INDEX npc_workflows_source_history ON npc_workflows(source_id,invocation);

CREATE INDEX npc_source_heads_live_region ON npc_source_heads((source_cell >> 16),source_id) WHERE NOT archived;
