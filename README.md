# BetterACE — BetterACEmulator

A Rust workspace for reimplementing official ACEmulator/ACE with exact legacy packet contracts, authoritative physics, compact PostgreSQL persistence, and live TOML-authored content.

**This is a working foundation, not a playable replacement server.** The crate layout and MUST rules are implemented; complete stock-client login/world entry, AC collision and complete official-world SQL conversion remain outstanding. See [implementation status](docs/implementation-status.md).

- [Architecture and crate ownership](ARCHITECTURE.md)
- [Mandatory contributor/agent rules](AGENTS.md)
- [Machine-checked dependency inventory](architecture.toml)
- [Pinned reference and data fingerprints](docs/baselines.toml)
- [Storage/publication contracts](docs/persistence.md)
- [Immutable `.bace` runtime pack format](docs/pack-format.md)
- [Host console and restart protocol](docs/host-console.md)
- [Physics precision and SIMD gates](docs/physics-math.md)
- [Validation results and limits](docs/validation.md)

## Build and verify

Rust 1.96.1 is pinned. Database integration tests require PostgreSQL `initdb` and `pg_ctl` on PATH, run as an ordinary user. They create isolated temporary clusters; no existing database is used.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask check
cargo run -p bace-server -- check --config tests/fixtures/config/server.toml
cargo run -p bace-server -- exercise --ticks 300 --players 100 --bodies 1000
```

The supplied development environment's named `1.96.1` toolchain lacks Cargo/rustfmt/clippy executables despite reporting installed components. Its `stable` toolchain has the same compiler version and complete tools. Use `cargo +stable ...` in that environment, or repair the named installation. This does not change the repository's pin.

The exercise command runs synthetic geometry on one dedicated simulation thread without networking. Physics and world state share this owner. Network/content adapters and the reserved save worker stay separate. `serve` fails its readiness gate until the playable milestone exists.

## Host console

`bace-server` starts the local authenticated host dashboard by default. Provision its separate host credentials with `bace-cli host-init` using piped stdin from a hidden password prompt. See [host-console setup and current limitations](docs/host-console.md). The supervised foundation child reports game serving as unavailable.

## Native content

Runtime content databases use `.bace` files. Single-template binary exports below are interchange envelopes, distinct from the indexed runtime pack format.

```sh
cargo run -p bace-cli -- convert \
  --input tests/fixtures/content/minimal.toml --from toml \
  --output /tmp/weenie.bin --to binary
cargo run -p bace-cli -- convert \
  --input /tmp/weenie.bin --from binary \
  --output /tmp/weenie.toml --to toml
```

Set `BACE_DATABASE_URL` in the environment for a dedicated development database. Do not put credentials in tracked configuration or command-line arguments.

```sh
cargo run -p bace-cli -- migrate
cargo run -p bace-server -- content-worker --config tests/fixtures/config/server.toml
# In another terminal:
cargo run -p bace-cli -- publish --input tests/fixtures/content/minimal.toml --format toml
cargo run -p bace-cli -- content-status
```

Publication queues immutable candidates. The worker validates binary data, verifies scalar identity, quarantines invalid batches and publishes accepted catalog generations through a bounded tick-boundary channel. It does not spawn a world object or run a game server. Direct native SQL insertions use the same database trigger/journal path.

SQL/JSON conversion boundaries and supported legacy dialects are documented in [bace-import](crates/content/bace-import/README.md). Unsupported conversion fails explicitly; arbitrary MySQL SQL is never sent to PostgreSQL.

For isolated conversion of a supported weenie SQL batch, point the tool at a private MariaDB installation (only conversion needs it):

```sh
BACE_MARIADB_BASEDIR=/path/to/mariadb/usr cargo run -p bace-cli -- import-sql \
  --input /path/to/weenies.sql --output-directory /tmp/converted-weenies
```

This emits one native TOML file per weenie plus a source/count manifest. Complete official-world dumps contain other systems and are explicitly rejected until those converters are implemented.

## User-supplied assets

```sh
cargo run -p bace-cli -- dat-inspect /path/to/client_cell_1.dat
BACE_DAT_DIRECTORY=/path/to/DATS cargo test -p bace-dat --test archive -- --ignored --nocapture
```

DAT header, BTree, sector chains and outdoor landblock records are implemented. These readers are not a full AC collision engine. DAT files are excluded from Git.

The [official C# oracle](tools/bace-compat/README.md) generates synthetic golden wire vectors from pinned unmodified sources. Only enumerated tested features have compatibility evidence.

## License

BetterACE is licensed under the **GNU Affero General Public License, version 3 only** (`AGPL-3.0-only`). See [LICENSE](LICENSE) for the full terms. All workspace crates inherit this license from the root `Cargo.toml`.

Upstream copyright notices, source attribution and applicable third-party license terms are preserved; see crate provenance notes.


Networking now has tested paired-UDP/session foundations, bounded authentication
and DAT preparation workers, opcode catalogs and selected source-backed codecs.
Full stock-client operation remains unavailable. See
[implementation status](docs/implementation-status.md),
[network coverage](docs/network-coverage.toml),
[gameplay parity](docs/gameplay-parity.toml), and
[reported divergences](docs/divergences.toml) for exact scope and remaining work.
