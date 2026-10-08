//! Source ephemeral query/appraisal state; authority remains with simulation.
use crate::ActionContext;
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargetSelection {
    pub health: Option<EntityId>,
    pub mana: Option<EntityId>,
    pub requested_appraisal: Option<EntityId>,
    pub current_appraisal: Option<EntityId>,
    pub appraisal_requested_at: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetQueryKind {
    Health,
    ItemMana,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedItemManaQuery {
    pub revision: u64,
    pub current: Option<i32>,
    pub maximum: Option<i32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetQueryResponse {
    Health {
        target: EntityId,
        fraction: f32,
    },
    ItemMana {
        target: EntityId,
        fraction: f32,
        success: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetQueryEvent {
    pub context: ActionContext,
    pub response: Option<TargetQueryResponse>,
}
