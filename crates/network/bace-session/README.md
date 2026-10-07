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
