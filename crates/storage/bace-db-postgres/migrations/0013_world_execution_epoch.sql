-- Fences source-defined NPC stage writers even when the old world-lock
-- connection is lost while its independent writer connection remains alive.
CREATE TABLE world_execution_epoch (
    singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
    epoch bigint NOT NULL CHECK(epoch>=0)
);
INSERT INTO world_execution_epoch VALUES(true,0);
