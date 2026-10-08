# Source slot checks

`generate.py` compiles unmodified `Player_Inventory.CheckWeaponCollision`,
`Creature_Equipment.GetEquippedItems`, `GetEquippedClothingArmor`,
`GetPlacementLocation`, and `IsWeaponSlot` from official ACE
`47edade3bd3f6044b676d4eb877c4965c7eda62b`. Surrounding owner getters/logging
and item construction are stubs; enum definitions are copied from that pin.
The 2,112 checked rows cover both hand slots, ranged/ammo combinations,
nullable combat styles, combat modes, clothing coverage, and occupancy.

The additional Rust regression covers DoHandleActionGetAndWieldItem's source
normalization, Aetheria locks, explicit conflict rejection, and hand movement.
Those checks are source-reviewed regressions, not separate original-method
oracle coverage. Fixtures contain no proprietary assets.
