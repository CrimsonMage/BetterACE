# Creature constructor / restoration source oracle

Official ACEmulator/ACE pin `47edade3bd3f6044b676d4eb877c4965c7eda62b`.
ACE source attribution and AGPL-3.0-only licensing are preserved. `generate.py`
extracts the **unchanged complete method bodies and constructor declarations**
listed in `golden.json`; it compiles them with the explicit helper stubs in
`Harness.cs` using .NET 8. Source-file and extracted-body SHA-256 hashes are
included in the golden metadata. No Rust implementation is called.

Run from any directory:

```sh
python3 crates/application/bace-runtime/oracle/constructed_restore/generate.py /path/to/dotnet
```

The generated 256 cases vary fresh/biota construction, Player/non-player,
negative/zero/positive/above-maximum saved vitals, existing inventory,
failed/successful wield helper, attackability at delayed-action execution, and
missing/preexisting property dictionaries. Each case records the full constructor
trace, queued action count, the trace immediately before 0.1 seconds and at the
0.1-second deadline, final vitals, equipment/inventory order, dictionary/skill
initialization, NonCombat state, selected-target reset, and container defaults.

## What is original source

- `Creature.cs`: both constructors, `InitializePropertyDictionaries`, and
  `SetEphemeralValues`.
- `Container.cs`: `SetEphemeralValues(bool fromBiota)`.
- `Monster_Inventory.cs`: `EquipInventoryItems(bool weaponsOnly = false)`.
- `Creature_Equipment.cs`: `TryWieldObject` and `TryActivateItemSpells`.

The source controls the two equip passes around wielded treasure generation,
removal/reinsertion on failed wield, skipped Player-only/non-player-only paths,
0.1-second delay request, and the deferred `Attackable` check with ordered spell
iteration. The source initializes missing dictionaries, repairs nonpositive
Player vitals, preserves positive Player values (including above maximum), and
refills all non-player vitals after generation. Fresh and biota non-player
constructors execute the same generation steps; biota construction is **not** an
ACE no-reroll path.

## Deliberate stub boundary

The harness supplies preselected deterministic inventory and spell IDs; helper
calls append trace records. It does not run random treasure tables, appearance
randomness, real slot/coverage tests, actual spell effects, attribute/vital
formulas, or asynchronous database inventory loading. `CreatureVital.MaxValue`
is the explicit fixture value 100/110/120, so these vectors qualify source
assignment/control flow, not maximum-vital arithmetic. The Container constructor
stub invokes the original ephemeral method and optionally supplies an existing
inventory before Creature initialization. This is a controlled helper input,
not a claim about ordering of ACE's asynchronous `GetInventoryInParallel`
callback. `TryEquipObject` success/failure is supplied explicitly.

The small `ActionChain` stub records the exact requested delay and executes queued
callbacks in insertion order at explicit supplied times; it does not qualify the
production ACE scheduler. Setting attackability to the opposite initial value
before callback execution verifies that the source reads it at release time.

## BetterACE durable-identity hardening — not ACE expected output

The planned BetterACE restoration owner must reconstruct already persisted
item identities/placements and must not generate another wield/CreateList/treasure
forest or redraw randomness while restoring that durable forest. This differs
from the original non-player `Creature(Biota)` generation calls observed here.
It must be documented and tested separately as durable-identity hardening; the
source fixture intentionally retains ACE's generation trace. It does not encode
the desired BetterACE deviation as an upstream expectation.

Ordinary saves or ownership transfers must not replay constructor actions or
reset live delayed actions. A true cold reconstruction may recreate qualified
ephemeral state; its existing inventory, enchantment cleanup/reapplication,
remaining timers, and post-restore vitality policy require explicit owner tests
before enabling the currently held restoration path. These oracle files alone
do not enable or claim complete constructor restoration, nested Creature
inventory, Vendor commerce, pets, GamePiece, or client compatibility.
