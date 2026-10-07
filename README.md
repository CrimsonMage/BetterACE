# 🚀 BetterACE — BetterACEmulator

> 🤖⚡ **An AI-assisted Rust emulator project powered by packet archaeology, strict ownership, and aggressively documented ✨vibes✨.**

BetterACE reimplements official ACEmulator/ACE in Rust, targeting the unmodified Asheron's Call client. The work spans legacy networking, authoritative simulation, PostgreSQL persistence, native TOML content, and immutable `.bace` packs.

[![🦀 Rust](https://img.shields.io/badge/Rust-1.96.1-orange?logo=rust)](rust-toolchain.toml)
[![CI](https://github.com/CrimsonMage/BetterACE/actions/workflows/ci.yml/badge.svg)](https://github.com/CrimsonMage/BetterACE/actions/workflows/ci.yml)
[![📜 License](https://img.shields.io/badge/license-AGPL--3.0--only-blue)](LICENSE)
[![🤖 README energy](https://img.shields.io/badge/README%20energy-AI%20%2B%20emoji-purple)](#ai-disclaimer)
[![🏰 Status](https://img.shields.io/badge/MMO-not%20yet%20MMOing-lightgrey)](#status)

> 🚧 **Working foundations and usable authoring tools; not yet a playable replacement server.**
>
> The host console, Content Studio, conversion tools, and validation harnesses run today. Complete stock-client world entry, authentic AC collision, gameplay/save composition, and full-world import remain unfinished. `serve` deliberately refuses readiness. See [implementation status](docs/implementation-status.md).

The repository is **BetterACE**. Existing `bace-` executable/crate names, `BACE_*` environment variables, and `.bace` files keep their current spelling. 🧬

<a id="contents"></a>
## 📑 Navigation, because every AI README needs a control panel

- [⚡ Quick start](#quick-start)
- [🧰 Current commands](#commands)
- [🎨 Content Studio](#content-studio)
- [🖥️ Host console and accounts](#host-console)
- [📦 Content conversion and publication](#content)
- [🗺️ User-supplied assets](#assets)
- [🧪 Build, tests, and oracles](#validation)
- [📡 What actually works](#status)
- [🧭 Architecture and roadmap](#roadmap)
- [🤝 Contributing and license](#contributing)
- [🤖 Extremely important disclaimer](#ai-disclaimer)

<a id="quick-start"></a>
## ⚡ Quick start — commands that actually do something

Rust **1.96.1** is pinned. Examples use Bash. PostgreSQL is needed for database workflows/tests; the GUI and offline JSON/TOML conversion do not need a game database. Python 3 and .NET 10 are used by the compatibility tooling.

```sh
git clone https://github.com/CrimsonMage/BetterACE.git
cd BetterACE

# Build the three user-facing executables once.
cargo build -p bace-server -p bace-cli -p bace-content-gui

# Use the built binaries directly in this shell.
export PATH="$PWD/target/debug:$PATH"

bace-cli --help
bace-server --help
bace-server check --config tests/fixtures/config/server.toml
bace-server exercise --ticks 300 --players 100 --bodies 1000
```

Content authors can then launch:

```sh
bace-content-gui
```

Without changing `PATH`, use `./target/debug/bace-cli`, `./target/debug/bace-server`, and `./target/debug/bace-content-gui`. On Windows, the binaries have `.exe` suffixes. Platform-specific limitations are listed below; compiling portable code is not the same as validating every platform. 🧪

For a release build, add `--release` and use `target/release`. For a source-run equivalent, use `cargo run -p bace-cli -- <command>` or `cargo run -p bace-server -- <command>`.

### 🦀 Toolchain reality check™

In the supplied development environment, the named `1.96.1` installation lacks some tools, while `stable` contains the same Rust/Cargo **1.96.1** and complete tools. Use `cargo +stable ...` there, or repair the pinned installation. This is a local workaround, not permission to silently upgrade the compiler.

> ✨ Build once. Type less. The borrow checker still gets a vote.

<a id="commands"></a>
## 🧰 Current command spellbook

These are the implemented public commands. `help` / `--help` gives the exact arguments; internal supervisor commands are not an operator interface.

| Executable | Command | What it does |
|---|---|---|
| `bace-server` | no command, or `host [--config FILE]` | Starts the authenticated local host dashboard and foundation child. |
| `bace-server` | `check --config FILE` | Validates configuration without opening game sockets or a database. |
| `bace-server` | `exercise [--ticks N] [--players N] [--bodies N]` | Runs the synthetic fixed-step simulation harness. |
| `bace-server` | `content-worker --config FILE` | Validates durable publication candidates and delivers catalog generations. |
| `bace-server` | `serve --config FILE` | Reports the unfinished game-readiness gate; does not start a playable world. |
| `bace-cli` | `host-init [--state-directory DIR]` | Provisions separate host credentials from piped stdin. |
| `bace-cli` | `account-create --name NAME [--password-env VAR]` | Creates an ordinary player account using a password environment variable. |
| `bace-cli` | `convert --input FILE --output FILE --from FORMAT --to FORMAT` | Converts one template between supported legacy/native formats. |
| `bace-cli` | `import-sql --input FILE --output-directory DIR` | Converts a supported weenie SQL batch into native TOML. |
| `bace-cli` | `migrate` | Applies native PostgreSQL schema migrations. |
| `bace-cli` | `publish --input FILE --format FORMAT` | Queues an immutable content candidate for validation. |
| `bace-cli` | `content-status` | Reads persisted generation and content/publication counts. |
| `bace-cli` | `dat-inspect FILE [--fingerprint] [--record HEX] [--output FILE]` | Inspects a DAT archive and optionally extracts a record. |
| `bace-content-gui` | launch directly | Opens the native editor, importer/exporter, and pack builder. |
| Cargo | `cargo xtask check` | Enforces architecture, dependency, source-size, and coverage rules. |

CLI input formats are `toml`, `ace-json`, `binary`, and `ace-sql`; CLI output formats are `toml` and `binary`. Legacy JSON/SQL export and folder-to-pack builds currently live in **Content Studio**, not an undocumented CLI subcommand.

Global CLI options: `--database-url-env VAR` defaults to `BACE_DATABASE_URL`; `--mariadb-basedir DIR` overrides `BACE_MARIADB_BASEDIR` for isolated SQL staging.

### 🧹 Command diet: planned, not invented

The command surface needs simplification: fewer repeated flags, consistent configuration discovery, and shorter common authoring/host workflows. That is follow-up work. The table above is today's interface; no imaginary `betterace start-everything` alias is hiding behind the curtain. 🎭

<a id="content-studio"></a>
## 🎨 Content Studio — the GUI has entered the chat

```sh
cargo run -p bace-content-gui
```

Or launch the built `bace-content-gui` executable directly. Current workflows include:

- 📝 New/open/save/save-as, clone-as-new, bounded undo/redo, validation, and a TOML source view.
- 🧩 Forms for 21 property families, with pinned ACE property names and editable unknown numeric IDs.
- 📥 ACE/Lifestoned/GDLE JSON import and supported weenie SQL import through isolated MariaDB staging.
- 📤 Legacy JSON/SQL export bundles with a complete native companion and explicit conversion notes.
- 📦 A **Build** tab that compiles native TOML folders into immutable `.bace` files and a binary generation manifest.

Saves check for external changes and preserve dirty state on failure. Conversion/build output goes into fresh folders. Pack building does **not** publish to PostgreSQL or activate a server catalog.

Linux UI flows have been exercised. Linux file dialogs need an XDG desktop portal backend or Zenity. Windows/macOS validation remains outstanding; legacy SQL staging is currently Linux-only. EmoteScript compilation, death-treasure editing, and automatic gameplay calculators remain unsupported.

See [Content Studio](crates/application/bace-content-studio/README.md) and its [coverage register](docs/content-studio-coverage.toml).

> 🪄 A real editor, real files, and a conspicuous absence of “save succeeded” when it did not.

<a id="host-console"></a>
## 🖥️ Host console and accounts

Provision the host password once with a hidden prompt, then start the dashboard:

```sh
python3 -c "import getpass, sys; sys.stdout.write(getpass.getpass('Host password: '))" \
  | bace-cli host-init --state-directory state/host

bace-server
# Equivalent explicit form:
# bace-server host --config tests/fixtures/config/server.toml
```

Open the printed URL (default `http://127.0.0.1:8080`). Host privileges are separate from game accounts. The dashboard supervises a foundation child that reports game serving as unavailable.

Native Windows host credential/ACL provisioning is not supported yet; macOS execution is unvalidated. Details: [host-console contract](docs/host-console.md).

After setting `BACE_DATABASE_URL` for a dedicated development PostgreSQL database and applying `bace-cli migrate`, create a player account with:

```sh
read -rsp "Initial player password: " BACE_INITIAL_PASSWORD
export BACE_INITIAL_PASSWORD
bace-cli account-create --name ExamplePlayer
unset BACE_INITIAL_PASSWORD
```

A duplicate account does not overwrite its password or privileges. No first-account auto-promotion. Native login policy defaults to automatic ordinary-player creation, configurable off; this does not make the gated game service playable.

```toml
[accounts]
allow_auto_creation = false

[dat_distribution]
enabled = false
```

DAT distribution defaults off. Enabling it requires `dat_directory` and validated assets. Add these sections to a complete server configuration, such as [the example](tests/fixtures/config/server.toml).

<a id="content"></a>
## 📦 Content conversion and publication

Native authoring uses TOML; runtime packs use versioned immutable `.bace` files. Single-template `binary` exports are interchange envelopes, not whole-world runtime packs.

```sh
bace-cli convert \
  --input tests/fixtures/content/minimal.toml --from toml \
  --output /tmp/weenie.bin --to binary

bace-cli convert \
  --input /tmp/weenie.bin --from binary \
  --output /tmp/weenie.toml --to toml

bace-cli convert \
  --input /path/to/legacy-weenie.json --from ace-json \
  --output /tmp/imported-weenie.toml --to toml
```

### 🧙 SQL → TOML, with actual prerequisites

MariaDB is a conversion prerequisite, not a game-server dependency. Use a private installation for disposable staging:

```sh
BACE_MARIADB_BASEDIR=/path/to/mariadb/usr \
  bace-cli import-sql --input /path/to/weenies.sql --output-directory /tmp/converted-weenies
```

The result is native TOML plus a source/count manifest. Unsupported SQL fails explicitly. Complete official-world dumps still contain unsupported systems; there is no “just import everything” button. Arbitrary MySQL SQL never runs against production PostgreSQL.

See [legacy conversion boundaries](crates/content/bace-import/README.md).

### 🐘 PostgreSQL publication

Set `BACE_DATABASE_URL` securely for your dedicated database. Keep real credentials out of command arguments, tracked configuration, and commits.

```sh
bace-cli migrate
bace-server content-worker --config tests/fixtures/config/server.toml

# In another terminal with the same database environment:
bace-cli publish --input tests/fixtures/content/minimal.toml --format toml
bace-cli content-status
```

Candidates are immutable. The worker validates data, quarantines rejected batches, and publishes accepted catalog generations through bounded delivery. Publication does not itself spawn an object or start a world. Direct native SQL insertion follows the same durable publication journal.

> 🤖 **Immutable candidates. Bounded queues. Explicit failures.** The infrastructure has read the architecture document.

<a id="assets"></a>
## 🗺️ User-supplied assets

```sh
bace-cli dat-inspect /path/to/client_cell_1.dat --fingerprint

# Example: inspect the portal XP-table record.
bace-cli dat-inspect /path/to/client_portal.dat --record 0x0E000018
```

`--output FILE` requires `--record HEX`; extracted DAT records must remain local. DAT archives and player captures are excluded from Git.

Archive headers, BTree/sector chains, outdoor landblocks, XP/skill tables, and complete character-generation records have implemented readers. Actual portal data has been decoded and prepared for 38 skills and 13 heritages. Authentic motion/BSP/cell collision and full world composition are separate unfinished systems.

<a id="validation"></a>
## 🧪 Build, tests, and compatibility oracles

Database integration tests require PostgreSQL `initdb` and `pg_ctl` on `PATH`, running as an ordinary user. They create disposable clusters rather than using an existing database.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask check
```

Additional suites need their stated prerequisites:

```sh
BACE_DAT_DIRECTORY=/path/to/DATS \
  cargo test -p bace-dat --test archive --test tables -- --ignored --nocapture

BACE_DAT_DIRECTORY=/path/to/DATS \
  cargo test -p bace-runtime --test character_assets -- --ignored

BACE_MARIADB_BASEDIR=/path/to/mariadb/usr \
  cargo test -p bace-import --test mariadb -- --ignored

cargo test -p bace-dat-service --test zlib_oracle -- --ignored
```

The zlib check needs Python 3 with its system zlib module. Optional Studio/MariaDB checks are documented in the [Studio README](crates/application/bace-content-studio/README.md). An ignored check is not a passing check. 🧾

### 🔬 Pinned ACE, independently compiled

Python 3 and .NET 10 regenerate synthetic fixtures from pinned official source:

```sh
python3 tools/bace-compat/oracle/generate.py --dotnet dotnet
python3 tools/bace-compat/oracle/message_generate.py --dotnet dotnet
python3 tools/bace-compat/oracle/transport_generate.py --dotnet dotnet
python3 tools/bace-compat/oracle/dat_generate.py --dotnet dotnet
python3 crates/gameplay/bace-character/oracle/generate.py --dotnet dotnet
python3 crates/gameplay/bace-quests/oracle/generate.py --dotnet dotnet
cargo test -p bace-compat
```

The generators overwrite their corresponding synthetic fixtures and verify official source bytes. With a local checkout of the exact ACE pin, source coverage can also be verified:

```sh
python3 tools/bace-compat/oracle/network_inventory.py --check \
  --source /path/to/pinned-ACE-checkout
```

Only tested features have compatibility evidence. See the [oracle documentation](tools/bace-compat/README.md), [pins](docs/baselines.toml), and [validation record](docs/validation.md). GDLE is a pinned secondary reference, not an override for official ACE.

> 🏅 Golden vectors beat “the packet looked right to the AI.” Every time.

<a id="status"></a>
## 📡 What actually works — status without the confetti cannon

| Area | Implemented foundation | Still needed |
|---|---|---|
| 🔌 Networking | Paired UDP, reliability, bounded sessions/authentication, and source-backed object/social/inventory/trade/vendor/movement codecs | Remaining payloads, complete session/gameplay composition, stock-client qualification |
| 🧠 Simulation | One owner, synthetic collision, character progression, bounded outcomes, retained state on recoverable shutdown | Authentic AC physics, full gameplay, durable character lifecycle |
| 📚 Character assets | XP, skills, CharGen, validated creation/training rules | Complete player construction, appearance/world admission, saves |
| 📦 Content/storage | TOML/binary tools, PostgreSQL foundations, immutable packs, publication/save workers | Complete world import and gameplay/runtime pack composition |
| 🎨 Authoring | Native editor, supported import/export, offline pack builder | Remaining authoring systems and platform validation |
| 🖥️ Hosting | Authenticated local supervisor/dashboard | Full game backend and cross-platform qualification |

A real UDP integration test carries a progression action through typed dispatch, authoritative simulation, and primary XP/trait replies. It uses a **synthetic world**; it is not a stock-client playability test. Rank-up secondary effects and durable gameplay composition remain work.

```sh
# This currently returns a readiness error; no game socket opens.
bace-server serve --config tests/fixtures/config/server.toml
```

For exact scope: [implementation status](docs/implementation-status.md), [network coverage](docs/network-coverage.toml), [gameplay parity](docs/gameplay-parity.toml), and [reported divergences](docs/divergences.toml).

<a id="roadmap"></a>
## 🧭 Architecture and roadmap

- 🏗️ [Architecture and ownership](ARCHITECTURE.md)
- 🤖 [Contributor/agent MUST rules](AGENTS.md)
- 📏 [Dependency inventory](architecture.toml)
- 💾 [Persistence](docs/persistence.md)
- 📦 [Runtime pack format](docs/pack-format.md)
- 🖥️ [Host lifecycle](docs/host-console.md)
- ⚙️ [Physics precision and SIMD gates](docs/physics-math.md)

Next milestones are authentic geometry/motion, complete character/world entry, persistence and replication composition, remaining gameplay/content systems, stock-client scenarios, and failure/performance qualification. Command simplification is also on the list. No invented completion percentage required. 📊

<a id="contributing"></a>
## 🤝 Contributing and license

Read the ownership rules before editing, keep crate roots thin, preserve upstream attribution, and run the relevant checks. Compatibility claims need source provenance and independent evidence. Never commit DAT assets, credentials, or player captures.

PR titles **and descriptions** should include emojis. The important part underneath the emojis is a concrete change, honest validation, and clear limitations. 🧪✨

BetterACE uses **AGPL-3.0-only**. All workspace crates inherit the root license; [LICENSE](LICENSE) retains the full text and source notices.

<a id="ai-disclaimer"></a>
## 🤖✨ AI README disclaimer

This README contains AI-assisted prose, enthusiastic emoji, suspiciously organized tables, and one planned command diet. Its jokes do not establish compatibility, performance, or platform support.

**The code handles the contracts. The tests handle the evidence. The README handles the ✨presentation✨.**

**The MMO is still responsible for eventually MMOing.** 🏰
