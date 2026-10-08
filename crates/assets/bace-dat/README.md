# bace-dat

Status: foundation with bounded archive, terrain and character-table readers.

`DatArchive` reads DAT headers, B-tree records and checked sector chains. `Landblock` decodes outdoor terrain. Authoritative setup/collision, motion and dungeon geometry decoding remain incomplete; missing collision/motion assets still block world entry.

Character/progression resources now decode the complete pinned ACE layouts:

- `XpTable` (`0x0E000018`): inclusive attribute/vital/trained/specialized rank tables, 64-bit character level XP and level skill credits.
- `SkillTable` (`0x0E000004`): packed hash-table entries, UTF-8 PStrings with record-relative DWORD alignment, signed trained/specialized costs, six formula words and finite double bounds/modifiers. Specialization cost includes training cost. The parser does not call ACE's separate `AddRetiredSkills` helper or synthesize absent entries.
- `CharGen` (`0x0E000002`): starter areas/positions, heritage credit budgets, creation skill overrides, templates, gender metadata and full nested appearance/gear choices. SmartArray counts, BinaryReader strings, compact known-type IDs, palette expansion and coordinate/quaternion ordering follow the source. Heritage costs are creation overrides and must not replace SkillTable costs for later training. Appearance/outer marker bytes skipped by ACE are preserved without invented version semantics.

Each type exposes `decode(bytes)` and `decode_with_limits(bytes, limits)`, plus `load_verified(archive, DatTableVersion)`. Loading requires a Portal dataset, exact caller-approved engine/game versions, and exact record iteration before reading. The records do **not** contain an independent schema-version field. `DatTableVersion` matching is not fingerprint admission: production composition must independently verify the immutable DAT against an approved asset manifest before trusting those expected values. The ordinary `decode` APIs validate layout/record ID, not source authenticity.

`DatTableLimits` defaults to 16 MiB per record, 65,536 entries and 1 MiB per string. CharGen additionally shares one aggregate nested-entry budget. Counts are bounded against remaining bytes before allocation; malformed offsets/counts, duplicate keys, negative XP counts, nonfinite numeric values, unexpected table IDs and trailing bytes return errors. Decoders do not perform world mutation, infer gameplay formulas, spend XP or accept a client-selected cost.

