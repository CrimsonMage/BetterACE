-- Housing identity/ownership stays relational; permissions, rent and state are
-- a frozen versioned binary aggregate in entity_snapshots.
CREATE TABLE house_ownership (
    object_id bigint PRIMARY KEY REFERENCES entity_snapshots(object_id),
    house_id bigint NOT NULL UNIQUE CHECK(house_id BETWEEN 1 AND 4294967295),
    owner_id bigint NOT NULL REFERENCES players(object_id)
);
CREATE INDEX houses_by_owner ON house_ownership(owner_id);
