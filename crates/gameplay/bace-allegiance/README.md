# bace-allegiance

BetterACE's indexed allegiance forest, source permission checks, metadata and XP
ledger. Immutable proposals compare exact before-state and one monotonic mutation
revision; simulation adoption follows the combined durable transaction. Nodes
cache rank and descendant counts; cold restore validates the entire bounded
forest, backlinks, roots, membership caches and unique dynamic chat rooms.

Hierarchy and management follow official ACE at the pin in
`docs/baselines.toml`. XP pass-up intentionally follows the user-selected GDLE
353cbab52ef7da2b7063bc3e3f008461d8531693 rules instead of ACE's maximum-time
assumption. `xp` preserves cached level/leadership/loyalty inputs, oath age,
300-second online-time checkpoints, capped unclaimed XP, lifetime counters and
online-only recursive forwarding. Offline patrons retain credit; login redemption
is NoHandling and does not forward or repair Vitae. The checkpoint clock is
explicit and does not obtain wall time on the simulation thread.

`tests/gdle.rs` consumes 402 independently compiled original C++ vectors generated
by `oracle/gdle_passup.py`; the generator verifies the pinned checkout and records
source hashes. This establishes those formula/order cases, not complete social
or stock-client qualification. Frozen DTOs and relational CAS live in
`bace-storage-codec` and `bace-db-postgres`; this crate performs no I/O.

Cached skill refresh follows the pinned GDLE call sites exactly:
`WeenieObject.cpp::GiveSkillXP` and `GiveSkillPoints` refresh both Leadership and
Loyalty (and cached level) after a positive award to skill35 or36;
`AllegianceTreeNode::UpdateWithWeenie` reads Current, not Base, without a291 storage
cap. Swearing also refreshes both parties. Login, enchantment changes, attribute
raises, and `GiveSkillAdvancementClass` do not call this refresh at the pin; their
stale-cache behavior is intentionally retained. Simulation queues frozen values
from accepted source award boundaries, blocks subsequent pass-up until the exact
ledger receipt, and preserves the queue across rejection. The public NPC award
hook requires integration after that award's durable adoption. Simulation's
`oracle/gdle_cached_skills.py` compiles the original C++ method and records four
value vectors; owner tests additionally check trigger gating, receipt ordering,
rejection and later modifier changes.
