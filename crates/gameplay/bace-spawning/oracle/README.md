# Official ACE generator oracle

`Extracted.cs`, `ExtractedPlacement.cs` and `ExtractedStatus.cs` contain unchanged method bodies from
ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`. Source filenames and
SHA-256 hashes are embedded in each extraction. Upstream copyright and
AGPL-3.0-only licensing apply. The harness stubs world insertion and supplies a
controlled random variate and profile availability; it does not implement
BetterACE's selection or transform formulas.

Regenerate with Python 3 and .NET SDK 8.0.408:

```
python3 crates/gameplay/bace-spawning/oracle/extract.py
dotnet run --project crates/gameplay/bace-spawning/oracle/Oracle.csproj --configuration Release > crates/gameplay/bace-spawning/tests/ace_selection.tsv
dotnet run --project crates/gameplay/bace-spawning/oracle/Oracle.csproj --configuration Release -- placement > crates/gameplay/bace-spawning/tests/ace_placement.tsv
dotnet run --project crates/gameplay/bace-spawning/oracle/Oracle.csproj --configuration Release -- status > crates/gameplay/bace-spawning/tests/ace_status.tsv
```

Rust fixture tests compare cumulative f32 probabilities, selected queue counts,
unlimited/treasure clamping, and requested positions against these independently
compiled C# results. The placement oracle does not certify geometry admission;
that requires physics integration tests. Thirty-five compiled status vectors exercise staged day/night/realtime/event
changes, reversed conditions and missing events. Explicit tick scheduling and
lifecycle hardening also use subsystem regressions; no .NET PRNG sequence parity is claimed.
