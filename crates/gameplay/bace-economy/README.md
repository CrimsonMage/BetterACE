# bace-economy

Vendors, currencies and player trading.

Status: foundation; complete subsystem behavior is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

Vendor buy/sell pricing now preserves pinned ACE's float intermediate, +/−0.1 rounding adjustment, minimum price, and promissory-note multipliers. Invalid negative/nonfinite/overflowing inputs fail explicitly. `tests/fixtures/prices.csv` contains 160 independent C# cases generated from verbatim `Vendor.GetBuyCost/GetSellCost`; generator lives in `../bace-combat/oracle/generate.py`.

Source: official ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `Source/ACE.Server/WorldObjects/Vendor.cs`, AGPL-3.0-only, ACE contributors. Listings, negotiated transactions, currency and durable purchase/sale composition remain unsupported.

`VendorStock` implements pinned ACE `Vendor.AddDefaultItem` on the single owner.
It preserves insertion order, merges into the first matching WCID with room, and
increments that retained stack by exactly one regardless of the incoming stack.
Proposals and commits are revision fenced. Receipts identify the actual retained
stock member and the unused incoming identity, so generator links cannot retain a
phantom child. `withdraw_default` removes an exact generator contribution, keeping
a merged stock identity alive while other contributions remain. Immutable source
item properties and identity admission remain with the simulation owner; this
stock owner does not make persistence or packet-send claims.

Provenance: official ACEmulator/ACE
`47edade3bd3f6044b676d4eb877c4965c7eda62b`,
`Source/ACE.Server/WorldObjects/Vendor.cs`, `AddDefaultItem` and
`GetDefaultItemsByWcid`, AGPL-3.0-only, ACE contributors.
`tests/fixtures/vendor_stock.csv` executes those original methods via
`../bace-loot/oracle/treasure_generate.py`; tests also cover stale receipts,
capacity, retained identity, and contribution withdrawal.

`quote_default_buy` is a bounded prerequisite for commerce. It accepts only
server-owned default offers and a stock revision, validates every signed client
amount before GUID lookup, retains duplicate source-order requests, skips
unknown GUIDs, and partitions fresh objects by the pinned
`Vendor.ItemProfileToWorldObjects` maximum-stack rule. Stack value uses the
pinned `WorldObject.SetStackSize` unit-value multiplication before existing
vendor pricing. Instance and currency totals use checked bounds. Vendor
services and unique items are explicitly unsupported in this quote. The quote
does not reserve currency or inventory and cannot report Buy success; the
single take-plus-grant inventory proposal, player currency snapshot and later
marker receipt are still required. `tests/vendor_buy_quote.rs` covers source
selection and bounded failure cases; the separate 160 original C# price vectors
remain the independent numeric evidence.
