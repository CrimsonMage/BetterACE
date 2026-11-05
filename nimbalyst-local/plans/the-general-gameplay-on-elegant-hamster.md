# BetterACE gameplay parity: coordinated Haiku worker program

## Context

BACE has strong foundations (428k lines of Rust: wire, transport, saves, packs, many subsystem slices), but live gameplay is thin. `docs/gameplay-parity.toml` lists most features as `foundation` or `scaffolded`. The `authoritative-world` and `full-gameplay-acceptance` gates are `unsupported`. Door and generic object Use are marked unsupported in `docs/divergences.toml`. 225 of 618 pinned ACE sources in `docs/network-coverage.toml` are `unsupported`.

The goals:
- every client GameAction works, and every spewbox message is emitted;
- complete content and DB loading;
- server-authoritative movement driven by DAT motion tables;
- GDLE-style door physics;
- every gap closed, both those in the divergences register and those found in the retail client decompile.

Constraints from you:
- The server must stay fast and cheap on CPU and memory.
- Pinned ACE and GDLE serve as oracle tests while we implement.
- PostgreSQL stays the only database.
- Content lives in mmapped immutable `.bace` packs, and those reads must be fast too.

All of this is already normative in ARCHITECTURE.md (SIM-01, COST-01, PACK-01..03, STORE-01). The plan keeps those rules as hard gates and does not reopen them.

**Answer: yes, it is feasible as a coordinated program.** It has one condition: Haiku workers only get narrow tasks with a working template, an oracle, and a mechanical acceptance check. Cross-cutting design stays with the coordinator: movement authority, door physics contracts, gameplay-api types, and pack namespaces.

## Reference sources (read-only)

- **ACE @ pinned `47edade`**: the local checkout `~/Documents/GitHub/ACE` sits at a different HEAD, but it contains the pin. Workers read pinned files with `git -C ~/Documents/GitHub/ACE show 47edade:Source/...` and never check out or edit.
- **GDLE @ `353cbab`**: not present locally. Setup step: clone `gitlab.com/Scribble/gdlenhanced` to `~/Documents/GitHub/gdlenhanced` (next to ACE) and check out the pin. Workers only read it and never edit it.
- **Retail client**: `/home/jokerfactor/Documents/acclient-source-split/INDEX/`. Follow `TINY_AI_SCOPE_GUIDE.md` (summary → family → pack manifest → file). The relevant manifests are 07 (network protocol), 08 (world/physics), 09 (client core) and 10 (gameplay).
- **"deviations.md"**: the redacted register in `docs/divergences.toml` (108 observations; the original `divergences.md` is not in the repo). Owner precedence is in `docs/baselines.toml` `[authority_policy]`:
  - ACE is the wire authority.
  - GDLE is the authority for doors, monsters, cast movement and combat.
  - Verified retail evidence overrides both.

## Worker model

- **Agent definition**: add `.claude/agents/bace-worker.md` (`model: haiku`, `effort: medium`, tools: Read/Edit/Write/Bash/Grep/Glob). Its system prompt embeds the AGENTS.md rules most likely to be violated:
  - FILE-01/02: thin `lib.rs`/`main.rs`.
  - FILE-03/04: 1,000/1,500-line limits.
  - OWN-01: dependency allowlist.
  - WIRE-03: no self round-trip as evidence.
  - QA-03: no `todo!()`, no fake success.
  - COST-01: no per-tick allocation.
- **Concurrency**: about 6 workers in the shared tree (no worktrees), each owning a disjoint file set (OWN-04). Workers run only `cargo check -p <crate>` and `cargo test -p <crate> <filter>`, because the Cargo target lock serializes builds anyway.
- **Communication**: each worker is spawned with a stable name (e.g. `w-movement`, `w-chat`, `w-doors`) and has SendMessage, so it can reach other workers and the coordinator (`main`).
  - A worker that needs a change in another worker's files, or in a shared contract, messages that owner instead of editing those files. Shared contracts are `bace-gameplay-api` types, `opcode_action.rs` and the status ledgers.
  - Gameplay-api changes go through me, and I make or approve them.
  - Each task card names the neighbouring workers whose files or contracts it touches.
