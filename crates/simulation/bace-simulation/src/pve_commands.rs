//! Trusted durable completions for the single-owner corpse/reward workflow.
use crate::{AllegianceTicket, PveError};
use bace_types::EntityId;
#[derive(Debug)]
pub enum PveServiceAction {
    Stage {
        operation: u64,
        forest: Box<crate::PreparedWorldRegionItems>,
    },
    Retry {
        operation: u64,
    },
    Committed {
        operation: u64,
        corpse: Option<EntityId>,
        items: Vec<EntityId>,
        credits: Vec<(EntityId, bace_character::ExperienceCredit)>,
        social: Option<Box<AllegianceTicket>>,
    },
}
#[derive(Debug)]
pub struct PveServiceCommand {
    pub correlation: u64,
    pub action: PveServiceAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PveServiceOutcome {
    pub correlation: u64,
    pub result: Result<(), PveError>,
}
impl PveServiceCommand {
    pub fn valid_bounds(&self) -> bool {
        if self.correlation == 0 {
            return false;
        }
        match &self.action {
            PveServiceAction::Stage { operation, forest } => {
                *operation != 0
                    && forest.items.len() <= 257
                    && forest.containers.len() <= 257
                    && forest.roots.len() <= 257
                    && forest.registries.len() <= 257
                    && forest.constructed.is_empty()
            }
            PveServiceAction::Retry { operation } => *operation != 0,
            PveServiceAction::Committed {
                operation,
                items,
                credits,
                social,
                ..
            } => {
                *operation != 0
                    && items.len() <= 1024
                    && credits.len() <= 1024
                    && social.as_ref().is_none_or(|s| {
                        s.player_changes.len() <= 1024
                            && s.credits.len() <= 1024
                            && s.patch.nodes.len() + s.patch.metadata.len() <= 1024
                    })
            }
        }
    }
}
