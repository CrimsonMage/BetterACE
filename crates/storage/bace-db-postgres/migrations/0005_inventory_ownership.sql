-- Items and containers share the authoritative aggregate identity space.
CREATE TABLE item_ownership (
    item_id bigint PRIMARY KEY REFERENCES entity_snapshots(object_id),
    container_id bigint NOT NULL REFERENCES entity_snapshots(object_id),
    slot bigint NOT NULL CHECK(slot BETWEEN 0 AND 4294967295),
    CHECK(item_id <> container_id),
    UNIQUE(container_id,slot) DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX item_container ON item_ownership(container_id);
