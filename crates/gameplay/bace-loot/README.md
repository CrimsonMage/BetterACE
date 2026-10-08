# bace-loot

Treasure tables and generated items.

Status: foundation; complete subsystem behavior is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

`select_create_list` implements pinned ACE's default-rate `Creature_Equipment.CreateListSelect` with explicit caller-owned random draws, preserved row order, unconditional initial RNG consumption and grouped probabilities. Zero-template placeholders are retained as selected row indices. Validation/output-capacity failure cannot partially append results. Non-default trophy rates remain unsupported by this selector. Prepared ACE treasure generation is described below.

Official ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `Source/ACE.Server/WorldObjects/Creature_Equipment.cs`, AGPL-3.0-only, ACE contributors. `tests/fixtures/create-list.csv` has 20 independent verbatim C# method vectors, including exact selection boundaries. Generator: `../bace-combat/oracle/generate.py`. These prove row-selection behavior, not item instantiation or death-treasure parity.

Native `LootGraphV1` supports bounded nested weighted, independent, ordered
cumulative and all-branch tables, explicit no-drop leaves, stack ranges and item
mutation subtables. `LootScratch` bounds expansion and preserves caller output on
failure. Immutable event identity plus table/node/branch paths reconstruct retries;
ordinary loot does not advance character rare state. `materialize_drop` applies
frozen mutations to a cloned template before placement/save preparation.

`RareEvaluator` follows the owner's Turbine evidence in `docs/rare-evidence.toml`:
eligibility, separate occurrence/tier/item stages, six tiers, a separate due
real-time opportunity, and at most one rare per kill. Unknown rates, tier weights,
interval distribution and timer initialization/reset must be explicitly configured
with evidence labels. The shipped draft is disabled; neither 1/2500 nor ACE/GDLE
probabilities are silently enabled. The evaluator returns a proposed state; only
a confirmed atomic corpse/item/player transaction may advance the owned character.
These native table mechanics do not claim the full original retail treasure
mutation algorithm or complete stock-client playability.

## Prepared ACE generator and death treasure

`DeathTreasure` now owns the pinned ACE death-treasure selection and property
mutation pipeline. It runs item, magic-item, and mundane rolls in source order;
selects WCIDs from the extracted source tables; applies the 51 original weapon,
caster, and armor mutation scripts; and applies material/color, workmanship,
gems, spells/cantrips, mana, wield requirements, value, cloak, pet, and mundane
add-on rules. `TreasureAssets` holds immutable source templates, ordered imported
material/gem-count rows, verified DAT clothing palettes and spell metadata,
scroll lookup, and parsed mutation scripts. Preparation performs no owner-tick I/O.
Literal pinned ACE tables, enum/spell indexes and all 51 mutation scripts are
compiled from native TOML into accepted `.bace` namespace 52, kind 27/schema 1.
Cold startup installs the selected immutable set before treasure preparation;
missing/corrupt data fails startup. Table-set changes require a game-child
restart; the active child does not swap its process-wide lookup set.
New world builds contain the pinned profile (ID 1). Publication of a revised
profile to an already accepted world is not yet supported.
Missing required dependencies return an error. A failed generation preserves the
caller's random cursor and publishes no partial item collection. Material-table
misses retain the source fallback; missing required clothing/spell/template assets
do not silently substitute content. Scroll rejection sampling is capped at 4,096
attempts, and one generated collection is capped at 4,096 items.

`WieldedTreasure` follows the active recursive
`WorldObject_Equipment.GenerateWieldedTreasureSets` implementation, including
skipped-subset entry draws, selection boundaries, palette/shade overrides, and
stack variance. `materialize_create_list` applies selected create-list properties.
These pipelines return complete proposed source properties; the simulation owner
still must admit identities, geometry, inventory/vendor destinations, and durable
acquisition before reporting those separate operations as successful. Native rare
policies remain separate and are not changed by ACE ordinary-treasure generation.

