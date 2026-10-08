# bace-housing

Pure housing lifecycle and access proposals, with explicit time and authoritative
actor/payment projections. Purchase checks level, monarch/rank, account age,
cooldown and existing ownership. Rent accepts partial item payments and explicit
currency change; due processing uses purchase-aligned periods, maintenance flags
and eligibility. Apartment interval selection is explicit in `rent_window`.

Ownership, rent, permissions, deed/player state and payment item effects must be
reserved and committed atomically before client success. `HousingProposal` names
its reason and preserves before/after revisions and access generations. It never
mutates a database or a physics actor. `RentScheduler` retains an overdue ticket
through failure; a changed deadline requires committed replacement.

Approved disposition policy: abandonment and eviction retain house contents.
Unowned houses deny access; the next committed owner inherits the existing
contents. Removing owner/permissions advances the access generation so old open
storage views cannot succeed. Bulk storage permission operates on registered
guests, never grants access to arbitrary strangers. Root/linked house metadata,
account and allegiance membership must come from authoritative services.

Primary sources at official ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`:
`Player_House.cs`, `House.cs`, `Managers/HouseManager.cs`, `Storage.cs`, `Hook.cs`.
The retention policy is explicit BetterACE behavior; it is not inferred from ACE's
message claiming stored items were lost. `oracle/generate.py` compiles unchanged
GetRentTimestamp/GetRentDue methods with an explicit synthetic clock; 42 vectors
cover month/apartment boundaries. Domain tests cover partial payments, retention,
revocation, offline due arithmetic and deadline retention. Complete housing packet,
barrier, hook and stock-client qualification requires integrated owner tests.
