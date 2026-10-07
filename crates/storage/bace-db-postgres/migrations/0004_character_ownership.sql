CREATE TABLE character_ownership (
    character_id bigint PRIMARY KEY REFERENCES entity_snapshots(object_id),
    epoch bigint NOT NULL DEFAULT 0 CHECK(epoch>=0),
    state text NOT NULL DEFAULT 'offline' CHECK(state IN ('offline','loading','online','logging_out')),
    cached_xp numeric(20,0) NOT NULL DEFAULT 0 CHECK(cached_xp BETWEEN 0 AND 4294967295)
);
CREATE TABLE offline_xp_events (
    event_id text PRIMARY KEY CHECK(octet_length(event_id) BETWEEN 1 AND 128),
    source_character bigint NOT NULL CHECK(source_character BETWEEN 1 AND 4294967295),
    target_character bigint NOT NULL REFERENCES character_ownership(character_id),
    amount numeric(20,0) NOT NULL CHECK(amount BETWEEN 1 AND 18446744073709551615),
    result_cached_xp numeric(20,0),
    CHECK(result_cached_xp IS NULL OR result_cached_xp BETWEEN 0 AND 4294967295)
);
CREATE INDEX offline_xp_pending ON offline_xp_events(target_character,event_id) WHERE result_cached_xp IS NULL;
