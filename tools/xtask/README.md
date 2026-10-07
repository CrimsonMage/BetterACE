# xtask

Status: implemented.

Checks crate ownership/dependencies, thin roots, macro/path bypasses, and 1000/1500 physical-line rules. Semantic gameplay invariants still require subsystem tests and review.

PACK-03 additionally enforces workspace unsafe forbid, lint inheritance for every package except the explicit codec deny exception, preservation of all other workspace lints, and the exact reviewed read-only mapping boundary. Unsafe code or lowered unsafe lints elsewhere, including inactive branches and macro bodies, are rejected. The mapping exception requires its SAFETY contract; expanding it requires an explicit policy/code review.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


The checker also validates `docs/network-coverage.toml`: official pin, source
fingerprints, unique entries, inventoried owners, explicit statuses and existing
evidence paths. CI additionally verifies the inventory against the complete pinned
Git tree using the official-source generator. An unsupported entry is visible
remaining work, not a test skip or a passing compatibility claim.
