# bace-social

BetterACE social preferences, indexed identities/friend watchers, squelch masks,
and recipient policy. The simulation owns accepted chat and bounded delivery;
this crate neither sends packets nor persists evolving gameplay structures.

`state` maintains one preferences owner per admitted character and retains cold
identity records after logout. `preferences` proposes bounded changes and checks
exact before-state on adoption. `routing` validates source text and applies
recipient/channel/squelch policy. Cold friend identities are prepared explicitly;
missing identities do not silently disappear from full-list output.

Policy provenance is official ACE at the pin in `docs/baselines.toml`, including
`Player_Character`, `Player`, `Player_Networking`, `TurbineChatHandler`,
and `SquelchManager`.
Wire fixtures belong to `bace-wire` and projection tests to `bace-replication`.
Subsystem tests do not qualify a stock-client production login/chat session.

`chat_policy` applies the pinned `TurbineChatHandler` public-channel gate order:
Olthoi restrictions, echo-only, account flag/age, character playtime/level, and
channel disable switches. Time and eligibility are immutable inputs. Historical
unknown account creation times fail closed only when that age gate is enabled.
There is no general chat-rate throttle in the pinned handlers; bounded simulation
outboxes provide overload backpressure. Movement soul-emote deduplication is a
separate animation concern, not a fabricated chat timer.

`oracle/chat_policy.py` compiles original `SquelchDB.Contains`, legal-channel
policy and Turbine channel adjustment (906 vectors). `oracle/public_chat.py`
compiles the original public gate block and rejection helper with an explicit
clock (144 vectors). The original channel number's disable-switch behavior is
preserved. Account/character/global squelches remain distinct; public Turbine
channels test the complete AllChannels mask. Private Tell refusals never become
accepted chat. The source has no staff exemption from those squelches.

`oracle/legacy_chat.py` compiles the original Fellow and CoVassals handler blocks
(64 vectors). Fellowship preserves the source's repeated sender echo for squelched
members; CoVassals delivers a separate Patron-channel message before co-vassal
messages. An unchanged global squelch produces its source notice without an extra
database update event. Cold `cache_identity` inputs carry only verified character,
account and display-name identity and cannot overwrite accepted online presence.