- **Coordinator (me)**:
  - Assigns tasks and reviews every diff.
  - Runs the full QA-01 gate (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo xtask check`) once per batch.
  - Commits each green batch through the commit-proposal tool. Nothing red is committed, and workers never commit themselves.
  - When a worker finishes, I reuse it through SendMessage with the next narrow task.
- **Task card format** (every assignment):
  - owning crate and exact files;
  - template path to copy;
  - pinned ACE/GDLE source lines;
  - oracle fixture to produce;
  - acceptance commands;
  - status-doc rows to update (`gameplay-parity.toml`, `network-coverage.toml`, `divergences.toml` `bace_status`).

## Templates workers follow

- **Inbound action vertical slice**:
  - The router is a cascade in `crates/application/bace-runtime/src/game_runtime/ingress.rs`. Each subsystem router returns `Accepted`, `Blocked` or `Unsupported`.
  - Decode lives in `crates/network/bace-wire/src/*_input.rs`. All 163 ACE action opcodes already exist in `opcode_action.rs`.
  - Session typing lives in `crates/network/bace-session/src/<family>.rs`.
  - Then the typed command goes into `bace-gameplay-api`, and the mutation runs in `bace-simulation`. The subsystem crate makes the decision, `bace-replication` projects the result, and the `bace-runtime/src/game_runtime/<family>/` output carries it to the client.
  - Pick one existing working family per task as the template, e.g. `doors.rs` or `inventory.rs`.
- **Oracle pattern**: copy the existing `crates/*/oracle/*.py` plus `tests/fixtures/*.{tsv,hex,csv}` style, e.g. `crates/simulation/bace-simulation/oracle/appraisal_roll.py`. The oracle is derived independently from the ACE/GDLE source, not from the Rust code (QA-02).

## Phases

### P0: Setup and gap ledger (coordinator plus ~6 read-only Haiku tasks, parallel)

1. Clone GDLE at the pin and add the worker agent definition.
2. Build `docs/action-coverage.toml`: one row per ACE `GameActionType` with status `working`, `decoded-closed` or `missing`, owner crate, ACE/GDLE handler path and client INDEX file. One Haiku task per family:
   - movement
   - inventory
   - combat
   - magic
   - chat/social
   - fellowship/allegiance
   - trade/vendor
   - housing
   - character/options
   - admin
3. Build a spewbox catalog ledger. Track coverage of the pinned `WeenieError`/`WeenieErrorWithString` enums, `SystemChat` 0xF7E0, `CommunicationTransientString` 0x02EB, HearSpeech/Ranged/Emote/Soul, Tell, ChannelBroadcast and PopupString. Record which ACE call sites emit each message.
4. Triage the divergences register: map all 108 observations to an owning crate and task, or mark them "needs retail evidence".
5. Run a client INDEX gap pass over manifests 07, 08, 09 and 10. List client-side message handlers and behaviors that have no BACE counterpart.

### P1: Authority core (coordinator designs; Haiku fills leaves)

This phase blocks everything physical.

- **Movement and motion tables**:
  - DAT MotionTable → `bace-motion` interpreted/raw motion state.
  - Velocity is driven by animation, at 30 Hz on the single simulation thread.
  - MoveToState and AutonomousPosition are treated only as intent or reconciliation hints (SIM-02/04).
  - Workers port leaf functions (motion-table lookup, run-rate, jump impulse), each against an ACE/GDLE oracle and kept beside a scalar reference.
- **Doors (GDLE)**:
  - Door state machine with open/close/locked states.
  - Ethereal toggle gated by collision occupancy.
  - Auto-close timer on explicit simulation time.
  - Motion On/Off broadcast, plus lock/key/lockpick checks.
  - Extends the existing `bace-session/src/door.rs` and `game_runtime/doors.rs`.
- **Performance gate for P1**: add a criterion-style benchmark (or extend the existing one) at the ARCHITECTURE.md target of 100 moving players plus 1,000 bodies, p99 ≤ 25 ms. Hot loops must not allocate per tick.

### P2: Breadth on client actions and spewbox (bulk of Haiku tasks)

- One task = one opcode, or one tightly related pair, taken from the `action-coverage.toml` rows marked missing or decoded-closed.
- Order follows dependency: chat/emote/social, then use/inventory/give/containers, then options/UI, then combat, then magic, then fellowship/allegiance/trade, then housing/minigames.
- Spewbox becomes one shared typed catalog (one owner: `bace-wire` codec plus a `bace-replication` projector). Workers add enum ranges and their ACE call-site emissions in batches.
- Every task needs:
  - golden bytes from the ACE encoders;
  - an invalid-input test (WIRE-04);
  - a status-row update.

### P3: Content and DB loading

- Close each missing content category through the existing native pipeline: legacy SQL → `bace-import` → TOML → `bace-content-tools` → `.bace` namespace → bounded mapped preparation in `bace-runtime`. The categories are emotes, recipes, spells/components, quests, events, house data and remaining weenie types. One worker per category.
- Each loader must use checked mapped views with no whole-world decode (PACK-01). Add a startup-time and RSS measurement per namespace.
- PostgreSQL character/item load and save stays in `bace-db-postgres`. Workers add only the DTO and migration rows a feature needs, using frozen versions (DATA-02).

### P4: Divergences and client-INDEX gaps

Work through the P0 triage ledger. Each closure needs:
- a source citation (retail INDEX, GDLE or ACE, per the precedence order);
- a valid-client regression test and an invalid-input regression test;
- an updated `bace_status`.

Items that still lack retail evidence stay labelled uncertain (AUTH-04, QA-03).

## Critical files

- Router and template: `crates/application/bace-runtime/src/game_runtime/ingress.rs`, `game_runtime/<family>.rs`.
- Wire: `crates/network/bace-wire/src/opcode_action.rs`, `*_input.rs`, `chat.rs`, `social_output.rs`, `error.rs`.
- Session: `crates/network/bace-session/src/{door,locomotion,social_actions,inventory,...}.rs`.
- Authority: `crates/simulation/{bace-motion,bace-physics,bace-world,bace-simulation}`, `crates/gameplay/bace-interactions`.
- Content: `crates/content/{bace-import,bace-content,bace-content-tools}`, `crates/storage/bace-storage-codec`.
- Ledgers: `docs/gameplay-parity.toml`, `docs/network-coverage.toml`, `docs/divergences.toml`, new `docs/action-coverage.toml`, `docs/split-plans.toml`, `architecture.toml` (new ownership only).

## Verification

- **Per task (worker)**: `cargo check -p`, the focused `cargo test -p`, the oracle fixture regenerated from ACE/GDLE source, and the file-size check.
- **Per batch (coordinator)**: the full QA-01 suite (fmt, clippy `-D warnings`, `cargo test --workspace`, `cargo xtask check`). When pack, content or persistence paths change, also run the PostgreSQL integration suites and the approved-DAT suites.
- **Performance**: run the tick benchmark after every P1/P2 batch that touches the simulation hot path. Reject any regression in p99 tick time or RSS.
- **End-to-end**: the ARCHITECTURE milestone C acceptance (two stock clients create, enter, move/collide/jump, portal, use doors, chat, save/reconnect), run by you. No playability claim is made before that.
