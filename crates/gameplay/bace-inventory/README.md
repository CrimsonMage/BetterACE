# bace-inventory

Authoritative inventory proposals over immutable item/container views. No packet,
SQL, wall-clock or physical-state owner lives in this crate.

`propose_inventory` covers pickup/container movement, drop, equipment replacement,
stack split to container/world/equipment and stack merge. It checks explicit
world-owner range/geometry/access verdicts, view generations, quantities, attunement,
trade reservations, active pets, quest/unique restrictions, wield eligibility,
capacities, burden and revision overflow. It returns changed item snapshots and
reserved ancestor identities; it does not adopt changes before durable success.
Conflicting equipped items return to the pack atomically. Main and backpack
placement lanes shift independently using pinned `Container.cs` ordering.

A container's `root_owner` denotes carried/equipped character inventory only.
Housing storage, hooks, chests and corpses use None, even if privately owned.
Their access/open/view generation comes from their authoritative subsystem. This
keeps stored contents out of player burden and prevents dropping attuned items
into housing. Attuned world rewards may still be picked up, as pinned ACE permits.

Server-only `propose_grant`, `propose_take`, `propose_take_items` and
`propose_take_template` support rewards/payment/hand-ins. Native emote TakeItems
uses inventory first and only falls back to equipped items if inventory has none;
it caps the requested amount at available units, matching ACE. Exact-ID payments
and spell component requirements instead fail atomically on any shortage.

`propose_vendor_purchase` joins an exact server-selected currency-stack debit
and fully prepared fresh leaf grants in one inventory proposal. It validates
the quoted cost, ownership, duplicate identities, available pack slots and
post-payment burden before returning participant revisions.
`select_vendor_currency_debits` traverses accepted direct stacks before side
containers in pinned ACE placement order, returning exact IDs and quantities.
The simulation owner can reserve this proposal with the Shop marker; the
runtime must still freeze every item and the player's currency property in one
durable operation and publish success only after the exact receipt. Live
vendor Buy remains unsupported.

`propose_component_use` receives ACE-mapped WCIDs, required quantities and the
magic owner's already sampled burn subset. It reserves all requirements while
mutating only burned stacks. Empty burns still validate the reservation and do
not imply a database write. Formula ID mapping and random draws do not occur here.

Reference: official ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b`,
`WorldObjects/Player_Inventory.cs`, `Container.cs`, and `Managers/EmoteManager.cs`.
`oracle/generate.py` verifies pinned source bytes and compiles unchanged placement
shift statements. Seven independent vectors cover main/backpack insertion/removal;
proposal tests cover atomic failure, quantity conservation, equipment swaps,
attunement and required-versus-burned components. These are not a complete
stock-client inventory-handler or physical-drop compatibility claim. Physical
placement, reach revalidation after animation, typed wire projection and mutation
adoption remain responsibilities of their existing owners.

`propose_stack_split` requires a separate fresh template instance, matching ACE
`Player_Inventory` (2243–2838 at the baseline pin). The source retains instance
mutations and enchantments; the new stack starts with factory state. Checked
capacity, whole-source acquisition burden, vendor/stuck/corpse restrictions and
attuned wield rules run before reservations. Counts of zero or the entire source
are rejected. The generic inventory entry point cannot substitute a source clone.

World splits additionally require prepared collision geometry in the simulation
owner. Exact receipts adopt inventory and the physical body together; a blocked
post-commit placement retains the pending operation for retry. Definite rejection
releases preparation with both accepted stacks unchanged. No placeholder or
unknown stack state is published. The unchanged C# stack-value methods generate
`tests/fixtures/split*`; runtime factory and simulation world-split tests cover
fresh properties, conservation and receipt failures. This evidence does not by
itself qualify stock-client motion or packet ordering.

Physical use distance and pickup height selection use the pinned ACE
`Physics/Common/Position.CylinderDistance` and
`WorldObjects/Player_Inventory.GetPickupMotion` methods. The independent compiled
C# oracle in `oracle/physical/generate.py` produces 140 fixtures, including
cylinder overlap, vertical separation, corpse height, and all pickup thresholds.
Simulation supplies accepted geometry and rechecks after the actual prepared
animation callback. Non-finite dimensions fail closed.

`check_wield_requirements` implements the same source file's four ordered
requirements, heritage and AllowedWielder gates, and the source configuration
bypass. `oracle/wield/generate.py` compiles both unchanged source methods and
produces 1,272 vectors covering every requirement kind, each slot, signed limits,
heritages, locations, ownership and bypass. Missing authoritative attributes or
vitals return an explicit error instead of ACE's null-reference failure. Runtime
login preparation supplies source-correct retired skill mapping and raw vital
formula values. This policy does not authorize a slot-only live equipment change:
combat, enchantment, vital, attachment and item-experience owners must compose the
full durable transition before live equipment ingress is enabled.

Equipment activation requirements preserve pinned
`WorldObject_Use.CheckUseRequirements` evaluation order, signed limits, training,
heritage/subtype restrictions and cooldown rejection. The compiled unchanged ACE
method produces 404 oracle vectors (`oracle/activation/generate.py`); typed errors
retain source message families. Missing authoritative qualities fail explicitly.
The simulation evaluates these predicates from its accepted character and registry
owners; a cold adapter does not authorize activation by supplying a boolean.

Live equipment checks use the complete revision-fenced gear roster and accepted
combat/Aetheria values. Source slot policy rejects conflicting equipment; its
proposal entry point does not implicitly dequip another item. Pinned ACE
`Player_Inventory.CheckWeaponCollision` and `Creature_Equipment` coverage/parent
occupancy methods are exercised by 2,112 compiled-original vectors in
`oracle/wield_slots`. Clothing normalization and Aetheria regression cases are
source-reviewed separately. The older generic proposal API retains its existing
replacement semantics; the live equipment owner uses the explicit checked path.
