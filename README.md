# 🚀 BetterACE — BetterACEmulator

> 🧠⚡ **A modern, compatibility-first reimplementation of ACEmulator/ACE — engineered for correctness, determinism, and ✨vibes✨.**

A Rust workspace for reimplementing official ACEmulator/ACE with exact legacy packet contracts, authoritative physics, compact PostgreSQL persistence, and live TOML-authored content.

[![🦀 Rust](https://img.shields.io/badge/Rust-1.96.1-orange?logo=rust)](https://www.rust-lang.org/)
[![💾 PostgreSQL](https://img.shields.io/badge/PostgreSQL-supported-336791?logo=postgresql)](https://www.postgresql.org/)
[![📜 License](https://img.shields.io/badge/license-AGPL--3.0--only-blue)](LICENSE)
[![🤖 AI-approved-ish](https://img.shields.io/badge/AI--generated%20README%20energy-%E2%9C%A8%F0%9F%A4%96%E2%9C%A8-purple)](#)
[![✅ Build](https://img.shields.io/badge/build-passing%20(trust%20me)-brightgreen)](#-build--verify)
[![🚀 Blazingly Fast](https://img.shields.io/badge/blazingly-fast%E2%84%A2-red)](#)
[![🏰 MMO Status](https://img.shields.io/badge/MMO-not%20yet%20MMOing-lightgrey)](#-networking-status)
[![📈 Emoji Coverage](https://img.shields.io/badge/emoji%20coverage-137%25-ff69b4)](#-ai-readme-disclaimer)
[![☕ Made with](https://img.shields.io/badge/made%20with-%F0%9F%A6%80%20%2B%20%E2%98%95%20%2B%20invariants-yellow)](#)

> ⚠️ **STATUS: WORKING FOUNDATION, NOT YET A PLAYABLE REPLACEMENT SERVER™**
>
> The crate layout and MUST rules are implemented; complete stock-client login/world entry, AC collision and complete official-world SQL conversion remain outstanding.
> See [implementation status](docs/implementation-status.md).

---

## 📑 Table of Contents

- [🧭 Project Map](#-project-map)
- [⚡ Quick Start (TL;DR)](#-quick-start-tldr)
- [🦀🔨 Build & Verify](#-build--verify)
- [🖥️✨ Host Console](#️-host-console)
- [📦 Native Content](#-native-content)
- [🧙‍♂️ SQL → TOML → Native Content Alchemy](#️-sql--toml--native-content-alchemy)
- [🗺️ User-Supplied Assets](#️-user-supplied-assets)
- [🧪🔬 Compatibility Oracle](#-compatibility-oracle)
- [📡🌐 Networking Status](#-networking-status)
- [🆚 Why BetterACE?](#-why-betterace)
- [🗺️ Roadmap](#️-roadmap)
- [❓ FAQ](#-faq)
- [🤝 Contributing](#-contributing)
- [📜 License](#-license)
- [🤖✨ AI README Disclaimer](#-ai-readme-disclaimer)

> 🧠 A Table of Contents signals **enterprise readiness**. The links may or may not resolve. That is between you and GitHub's anchor slugger. 🎰

---

## 🧭 Project Map

If you're here to understand the machinery before touching the machinery:

- 🏗️ [Architecture and crate ownership](ARCHITECTURE.md)
- 🤖 [Mandatory contributor/agent rules](AGENTS.md)
- 🧪 [Machine-checked dependency inventory](architecture.toml)
- 🔐 [Pinned reference and data fingerprints](docs/baselines.toml)
- 💾 [Storage/publication contracts](docs/persistence.md)
- 📦 [Immutable `.bace` runtime pack format](docs/pack-format.md)
- 🖥️ [Host console and restart protocol](docs/host-console.md)
- ⚙️ [Physics precision and SIMD gates](docs/physics-math.md)
- 📊 [Validation results and limits](docs/validation.md)

> 💡 **Philosophy:** boring contracts, deterministic systems, explicit failure modes.  
> ✨ **Aesthetic:** Rust, TOML, PostgreSQL, packet archaeology, and an unreasonable number of invariants.

---

## ⚡ Quick Start (TL;DR)

Get up and running in **3 easy steps**! 🏃💨

```sh
git clone https://github.com/CrimsonMage/BetterACE.git
cd BetterACE
cargo run -p bace-server -- serve   # ❌ fails its readiness gate. This is correct. This is intended. ✅
```

> 🎉 **Congratulations!** You have successfully *not* started an MMO.
> This is the expected behavior and is covered by tests. 🧪
> For the steps that actually do something, proceed to [Build & Verify](#-build--verify). 👇

---

## 🦀🔨 Build & Verify

Rust **1.96.1** is pinned. Database integration tests require PostgreSQL `initdb` and `pg_ctl` on `PATH`, run as an ordinary user. They create isolated temporary clusters; **no existing database is used.** 🛡️

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask check
cargo run -p bace-server -- check --config tests/fixtures/config/server.toml
cargo run -p bace-server -- exercise --ticks 300 --players 100 --bodies 1000
```

### 🧪 Toolchain Reality Check™️

The supplied development environment's named `1.96.1` toolchain lacks Cargo/rustfmt/clippy executables despite reporting installed components. Its `stable` toolchain has the same compiler version and complete tools.

👉 Use `cargo +stable ...` in that environment, or repair the named installation.

> 🧠 **Important:** This does **not** change the repository's pin. The pin remains the pin. The compiler remains the compiler. The tools remain mysteriously elsewhere. 🔮

The exercise command runs synthetic geometry on one dedicated simulation thread without networking. Physics and world state share this owner. Network/content adapters and the reserved save worker stay separate.

🚫 `serve` fails its readiness gate until the playable milestone exists.

---

## 🖥️✨ Host Console

`bace-server` starts the local authenticated host dashboard by default.

Provision its separate host credentials with `bace-cli host-init` using piped stdin from a hidden password prompt. See [host-console setup and current limitations](docs/host-console.md).

> 🛑 The supervised foundation child reports game serving as unavailable.
>
> Translation: **the dashboard can exist before the MMO does.** 😎

---

## 📦 Native Content

Runtime content databases use `.bace` files.

Single-template binary exports below are interchange envelopes, distinct from the indexed runtime pack format.

```sh
cargo run -p bace-cli -- convert \
  --input tests/fixtures/content/minimal.toml --from toml \
  --output /tmp/weenie.bin --to binary

cargo run -p bace-cli -- convert \
  --input /tmp/weenie.bin --from binary \
  --output /tmp/weenie.toml --to toml
```

### 🗄️ PostgreSQL + Content Publication Pipeline™

Set `BACE_DATABASE_URL` in the environment for a dedicated development database.

> 🔐 **Security-shaped text:** Do not put credentials in tracked configuration or command-line arguments.

```sh
cargo run -p bace-cli -- migrate
cargo run -p bace-server -- content-worker --config tests/fixtures/config/server.toml

# In another terminal:
cargo run -p bace-cli -- publish --input tests/fixtures/content/minimal.toml --format toml
cargo run -p bace-cli -- content-status
```

### 🔄 Immutable Publication Flow

Publication queues immutable candidates.

The worker:

1. 🔍 validates binary data
2. 🧬 verifies scalar identity
3. ☣️ quarantines invalid batches
4. 📚 publishes accepted catalog generations
5. ⏱️ crosses the bounded tick-boundary channel
6. 🚫 does **not** spawn a world object
7. 🚫 does **not** run a game server

Direct native SQL insertions use the same database trigger/journal path.

> ✨ **Immutable candidates. Bounded channels. Explicit ownership.**
>
> Because apparently the correct answer to legacy MMO infrastructure is **more invariants**.

---

## 🧙‍♂️ SQL → TOML → Native Content Alchemy

SQL/JSON conversion boundaries and supported legacy dialects are documented in [bace-import](crates/content/bace-import/README.md).

Unsupported conversion fails explicitly.

🚫 Arbitrary MySQL SQL is **never** sent to PostgreSQL.

For isolated conversion of a supported weenie SQL batch, point the tool at a private MariaDB installation (**only conversion needs it**):

```sh
BACE_MARIADB_BASEDIR=/path/to/mariadb/usr cargo run -p bace-cli -- import-sql \
  --input /path/to/weenies.sql --output-directory /tmp/converted-weenies
```

This emits one native TOML file per weenie plus a source/count manifest.

> ⚠️ Complete official-world dumps contain other systems and are explicitly rejected until those converters are implemented.
>
> **No magical “just import everything” button.** 🪄❌

---

## 🗺️ User-Supplied Assets

```sh
cargo run -p bace-cli -- dat-inspect /path/to/client_cell_1.dat
BACE_DAT_DIRECTORY=/path/to/DATS cargo test -p bace-dat --test archive -- --ignored --nocapture
```

DAT header, BTree, sector chains and outdoor landblock records are implemented.

> 🧱 These readers are **not** a full AC collision engine.
>
> 📁 DAT files are excluded from Git.
>
> 🧠 The system knows this. The README knows this. The CI knows this. Everyone is aligned. 🤝

---

## 🧪🔬 Compatibility Oracle

The [official C# oracle](tools/bace-compat/README.md) generates synthetic golden wire vectors from pinned unmodified sources.

Only enumerated tested features have compatibility evidence.

> 🎯 **Compatibility is demonstrated, not manifested through vibes.**
>
> Golden vectors > “trust me bro.” 🫡

---

## 📡🌐 Networking Status

Networking now has:

- 🔌 tested paired-UDP/session foundations
- 🔐 bounded authentication
- 📦 DAT preparation workers
- 🧾 opcode catalogs
- 🧬 selected source-backed codecs

🚧 **Full stock-client operation remains unavailable.**

See:

- 📋 [implementation status](docs/implementation-status.md)
- 📡 [network coverage](docs/network-coverage.toml)
- 🎮 [gameplay parity](docs/gameplay-parity.toml)
- 🔎 [reported divergences](docs/divergences.toml)

for exact scope and remaining work.

> 🚀 **The packets are becoming packetier.**
>
> 🧪 **The compatibility is becoming compatibilityier.**
>
> 🏰 **The MMO is not yet MMOing.**
>
> Progress! ✨

---

## 🆚 Why BetterACE?

| Feature | ACEmulator/ACE | BetterACE |
|---|:---:|:---:|
| 🦀 Written in Rust | ❌ | ✅ |
| 🐘 PostgreSQL persistence | ❌ | ✅ |
| 📜 Live TOML-authored content | ❌ | ✅ |
| 🧾 Exact legacy packet contracts | ✅ | ✅ (with **golden vectors** 🏅) |
| 🧠 Unreasonable number of invariants | some | ✅✅✅ |
| 🎮 Playable MMO | ✅ | 🚧 Soon™ |
| ✨ Emoji in README | ❌ | ✅ 137% |
| 🤖 README written by a robot | ❌ | ✅ |

> 📊 Table accuracy: **vibes-verified.** 🫡

---

## 🗺️ Roadmap

- [x] 🏗️ Crate layout
- [x] 📏 MUST rules
- [x] 🔌 Paired-UDP/session foundations
- [x] 🔐 Bounded authentication workers
- [x] 📦 `.bace` immutable runtime packs
- [x] 🖥️ Host dashboard that exists before the game does
- [ ] 🔑 Complete stock-client login/world entry
- [ ] 🧱 AC collision
- [ ] 🗄️ Complete official-world SQL conversion
- [ ] 🏰 MMO actually MMOing
- [x] ✨ README emoji saturation **(this PR)** 🎯

> 🚀 **5 of 11 complete.** We're basically halfway to Dereth. 🧭

---

## ❓ FAQ

**Q: Is it playable?** 🎮
A: No. 🏰 See [implementation status](docs/implementation-status.md).

**Q: Can I log in?** 🔐
A: The authentication worker is *bounded*. Your expectations should be too. 🙏

**Q: Is it blazingly fast?** 🚀
A: The exercise command runs 300 ticks, 100 players and 1000 bodies on one dedicated simulation thread. Blazingly. 🔥

**Q: Why Rust?** 🦀
A: Because the borrow checker is the only thing that has ever successfully enforced a MUST rule. 📏

**Q: Can I just import the whole official world dump?** 🗄️
A: No. ❌ It is *explicitly rejected*. The rejection is *documented*. The documentation is *immutable*. 📚

**Q: Did an AI write this README?** 🤖
A: ✨ Yes. ✨ The code, however, was written with ✨ *care* ✨ and *also* ✨ an AI. 🧠

---

## 🤝 Contributing

We ❤️ contributions!

1. 📖 Read the [mandatory contributor/agent rules](AGENTS.md). The agents already did. 🤖
2. 🧪 Run the full [Build & Verify](#-build--verify) gauntlet. All of it. `-D warnings` means **-D warnings**. 🛡️
3. 📝 Open a PR with a title that contains at least one (1) emoji. This is now a **MUST rule**. ✨
4. 🫡 Await review from a human, an agent, or an oracle. Whichever is available.

> 🏆 **Contributors of the Month:** the borrow checker, `cargo clippy`, and whoever wrote the readiness gate.

---

## 🧠 Architecture in One Extremely AI-Coded Sentence

**Deterministic simulation ownership + explicit compatibility contracts + immutable content publication + bounded workers + PostgreSQL persistence + aggressively documented failure modes = BetterACE.** 🤖⚙️📦

---

## ⭐ Star History

```
  ⭐
  │                                                        ✨ you, right now?
  │                                                       ╱
  │                                                      ╱
  │                                            ─────────╯
  │                              ─────────────╯
  │                  ───────────╯
  │        ─────────╯
  └──────────────────────────────────────────────────────────── 🕒
        "Add files via upload"              "Make README 37% More AI"
```

> 📈 If this README made you feel something, consider starring the repo. ⭐
> Stars do **not** affect the readiness gate. We checked. 🧪

---

## 📜 License

BetterACE is licensed under the **GNU Affero General Public License, version 3 only** (`AGPL-3.0-only`).

See [LICENSE](LICENSE) for the full terms.

All workspace crates inherit this license from the root `Cargo.toml`.

Upstream copyright notices, source attribution and applicable third-party license terms are preserved; see crate provenance notes.

---

## 🤖✨ AI README Disclaimer

This README contains:

- 🤖 AI-generated enthusiasm
- ✨ unnecessary emoji
- 🧠 suspiciously confident section titles
- 📈 approximately 37% more “platform” energy
- 📊 one (1) comparison table of questionable rigor
- 🗺️ a roadmap where the only completed item is this README
- ⭐ an ASCII star-history chart with n=1 data points
- 🚀 zero additional gameplay functionality

**The code remains responsible for being correct.**

**The README remains responsible for looking extremely important.**

**The MMO remains responsible for eventually MMOing.** 🏰

---