Provenance: official ACEmulator/ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `ACE.DatLoader/FileTypes/{XpTable,SkillTable,CharGen}.cs`, `Entity/{SkillBase,SkillFormula,HeritageGroupCG,TemplateCG,SexCG,ObjDesc,...}.cs`, `BinaryReaderExtensions.cs` and `UnpackableExtensions.cs`. All upstream attribution and AGPL-3.0-only licensing are retained. [Official DAT loader](https://github.com/ACEmulator/ACE/tree/47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.DatLoader) is authoritative.

Independent golden evidence is in `tools/bace-compat/fixtures/dat.json` and `tests/dat_vectors.rs`. `oracle/dat_generate.py` verifies every C# input against immutable pinned GitHub bytes, compiles **unmodified** official table/nested decoders, and records source hashes. The fixture builder uses only synthetic records, including multi-byte string/count boundaries, Unicode strings, negative skill cost preservation, extended resource IDs and 64-bit XP. The sole dependency stub is an unused Frame convenience-constructor parameter type; it does not parse bytes. These tests compare every exposed synthetic table/appearance field and exhaustively reject truncated synthetic inputs.

`cargo test -p bace-dat` covers malformed/limited records and explicit archive/version/iteration rejection. For supplied assets, set `BACE_DAT_DIRECTORY` and run `cargo test -p bace-dat --test tables supplied_portal -- --ignored --nocapture`. That explicit smoke suite fingerprints the user's archive, loads all three tables and checks sampled truncations across actual records. It has passed on the supplied Linux archive (191 attribute entries, 197 vital entries, 209 trained entries, 227 specialized entries, 276 levels, 38 skills, 13 heritages). No proprietary bytes are committed. This is table-format evidence, not full stock-client character creation or movement acceptance.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

## Static inspection resources

Bounded safe Rust readers also expose `ModelSetup`, `GraphicsObject`, `DatSurface`,
`DatTexture`, texture lists, `ClothingTable`, `DatPalette`, `DatPaletteSet` and
`VitalTable` (`0x0E000003`). Setup placements, graphics vertices/polygons/UVs and
clothing/palette references drive the Studio inspector. Drawing BSP branches
are checked and skipped; physics BSPs and their polygons are retained for separate
collision preparation. Recursive BSP
walks have a depth limit; array allocations share bounded table budgets. Texture
envelopes cap dimensions/pixel counts; pixel expansion belongs to Studio.

Source authority is the same pinned ACE.DatLoader, specifically
`FileTypes/{SetupModel,GfxObj,Surface,Texture,SurfaceTexture,Palette,PaletteSet,
ClothingTable,SecondaryAttributeTable}.cs` and their nested entities. Independent
synthetic fixtures in `bace-compat/fixtures/dat_visual.json` are generated by
compiling the unmodified verified C# decoders (`oracle/dat_visual_generate.py`).
`dat_visual_vectors.rs` checks fields and truncated records. The oracle is a test
reference, not a runtime bridge. Supplied-DAT Studio tests verify selected complete
asset chains; they do not establish all possible asset variants or client parity.

## World geometry and animation assets

`EnvCell`, `Environment`, `BspTree`, `MotionTable` and `Animation` now decode the
pinned official layouts with explicit byte/count/nesting budgets. Environment
records retain indexed vertices/UVs, drawing and physics polygons, portal polygon
IDs, and all three BSP tree kinds. BSP ownership is a flat node array; parse depth
is capped at 128 and malformed nodes do not create recursive-drop chains.
`EnvCell` retains surface/environment references, cell transforms, portals,
visible-cell IDs, static object transforms and optional restriction IDs. The word
upstream skips as a repeated cell ID is preserved without invented equality rules.

Motion data retains signed low/high frame boundaries, negative playback rates,
style/cycle/modifier/link maps, and optional velocity/omega. Animation retains
position frames, part frames and all 26 implemented upstream hook kinds with
typed payloads, including attack cones and directions. The attack-hook iterator
exposes authored frame indices; it does not schedule damage or infer hit timing.
Replacement-object hooks preserve their raw ushort part index and also expose
ACE's narrowed low byte. Unknown hook/node layouts, duplicate keys, nonfinite
float inputs, excessive nesting/counts, missing polygon vertices/UVs, truncations
and trailing bytes fail explicitly. NoOp/unknown hooks that upstream fails to
consume are rejected rather than fabricating success.

Independent evidence: `tools/bace-compat/oracle/dat_geometry_generate.py` verifies
immutable source bytes against official ACE at the baseline pin and compiles the
**unmodified** EnvCell, Environment, BSP, MotionTable, Animation and nested parsers.
The synthetic harness covers every admitted BSP tag in all three kinds, both cell
optional sections, two environment cell geometries, all motion optional fields,
reverse playback and all 26 implemented animation hook types. Tests compare every
exposed decoded field and reject every truncated fixture. Unused Position,
DatManager and logging dependencies are stubbed; no parser or formula is stubbed.

Explicit Linux tests on the approved local DAT fingerprints passed: 256 sampled
EnvCells with 666 portals and 125 static objects; all 436 motion tables; 72 sampled
animations with 35 attack hooks; all 772 environment models containing 3,168 cells
and 47,894 physics BSP nodes. No proprietary bytes were added to fixtures.

```sh
cargo +stable test -p bace-compat --test dat_geometry_vectors
BACE_DAT_DIRECTORY=/path/to/DATs cargo +stable test -p bace-dat --test geometry -- --ignored --nocapture
```

These are asset-format results. Full setup collision primitives, outdoor/object
collision preparation, animation-driven simulation and stock-client traversal
qualification remain separate work. Production asset admission must verify the
approved fingerprint/version and run cold reads/decoding off the simulation
thread. Decoders never perform I/O, create world actors or disable collision when
assets are absent.

`ClothingTable::template_order` retains authored palette-template order alongside
checked keyed lookup, so copying/merging a DAT table does not change its first-template
fallback. Studio resolves explicit selections; JSON/TOML interchange preserves this order.

`CollisionSetup` now independently decodes and retains the full pinned SetupModel
layout for collision preparation: parts/parents/default scales, holding and
connection frames, placements with animation hooks, cylinders, spheres, height,
radius, step-up/down heights, sorting/selection spheres, lights and default
resource IDs. Existing visual `ModelSetup` remains separate. The decoder preserves
source values; physics admission validates collision semantics rather than
inventing shapes from visual bounds. The independent geometry C# oracle now also
compares every Setup field and every truncated synthetic input. An explicit
approved-DAT test decoded all 5,935 setup records containing 4,277 spheres and 859
cylinders. This supplies geometry inputs; it does not automatically instantiate
static-object collision or implement dynamic door transforms.

`SpellTable` (0x0E00000E) and `SpellComponents` (0x0E00000F) decode obfuscated
CP1252 strings, every base spell/component field, conditional enchantment/portal
metadata, encrypted formula words and sparse spell-set tiers. The signed-byte
legacy hash and account taper substitutions for formula versions 1–3 preserve
pinned ACE arithmetic. Unsupported formula shapes, zero divisors, duplicate IDs,
nonfinite numbers and exceeded nested/string/record budgets fail explicitly.
Spell-set predecessor lookup replaces the upstream potentially unbounded
HighestTier expansion. These algorithms are legacy content transformations,
not cryptographic gameplay randomness.

`tools/bace-compat/oracle/dat_magic_generate.py` compiles the **unmodified** pinned
ACE SpellTable/SpellBase/SpellComponent/SpellSet parsers and account formulas with
synthetic binary inputs. `dat_magic_vectors.rs` compares fields, CP1252/hash cases,
all formula versions, sparse tier lookup and every truncated synthetic record.
No actual-asset spell-table qualification is claimed yet. Component-to-WCID
DualDidMapper and complete runtime account/foci preparation remain separate work.

`CombatManeuverTable` preserves authored maneuver order and all five DWORD fields,
including minimum skill. Independent unchanged official C# parser fixtures compare
all fields and exhaustive truncations. `RegionLand::decode_prefix` explicitly
projects only the RegionDesc header and LandDefs height lattice; it does not claim
to decode the remaining sky/time/terrain-description sections. `GraphicsObject`
now retains physics polygons and its bounded physics BSP instead of discarding
those fields. Runtime collision preparation remains a separate owner.

The movement oracle (`tools/bace-compat/oracle/movement_generate.py`) verifies
source bytes against official ACE's immutable pin. Supplied-asset tests decoded
all 71 CMTs (886 maneuvers), prepared 128 sampled terrain blocks with the actual
height LUT, and resolved the human combat table to 102 motion timelines containing
171 directional attack hooks. These results establish tested asset preparation,
not complete world-static composition or stock-client movement qualification.

`QualityFilter` decodes the complete eleven-list enchantability record
`0x0E010001`, including source order and duplicate entries. Counts share an
aggregate budget; negative-as-u32 counts, truncation and trailing bytes reject.
Its GDLE `allows_int`/`allows_float` queries retain the source 512-stat bound.
`oracle/quality_filter.py` compiles the unchanged pinned ACE decoder against a
synthetic record; the regression checks every list and every truncated prefix.

DualDidMapper additionally supports an explicitly expected `0x27` record identity;
component-only decoding remains fenced to `0x27000002`. The material record
`0x27000000` uses the same bounded source layout. Independent original C# fixtures
in `tests/fixtures/materials_mapper.*` include provenance for DualDidMapper and
RecipeManager.GetMaterialName; tests reject mismatched identities, other record
classes, every truncation and trailing bytes. Fingerprint admission remains the
cold adapter's responsibility.
