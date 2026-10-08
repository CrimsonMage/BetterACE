# Official ACE world policies oracle

Pin: `47edade3bd3f6044b676d4eb877c4965c7eda62b`, AGPL-3.0-only, ACEmulator contributors. `extract.py` copies unchanged source methods and set initializers; `source.sha256` identifies each input. `Program.cs` supplies only deterministic dependencies and fixture inputs. Run the extractor and `dotnet run --project Oracle.csproj -c Release` with .NET 8, directing output to `../../tests/fixtures/ace_world_policy.tsv`. The first L/D/X rows are also copied into the config crate's `tests/fixtures/ace_world_zones.tsv`.

The differential covers all 275 supported levels, four configured vitae floors, all death item-loss augmentation levels, PK/PvE cases, 128 status bit patterns and player-GUID boundaries, plus exact f32 recall distance boundaries. Synthetic positions do not qualify DAT collision or client animation.
