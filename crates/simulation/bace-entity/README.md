# bace-entity

Status: foundation.

Runtime entity state and read projections.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

`Combatant` holds authoritative health, dirty revision and prepared melee input/profile state. Its schema is an in-memory simulation model, not a frozen save DTO. It does not replace full character vitals/armor/weapon composition.

`EntityProperties` provides bounded typed scalar qualities and before/after
revision proposals for native NPC state. It preserves absent values, validates
families/widths/finite values and rejects stale adoption. `Combatant` has explicit
optional mana/stamina pools alongside its existing sole health value; missing
resources fail instead of inventing values. World applies complete resource
batches only after checking all affected owners.
