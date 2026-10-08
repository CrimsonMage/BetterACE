# bace-session

Implemented session lifecycle kernel matching the named official ACE states and flag gates. The adapter supplies monotonic time and a cryptographically random challenge. `begin_verified_login` is callable only after account verification by `bace-auth`. Connect-response checks bind the cookie and source IP before recording the send endpoint; repeated responses, premature transitions and expired handshakes fail without advancing the state.

Official references: [Session.cs](https://github.com/ACEmulator/ACE/blob/47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Server/Network/Session.cs), `Enum/SessionState.cs`, `Managers/NetworkManager.cs`, `Handlers/AuthenticationHandler.cs` at the same pin. Lifecycle tests cover transition rejection; `bace-compat` independently checks the connect-request payload against the unmodified C# serializer.

This kernel does not verify passwords, allocate player identities, send character lists, load characters, implement DDD, or authorize a playable world session. The runtime must not expose it as a working authentication service. World entry is a server-only transition after ownership/assets/world commit. Validated-activity timeout refresh and a bounded generation-fenced registry are implemented; complete character/world lifecycle composition remains deferred.

`validate_password_login` now applies official client version 1802 and account-length gates, explicitly rejects unsupported authentication types, and returns borrowed redacted credentials. The account-override field grants no impersonation. Actual password/account-disabled validation MUST still occur through `bace-auth` before challenge initiation. Empty/control account names and empty passwords are rejected for native fresh accounts.


`SessionRegistry` owns bounded endpoint slots and fresh generations. Expiry marks
work for termination but does not release an active world slot; owner-side drain
completion is required. Late authentication or drain results cannot change a
reused slot. `AccountSessions` keeps one account owner and at most one pending
replacement; old-owner drain is mandatory before promotion.

`decode_progression` maps the three implemented XP raise actions to typed gameplay
requests using authenticated identity. Wrong-state, malformed, invalid-target and
unsupported actions are explicit errors. The simulation owner independently
checks active binding and replay before changing progression. Other action IDs
are cataloged, but their dispatch is not implemented.


Generations are process-wide, allocated once per admitted session with a checked
atomic increment. Replacing a network registry cannot reuse a generation while
old authentication, simulation or drain work remains in the process. This is
session fencing only; durable ownership still uses repository epochs.

`decode_combat` now routes targeted melee, mode, cancellation and health-query
inputs into `bace-gameplay-api::CombatRequest`, using authenticated actor/account
identity and the decoded action sequence. It only accepts WorldConnected and
retains ignored suffix counts. It does not validate physical state, finite power,
attack cooldowns or replay: those checks belong to the single simulation owner.

`decode_world_control` permits LoginComplete/ForceObjectDesc only after server-side
world admission. `decode_door_use` maps the existing independently verified Use
body into a bound `UseDoor` intent after the application selects the door route.
The simulation still checks real object kind, reach, replay and obstruction;
client use does not set open state or collision flags.

`decode_magic` decodes targeted/untargeted cast actions only in WorldConnected,
binds the caster from the authenticated session and preserves target/spell IDs as
untrusted proposals. There is no invented client cancel-spell opcode. Domain
validation, motion and resource commitment remain simulation responsibilities;
`tests/magic.rs` covers actor binding, state admission and truncated payloads.


Recall input now covers all seven pinned zero-field handlers: lifestone,
marketplace, personal/allegiance housing, allegiance hometown and both arenas.
Fourteen compiled-original C# cases verify routing and ignored suffix lengths;
packet budgets and world-session binding have separate invalid-input tests.
Destinations, permissions, motion preparation and durable execution remain with
the runtime/simulation owners. Decoder coverage does not establish playable recalls.

`decode_locomotion` binds MoveToState, Jump and AutonomousPosition to the entered
session and preserves wire epochs for the canonical replication owner to check.
Reported position, velocity, contact, object IDs and spell IDs remain diagnostics;
the typed simulation request contains no accepted physical state. Raw physics
state defaults follow the pinned ACE physics `RawMotionState.InitDefaults`.
Action nodes are observation-only on this path; client emote admission is not
implemented by the locomotion adapter. Nonfinite observations, excessive action
counts and trailing bytes reject before command admission (the last is an explicit
hardening boundary beyond ACE's ignored suffixes). Tests use consumed prefixes from
the independently compiled official C# movement decoder corpus.

The five corpse consent actions now leave `decode_social_action` as distinct
`CorpseConsentRequest` values under the authenticated session and action
sequence. Decoding does not alter a player's consent list or a corpse's rights.
