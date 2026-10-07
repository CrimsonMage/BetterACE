# Content fixture provenance

`upstream-arrow.sql` is unchanged official ACE-World-16PY-Patches content.

- Commit: `723f5dd74fcf42691593d2109720c2866482d3cc`
- Source: https://raw.githubusercontent.com/ACEmulator/ACE-World-16PY-Patches/723f5dd74fcf42691593d2109720c2866482d3cc/Database/Patches/9%20WeenieDefaults/Ammunition/MissileWeapon/00300%20Arrow.sql
- SHA256: `50afe1cb6941f73ee0aa424308799a6a67c0a0f1fc933089f7f75a7afa7f68c6`

The TOML and Lifestoned fixtures are synthetic schema-coverage inputs authored for these tests from official ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b`. They contain no player data or proprietary DAT assets.

`complex-weenie.sql` is synthetic, using pinned ACE world schema and formatter conventions. It deliberately includes quotes/semicolons, session variables, NULL fields and non-contiguous action/page orders.

Importer assets `world-base.sql` and classname/enum lookup data derive from pinned official ACE. The schema is unchanged on disk; the isolated staging instance applies the documented unsigned-motion compatibility adjustment before loading input.
