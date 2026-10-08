# bace-wire

Checked legacy primitives/framing, complete named ACE identifier catalogs and selected typed message families are implemented. Full message-payload and stock-client compatibility remain unsupported.

Public modules implement little-endian readers/writers, 20-byte packet and 16-byte fragment headers, server datagram encoding, transport optional headers, Windows-1252 output strings, packed DWORDs, Hash32, ACE's ISAAC variant/client key window, connect-request/response payloads, and inbound login payload/clear-checksum datagram decoding. `Datagram` exposes ignored trailing-byte count as official ACE does. All network reads are checked; server serialization preflights its 464-byte body budget.

`OptionalHeaders` supports transport fields only. Login, referral, world-login and other control payloads return an explicit unsupported-flags error there. The general string16 decoder decodes server CP1252 strings; the dedicated `LoginRequest` decoder separately preserves ACE's UTF-8/UTF-16-unit login string rules, one/two-byte secret-prefix behavior and optional terminal padding. It preserves unknown auth types and ignored metadata without granting impersonation. Borrowed game-action/event envelopes validate the outer opcode and bound payload size without allocating or pretending an unknown payload is implemented. Echo response includes both client time and delta in upstream optional-header order.

Source baseline: [official ACE at 47edade3bd3f6044b676d4eb877c4965c7eda62b](https://github.com/ACEmulator/ACE/tree/47edade3bd3f6044b676d4eb877c4965c7eda62b). Codec provenance is `Source/ACE.Server/Network/{PacketHeader,PacketFragmentHeader,PacketHeaderOptional,ServerPacket,ServerPacketFragment,Extensions,Packets/PacketoutboundConnectRequest}.cs` and `Source/ACE.Common/Cryptography/{Hash32,ISAAC,CryptoSystem}.cs`.

`bace-compat` compiles unmodified pinned C# sources to generate golden fixtures, including SHA-256 provenance. Tests establish the individually tested framing/message subsets only. No claim of full client compatibility is made.

Intentional hardening: reject truncated fragment bodies, zero counts, out-of-range indexes/queues, strings not representable in CP1252, and packed integers over 31 bits rather than silently losing information. Control codecs are separate from transport optional headers. All such rejections have bounds tests; ordinary encoding has C# golden tests. No proprietary assets or captures are included.

Login hardening rejects malformed UTF-8 and truncated string contents; ignored declared-length metadata and missing final secret padding preserve tested official behavior. Mixed initial-login header flags remain unsupported. Login/password DTO Debug output redacts credentials. Nine synthetic official C# login vectors cover ASCII, accented/astral text, 255/256-unit secrets, absent terminal padding, GLS ticket and account-only forms.

The `opcode` module preserves all 82 message names (including aliases), 163 action names, 104 event names, 21 character-error names and 13 queue names at the pin. Newtype identifiers preserve unknown values, and `NAMED` exposes every upstream spelling. The catalog is evidence of identifiers only; upstream unused identifiers and unimplemented payloads are not automatically supported.

Implemented message families:

The private position update codec implements pinned ACE opcode 0x02DB with a
byte property sequence, DWORD PositionType and exact 32-byte Position payload.
`tests/movement.rs` checks the source layout and truncation/trailing-byte
rejection. Runtime currently uses PositionType 14 for a committed outdoor
corpse's LastOutsideDeath; client playback remains unqualified.

- Character list encoding/checked decoding, character error, conditional create response, restore, delete, logoff and world-server-ready output; server-name UI output.
- System/speech/ranged-speech/emote/soul-emote output; account boot and ban output. A boot with no reason emits only the opcode; an empty reason emits a String16L. Ban remaining time is supplied explicitly, never read from a codec clock. The official ban fixture asserts its measured integral duration; the Rust codec accepts that duration as input.
- Private/public integer, int64, bool, double, string, data-ID and instance-ID property output; attribute, private/public vital, current-vital and private skill output. Public strings preserve the unusual property-before-GUID order and alignment; skill ranks and `adjustPP` are 16-bit.
- DDD interrogation/end/error, bounded begin iteration lists, bounded prepared data output and exact request-data input; bounded interrogation-response iteration-run input. Begin lists preserve ACE's Portal/Language/Cell/HighRes grouping and cell purge semantics. DDD data's compression flag occupies one byte; compression and asset lookup belong to `bace-dat-service`, not this codec.
- Use-done, weenie-error, ping-response and fellowship-update-done game events. Fellowship completion has no payload at this ACE pin, regardless of other retail evidence.

`tools/bace-compat/oracle/message_generate.py` compiles unmodified pinned message serializers, game-action dispatch and iteration decoders. SHA-256 hashes of each upstream input accompany `fixtures/messages.json`; harness adapters supply synthetic domain values instead of databases, assets and clocks. `tests/message_vectors.rs` compares every named identifier and selected payload against this independent C# output. DDD compressed fixtures establish the flag and envelope, not zlib correctness. Whole-session behavior and per-opcode action gameplay remain separate acceptance gates.

Additional deliberate hardening is covered by `tests/messages.rs`: reject envelope size excess, malformed character counts/bools/trailing bytes, DDD count/run overflow and overshoot, and iteration decoding beyond explicit run budgets. Run-length representation stays compressed, preventing allocation by expanded iteration count. ACE's ignored second interrogation list/flags are reported as trailing bytes without invented semantics. `-1` runs that make no progress are accepted only within the explicit run budget. Outbound packet encoding permits ACE's optional-only packet with `BlobFragments` still set; real fragments still require the flag.

Inbound progression decoding covers RaiseAttribute/RaiseVital/RaiseSkill (target and unsigned XP) and TrainSkill (target and signed credits). Official handlers consume eight payload bytes and ignore a suffix, which is bounded and reported in `ProgressionRequest::trailing_bytes`. Attribute/vital targets above `u16::MAX` are rejected instead of reproducing upstream's truncating enum cast; gameplay must additionally validate meaningful target IDs, available XP and training costs. This hardening has valid official action-handler vectors and invalid-input regressions.

`CharacterCreateRequest` decodes the complete official CharacterCreateInfo and Appearance wire layout, including bounded skill classes, UTF-8 strings counted in UTF-16 units, all appearance fields, signed template option and requested privilege flags. The ignored constant and trailing-byte count are preserved. `requested_admin` / `requested_sentinel` are untrusted requests, never authority. DAT-backed legality, account ownership, accepted names and actual character construction are outside the wire crate. C# fixtures cover empty/nonempty skill lists, ASCII/accented/astral names, ignored suffixes and exact `word == 1` flags. SHA-verified evidence-only sources establish full-handler prefixes and domain adapter widths; the fixture metadata distinguishes those from compiled serializers/decoders.

Movement codecs now cover full PositionPack conditionals/counters and UpdatePosition, VectorUpdate and outbound AutonomousPosition. `WirePosition` stores explicit W/X/Y/Z wire order. Inbound `ClientJump`, `ClientAutonomousPosition` and `ClientMoveToState` retain reported velocities, poses, contact, jump flags, epochs and bounded raw command lists as untrusted observations. No accepted pose setter, gameplay timing or physical-state mutation exists in this crate. Numeric validity, legal motion commands and accepted contact/velocity require authoritative physics; NaN bits are preserved by this low-level codec, not accepted as physical state.

Outgoing `MotionUpdate` implements the five ACE serialized branches (state/invalid, move-to-object, move-to-position, turn-to-object, turn-to-heading), including all interpreted-state flags, command sequence/autonomy packing, alignment, sticky target and directed movement parameters. Callers provide counters explicitly; codecs do not advance world state. Unknown movement forms are not guessed. Rejection tests cover oversized/truncated input, high count bits that ACE truncates, unknown output flags, inconsistent sticky fields and overflowing 15-bit command sequences. Client movement terminal DWORD padding is required as explicit hardening (upstream `Align` can seek beyond the available buffer); ordinary padded inputs and truncated-padding regressions are tested.

The movement oracle compiles the original Position, PositionPack, JumpPack, MoveToState, RawMotionState, MotionItem, MovementData and subordinate serializers/decoders and both simple client handlers. The original Vector3 writer is extracted verbatim from SHA-verified AllegianceHierarchy source to avoid compiling unrelated allegiance services; fixture metadata identifies that extraction. Goldens cover all 128 position flag combinations, 256 interpreted state cases (all flag masks and autonomy), all four directed branches and conditional raw input. Domain fixtures supply fixed counters and poses rather than simulation behavior.

User-provided divergence rows 1/35/50 concern replication cadence and row 51 concerns the empty-motion-before-vector jump ordering. They do not change these pinned layouts. A valid empty motion and vector output are individually covered; replication must establish ordering and authoritative timing separately. No retail cadence, accepted movement model or playable stock-client movement is claimed by codec tests.

Object serialization now covers `ObjectDescription::encode_create` / `encode_update`, `AppearanceUpdate` and `ObjectControl` delete/player-create/state/parent/pickup/inventory-remove/stack-size/teleport messages. `ObjectModel` has checked encoding and decoding of the 0x11 header, byte-sized lists, known-type packed palette/texture/animation IDs and DWORD alignment. Palette offset/length fields are raw wire bytes (including the upstream zero representation); projection must perform any asset-level interpretation. A wrong resource prefix is rejected instead of reproducing upstream's lossy typed-ID subtraction.

`PhysicsDescription` encodes all fields emitted by pinned `SerializePhysicsData`: explicit state, embedded motion or animation frame, full position, motion/sound/effect/setup tables, parent/children, scalar material properties, vectors, default script/intensity and all nine ushort counters. `MovementDescription` reuses the previously validated movement body without standalone-message counters. The timestamp words are unconditional, as in ACE. Options specify field presence; projection decides defaults, scale/translucency thresholds, hidden/cloaked/admin visibility and parent ownership before calling the codec.

`ObjectGameData` covers all 32 first-header fields and all four second-header fields with their exact widths/order. Presence is explicit, including present zero values. The second-header marker is added when needed. Housing restrictions use version 0x10000002, advertised bucket count 768 and ACE's actual GUID sorting by `(guid % 89, guid)`; duplicate IDs and count overflow are rejected. Housing resolves owner/allegiance/account privileges into this immutable permission projection before encoding. Model, children, motion, permission and string limits plus a final message-size limit are explicit caller inputs.

Five object oracle suites cover nine model cases (including 255 palettes), 21 physics flag selections, every game header bit, combined create/update composition and fixed controls. Four malformed/bounds suites cover oversized lists, typed-ID mismatch, duplicate permissions, absent palette prerequisites, nested limits and message caps. The oracle compiles original message wrappers, model types, flags and restriction sorting, and extracts seven **unchanged method bodies** from SHA-verified `WorldObject_Networking.cs`; unrelated world services are replaced only by explicitly declared input adapters. Metadata lists extracted bodies separately from whole compiled files. The message generator verifies all compiled upstream files and additional evidence-only sources separately. These tests establish serialization of supplied projections, not correctness of upstream property selection or asset-derived appearance.

User divergence rows 17–19 remain distinct from codec support: explicit physics-state bits are preserved; the appearance-only 0xF625 message is distinct from a full 0xF745 create. Door state transitions and handling ForceObjectDesc for missing objects belong to gameplay/replication. Nothing here assumes retail and the other-revision shard in the user register have identical behavior.


Turbine chat now has a bounded frame/header view, typed inbound request decoding and pinned event/response output. Both output length words intentionally retain ACE's +8 arithmetic, and responses intentionally use ByName dispatch 1 with body response/method IDs 2/2 (user divergence rows 24 and 58, independently verified at this pin). LoginQueue 4 is the output queue. Incoming declared lengths are preserved as metadata, never allocation sizes; actual bytes and UTF-16 units are bounded. Claimed sender IDs remain untrusted. The ByName request form follows ACE's same numeric-channel read rather than inventing a room-name schema. Unknown routing IDs are retained for domain policy to reject.

Turbine output rejects sender names over 127 UTF-16 units and text over 255 units: the pinned source incorrectly uses message length for a long sender prefix and discards length bits for messages >=256. Independent C# fixtures capture those corrupt forms and regression tests require refusal, while valid 127/128/255 boundaries and astral text match exactly. Inbound UTF-16 rejects lone surrogates instead of reproducing .NET replacement characters. Truncated lengths/text/trailers return errors without state mutation; packet and string budgets are explicit.

`SocialRequest` decodes 16 chat, AFK, friend, channel and squelch action bodies, preserving ignored suffix counts and nonzero DWORD booleans. UTF-8 String16L uses UTF-16 unit lengths; names remain untrimmed for domain normalization. Required string padding is enforced. `SocialEvent` encodes tell, channel broadcast/list/index, transient strings, Turbine channel assignments, full/delta friend lists and squelch tables. Friend online/appear-offline decisions are precomputed inputs. Squelch output deliberately has no account-name section, orders merged character/account entries by `(guid % 32, guid)` and preserves explicit filter vectors (including the source's four repeated masks). ChannelIndex preserves zero/multiple ordered privilege lists without inferring privileges.

Seven social oracle suites use 65 synthetic vectors, compiling unchanged simple action handlers and outbound serializers plus verbatim input-reading prefixes from gameplay-heavy handlers and the unchanged SquelchDB serializer class. `message_social_extract.py` and fixture metadata identify those excerpts and omitted policy explicitly. Four boundary suites cover request/UTF budgets, truncation, invalid text, lossy output prefixes, duplicate squelch IDs, nested table limits, friend delta cardinality and final output caps. These are wire guarantees, not live friend persistence, delivery authorization or chat-moderation claims.

The five corpse consent GameActions (0x0216–0x021A) decode the exact empty or
single String16L prefixes read by pinned ACE's unchanged action handlers. This
codec does not grant loot permission; authenticated online policy and durable
corpse access belong to the death owner.


`InventoryRequest` now decodes 19 inventory/use/stack/vendor/trade action bodies with explicit packet and item-count budgets. Signed quantities and placements are retained as untrusted proposals, and the buy/sell trailing currency word remains ignored/reported exactly as in ACE. AcceptTrade's partner, timestamp, status and acceptance claims are decoded for inspection, never treated as authoritative. Zero-payload close/decline/reset requests preserve ignored suffix counts.

`InventoryEvent` encodes 14 inventory/container/trade events, including the register-trade zero int64, add-to-trade zero slot and empty clear-acceptance payload. Container contents are supplied in the stable placement order established by projection. `VendorListing` encodes ApproachVendor with optional alternate currency, explicit totals/prices, default-then-unique stock order and existing game-description encoding. Its fixed high-byte public-description tag and low 24-bit stock quantity match ACE; -1 is supported for unlimited stock, while other negative or overflowing quantities are rejected rather than truncated. These message APIs neither mutate inventory nor acknowledge a durable operation automatically: callers may supply success events only after their authoritative durability checks succeed.

Three inventory/vendor oracle suites cover 19 request and 16 output vectors, using original handlers, event serializers and the verbatim Vendor.forEachItem traversal. The extracted AcceptTrade reader prefix exposes fields that the original handler subsequently ignores; metadata records that distinction. Four regression suites exercise signed quantities, ignored currency suffixes, malicious item counts, nested output caps and lossy stock encodings. Inventory ownership, transaction atomicity, vendor eligibility/pricing and trade close-reason policy remain outside this crate; no gameplay parity is implied by these wire fixtures.

Combat input now covers targeted melee, combat mode, cancel attack and health
query. Values remain untrusted proposals, including float bits and enum IDs.
Combat output covers attack completion/commencement, attacker/defender damage,
evasion, health fractions, kill/death notifications, sounds, scripts and player
death broadcasts. Percentages widen ACE's float input to a double on the damage
notifications; health fractions remain floats. Conditions use the full 64-bit
wire field, including the signed upstream enum's extension. Four original C#
handlers and twelve original serializers provide independent golden fixtures.
Input truncation, suffix/budget behavior and output text/size failures are tested.
The gameplay owner must validate finite power, heights, stance, range and targets.

`PlayerDescription` serializes preselected login property tables, full ordered
attributes/vitals, skills, known spells, options, shortcuts, eight spell bars,
component refill preferences and possessions. Bucket ordering, explicit zero
values, integer widths and skill constants follow pinned
`GameEventPlayerDescription.cs`; nine original-serializer cases cover each
optional family and their combination. Property selection, privacy, name
prefixes and inventory placement order are supplied by authoritative projection.
`PlayerDescription::enchantments` now carries a bounded typed registry. A true
legacy `has_enchantments` flag without supplied registry data still fails rather
than silently omitting persisted state.
`CharacterTitle` and bounded character enter/delete/restore/logoff/request inputs
also have independent source-derived fixtures. Character input account strings
remain untrusted and cannot establish ownership. The original handler's ignored
suffixes are reported; missing required string padding is rejected as hardening.
These codecs do not authorize or commit world entry.

`WorldControlRequest` decodes LoginComplete and ForceObjectDescSend using original
handler fixtures. Portal exit is a world-session observation, never permission to
enter; force-description targets require authoritative visibility/ownership policy.

Magic request readers preserve the exact 0x0048 spell DWORD and 0x004A target/spell
DWORD order, bounded ignored suffixes and untrusted IDs. `Enchantment`,
`EnchantmentRegistry` and nine `MagicEvent` variants preserve pinned ACE field
widths, doubles/floats, category ordering, ushort set-presence flag, optional set
ID and the Vitae layer-zero encoding. PlayerDescription sets vector flag 0x0200
and inserts the registry after known spells. `tools/bace-compat/fixtures/magic.json`
contains independent original-wrapper/verbatim-writer vectors. A compositional
PlayerDescription test joins the independent upstream fresh description and
registry vector at the source insertion point. These are codec results, not a
claim of full spell-effect or login transcript qualification.

Fellowship/allegiance codecs now cover 35 request layouts, full/delta fellowship
records, departed-member bucket order, lock records, full allegiance hierarchy,
update/info prefixes, completion and confirmation output. Twelve unchanged C#
serializers and 35 unchanged action-reading prefixes compile in
`oracle/group_generate.py`; fixture provenance identifies every source hash.
Empty upstream officer/title/MOTD fields and zero age counters remain exact.
Group counts, strings, packet lengths and malformed/truncated inputs are bounded.
`AdvocateTeleport` reads the ignored String16L target and untrusted Position;
32 original-handler vectors cover role, water and building-height decisions.


Recall input now covers all seven pinned zero-field handlers: lifestone,
marketplace, personal/allegiance housing, allegiance hometown and both arenas.
Fourteen compiled-original C# cases verify routing and ignored suffix lengths;
packet budgets and world-session binding have separate invalid-input tests.
Destinations, permissions, motion preparation and durable execution remain with
the runtime/simulation owners. Decoder coverage does not establish playable recalls.

The local client decompile exposes a jump-layout discrepancy: JumpPack::Pack at
0x00516D10 includes a full Position before the four epoch words (56-byte payload),
whereas pinned ACE reads its shorter32-byte Jump handler prefix. These are explicit
separate codecs; the session adapter selects only exact supported lengths. Client
CM_Movement::Event_Jump_NonAutonomous at0x006AFB30 writes only a float extent and
has no epoch or object/spell fields. Eight synthetic byte vectors compile those
original local methods with size/Position/UI adapters; fixtures record file hashes
and adaptations. The local client's build provenance remains unconfirmed. No
retail decompile body, DAT bytes or player capture is committed. Both position and
velocity remain untrusted observations; these codecs grant no physical authority.

Target query codecs preserve QueryHealth/QueryItemMana's u32 target (including
zero), source trailing-input behavior, and UpdateHealth/QueryItemManaResponse
payloads. `oracle/target_query_generate.py` compiles the original event
constructors and base event-header writer; 30 independent packets include NaN,
infinity, zero and high-bit IDs. Incoming buffers remain bounded, truncated u32
reads reject, and unsupported success values cannot be emitted by the adapter.

The appraisal encoder implements original AppraiseInfo serialization, including
all six property tables, source bucket order, enchantment-tagged spell order,
armor/creature/weapon/hook profiles and highlight masks. Its 136 compiled-original
serializer vectors qualify flags, primitive widths and conditional sections;
they do not qualify profile calculation, live assessment rolls or NPC wake-up.
The owner must supply a fully prepared profile before this codec can be used.