Compatibility evidence is independently generated from official ACEmulator/ACE
`47edade3bd3f6044b676d4eb877c4965c7eda62b`, principally
`Source/ACE.Server/Factories/LootGenerationFactory*.cs`, `Factories/Tables`,
`Factories/Entity/ChanceTable.cs`, `Entity/Mutations`, and
`WorldObjects/WorldObject_Equipment.cs`. Original ACE contributors' source and
scripts retain AGPL-3.0-only attribution. No DAT assets are included.

The C# oracles under `oracle/selection`, `oracle/materials`, `oracle/value`,
`oracle/armor`, and `oracle/cloak` extract and execute the original methods against
synthetic immutable inputs. Each `generate.py` accepts `--source PATH --dotnet PATH`.
Their fixtures cover WCID routing/draw counts, material fallbacks and DAT palette
intersection, floating-point color totals, value and burden, armor routing and
ratings, and complete cloak mutation. `oracle/spells`, `oracle/mutations`, and
`oracle/tables` independently cover spell/mana rules, script parsing/evaluation,
and extracted table data; their READMEs document regeneration. The generator in
`oracle/treasure_generate.py` covers active wielded sets and vendor stock behavior.
These fixtures establish the cited source behavior with synthetic assets; they do
not claim that a particular proprietary DAT/content combination or stock-client
session has been qualified.

`generate_creature_equipment` prepares the two constructor equip passes in pinned
`Creature.cs`, `Monster_Inventory.cs` and `Creature_Equipment.cs`: grouped Wield
create-list selection, clothing/armor priorities, source list-sort tie ordering,
weapon shuffle, ammunition and shield selection, followed by wielded treasure.
`append_creature_inventory` adds DID33 treasure after those passes. Prepared items
retain ephemeral destination/drop policy, parent indices, placement slots and
successful equip order. Container contain lists and sidepack overflow are bounded
by 1,024 items and 16 nesting levels. Source-generated objects rejected by capacity
or detached by failed clothing equip are discarded before identities are allocated.
The explicit plain-Wield death-drop policy defaults to ACE's configured false;
DID32-created objects retain source Undef destination. Bonded Destroy filtering
and source drop flags have original C# fixtures. `materialize_create_list_tree`
provides the same complete immutable container expansion for death trophies.
`generate_create_list_selection` compares full-precision draws with source f32
cumulative probabilities; it preserves the initial draw even for an empty list.

`oracle/equipment/generate.py` compiles the original selection methods and drop
filter with synthetic ownership adapters. Its 120 selection cases cover both
passes, armor ties up to 65 candidates, clothing, dual wielding, ranged ammunition,
and shields. Runtime admission separately binds reserved IDs and verified geometry;
item-spell activation and physical modifier consumption remain simulation concerns.

`materialize_container_tree` preserves an already-mutated root and expands the
non-Creature Container constructors' CreateList entries. Container destinations
1/9 apply positive Shade without a probability roll; missing child templates are
skipped as the source factory does. Inventory capacity may move a child into an
existing side pack while GeneratorID still points to the original creating
container. Recursion and total items are bounded, and a rejected tree returns no
partial output. The original `Container.GenerateContainList` and both
`TryAddToInventory` bodies are compiled by `oracle/containers/generate.py`; 45
fixtures cover capacities, source unstable equal-key pack sorting, shade/stack
mutations and factory subtypes derived from the pinned factory and inheritance.
Two added vectors execute pinned `Container.TryAddToInventory` with the complete
Patches WCID 30997 capacity sentinel: both capacities at -1 reject direct
insertion, while a variant with ItemCapacity -1 and one container slot accepts
an item into an available side pack. Capacity application treats -1 as zero
available slots; the authored signed properties remain unchanged. Values below -1 are
rejected by BetterACE's content admission bound.

`player_olthoi` implements the pinned `LootGenerationFactory_OlthoiPlay`
`RollSlag`, tier heuristic and `RollGland` draws. `oracle/player_olthoi.py` executes
the original C# methods and source away-from-zero float rounding; 100 vectors
cover tier edges, one-hour accrual, repeat drops, Vitae and timestamp mutation.
The bounded random cursor commits only on success. An adversarial repeat stream
exhausting the 1,024-draw bound returns an error without consuming the cursor.
