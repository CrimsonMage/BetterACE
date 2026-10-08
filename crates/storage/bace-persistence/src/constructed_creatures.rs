use crate::PlacementOperation;

/// Fresh durable acquisition of constructed Creature/Cow trees. The exact
/// source construction companions are in the ItemSaveV5 root snapshots.
#[derive(Clone, Debug)]
pub struct ConstructedCreaturePromotionOperation {
    pub world_epoch: u64,
    pub inventory: PlacementOperation,
    /// Every fresh constructed root in the placement request, in source order.
    pub creature_roots: Vec<u32>,
}
