# ACE equipment activation oracle

`generate.py` compiles the unchanged `CheckUseRequirements` method from official
ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`,
`Source/ACE.Server/WorldObjects/WorldObject_Use.cs`. Surrounding getters and network
message objects are controlled stubs; enum sentence stubs emit numeric identity
so fixture comparisons isolate the method's decisions and ordering. The original
source SHA-256 is recorded in the fixture. Regenerate using .NET 8 (`DOTNET` may
select the executable). No live gameplay or client qualification is implied.
