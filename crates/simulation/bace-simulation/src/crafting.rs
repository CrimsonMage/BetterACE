//! Pending crafting proposals only; accepted ownership remains in Inventory.
use crate::inventory::{Inventory, InventoryReceipt};
use bace_crafting::{
    Confirmation, CraftContext, CraftError, CraftItem, CraftProposal, PreparedRecipe,
    SalvageProposal, TinkerChance,
};
use bace_inventory::{InventoryItem, InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub enum CraftingDecision {
    Tinker(Box<CraftProposal>),
    Salvage(Box<SalvageProposal>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftingTicket {
    pub operation: u64,
    pub actor: EntityId,
    pub decision: CraftingDecision,
    pub inventory: InventoryProposal,
    /// Registries frozen on the simulation owner until definite completion.
    pub registry_reservations: Vec<EntityId>,
    pub proficiency: Option<CraftingProficiency>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftingProficiency {
    pub change: bace_character::ProficiencyChange,
    pub vitals: Vec<bace_entity::VitalMutation>,
    pub vitae: Option<crate::PlayerVitaeRecovery>,
    pub skill_base: Option<u32>,
    pub skill_maximum: bool,
    pub experience: Option<bace_gameplay_api::experience::ExperienceEvent>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SalvageAdmission {
    Empty(SalvageProposal),
    Pending(u64),
}
pub(crate) struct QuoteInput<'a> {
    pub context: &'a CraftContext,
    pub source: &'a CraftItem,
    pub target: &'a CraftItem,
    pub recipe: &'a PreparedRecipe,
    pub now: u64,
    pub lifetime: u64,
}
pub(crate) struct Crafting {
    pub(crate) animations: BTreeMap<EntityId, CraftingAnimation>,
    pub(crate) animation_scratch: Vec<EntityId>,
    root: Option<Arc<bace_random::RandomRoot>>,
    quotes: BTreeMap<EntityId, Confirmation>,
    pending: BTreeMap<u64, CraftingTicket>,
    submitted: BTreeSet<u64>,
    outbox: VecDeque<u64>,
    capacity: usize,
}
impl Crafting {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            animations: BTreeMap::new(),
            animation_scratch: Vec::new(),
            root: None,
            quotes: BTreeMap::new(),
            pending: BTreeMap::new(),
            submitted: BTreeSet::new(),
            outbox: VecDeque::new(),
            capacity: capacity.min(64),
        }
    }
    pub(crate) fn configure(
        &mut self,
        root: Arc<bace_random::RandomRoot>,
    ) -> Result<(), CraftError> {
        if self.has_state() {
            return Err(CraftError::Busy);
        }
        self.root = Some(root);
        Ok(())
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.quotes.is_empty() || !self.pending.is_empty() || !self.animations.is_empty()
    }
    pub(crate) fn expire_quotes(&mut self, now: u64) {
        self.quotes.retain(|_, q| q.expires_at() > now);
    }
    pub(crate) fn cancel_quote(&mut self, actor: EntityId) {
        self.quotes.remove(&actor);
    }
    pub(crate) fn quote(
        &mut self,
        input: QuoteInput<'_>,
        inventory: &Inventory,
    ) -> Result<TinkerChance, CraftError> {
        let QuoteInput {
            context,
            source,
            target,
            recipe,
            now,
            lifetime,
        } = input;
        if self.root.is_none() {
            return Err(CraftError::Random);
        }
        self.quotes.retain(|_, q| q.expires_at() > now);
        if self.quotes.len() >= self.capacity && !self.quotes.contains_key(&EntityId(context.actor))
        {
            return Err(CraftError::Capacity);
        }
        check_item(inventory, source, context.actor)?;
        check_item(inventory, target, context.actor)?;
        let quote = bace_crafting::quote_craft(context, source, target, recipe, now, lifetime)?;
        let chance = quote.chance();
        self.quotes.insert(EntityId(context.actor), quote);
        Ok(chance)
    }
    pub(crate) fn confirm(
        &mut self,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
        now: u64,
        inventory: &mut Inventory,
    ) -> Result<u64, CraftError> {
        if self.pending.len() >= self.capacity || !inventory.can_accept() {
            return Err(CraftError::Capacity);
        }
        check_item(inventory, source, context.actor)?;
        check_item(inventory, target, context.actor)?;
        let quote = self
            .quotes
            .get(&EntityId(context.actor))
            .ok_or(CraftError::StaleConfirmation)?;
        let decision = bace_crafting::propose_confirmed_craft(
            context,
            source,
            target,
            recipe,
            quote,
            now,
            self.root.as_deref().ok_or(CraftError::Random)?,
        )?;
        let mut changes = Vec::new();
        for (before, after) in [
            (&decision.source_before, decision.source_after.as_ref()),
            (&decision.target_before, decision.target_after.as_ref()),
        ] {
            let original = inventory
                .item(EntityId(before.id))
                .ok_or(CraftError::InvalidState)?
                .clone();
            let mut next = original.clone();
            if let Some(after) = after {
                next.stack = after.stack;
                next.revision = after.revision;
                for (id, output) in [(5, &mut next.unit_burden), (19, &mut next.unit_value)] {
                    if let Some(bace_crafting::PropertyValue::Int(value)) =
                        after.properties.get(&bace_crafting::PropertyKey {
                            kind: bace_crafting::PropertyKind::Int,
                            id,
                        })
                    {
                        let total = u32::try_from(*value).map_err(|_| CraftError::Overflow)?;
                        if total % after.stack != 0 {
                            return Err(CraftError::Unsupported);
                        }
                        *output = total / after.stack;
                    }
                }
            } else {
                next.stack = 0;
                next.place = ItemPlace::Removed;
                next.revision = next.revision.checked_add(1).ok_or(CraftError::Overflow)?;
            }
            // ACE RecipeManager.UpdateObj sends source then target and moves each
            // surviving modified item to the front of its current container.
            let branch = if decision.success {
                &recipe.success
            } else {
                &recipe.failure
            };
            let participant = if before.id == source.id {
                bace_crafting::Participant::Source
            } else {
                bace_crafting::Participant::Target
            };
            if after.is_some()
                && (branch
                    .mutations
                    .iter()
                    .any(|m| m.participant == participant)
                    || participant == bace_crafting::Participant::Target
                        && recipe.increment_tinker_count)
                && let ItemPlace::Contained {
                    container,
                    equipped: 0,
                    ..
                } = next.place
            {
                next.place = ItemPlace::Contained {
                    container,
                    slot: 0,
                    equipped: 0,
                };
            }
            if next != original {
                changes.push(ItemChange {
                    before: Some(original),
                    after: next,
                });
            }
        }
        let proposal = finish(inventory, EntityId(context.actor), changes)?;
        let operation = self.reserve(
            EntityId(context.actor),
            CraftingDecision::Tinker(Box::new(decision)),
            proposal,
            inventory,
        )?;
        self.quotes.remove(&EntityId(context.actor));
        Ok(operation)
    }
    pub(crate) fn salvage(
        &mut self,
        decision: SalvageProposal,
        generated: &[InventoryItem],
        inventory: &mut Inventory,
    ) -> Result<u64, CraftError> {
        if self.pending.len() >= self.capacity
            || !inventory.can_accept()
            || generated.len() != decision.bags.len()
        {
            return Err(CraftError::Capacity);
        }
        if decision.consumed.is_empty() {
            return Err(CraftError::InvalidState);
        }
        let mut changes = Vec::new();
        for (id, revision) in &decision.consumed {
            let old = inventory
                .item(EntityId(*id))
                .ok_or(CraftError::InvalidState)?;
            if old.revision != *revision
                || old.stack != 1
                || inventory.reserved(old.id)
                || !matches!(old.place, ItemPlace::Contained { equipped: 0, .. })
                || old.trade_reserved
                || old.active_pet
                || old.is_container
                || !inventory.owned(EntityId(decision.actor), old.id)
            {
                return Err(CraftError::Ownership);
            }
            let mut next = old.clone();
            next.revision = next.revision.checked_add(1).ok_or(CraftError::Overflow)?;
            next.stack = 0;
            next.place = ItemPlace::Removed;
            changes.push(ItemChange {
                before: Some(old.clone()),
                after: next,
            });
        }
        for (bag, item) in decision.bags.iter().zip(generated) {
            if inventory.item(item.id).is_some()
                || item.id == EntityId(decision.actor)
                || item.template != bag.template
                || item.stack != 1
                || item.revision != 1
                || item.unit_value != bag.value
                || item.structure != Some(bag.units)
                || !matches!(item.place,ItemPlace::Contained{container,equipped:0,..} if container==EntityId(decision.actor))
            {
                return Err(CraftError::InvalidState);
            }
            changes.push(ItemChange {
                before: None,
                after: item.clone(),
            });
        }
        let mut proposal = finish(inventory, EntityId(decision.actor), changes)?;
        let tool = inventory
            .item(EntityId(decision.tool.id))
            .ok_or(CraftError::InvalidState)?;
        if tool.revision != decision.tool.revision
            || !inventory.owned(EntityId(decision.actor), tool.id)
        {
            return Err(CraftError::Ownership);
        }
        if !proposal.participants.iter().any(|(id, _)| *id == tool.id) {
            proposal.participants.push((tool.id, tool.revision));
        }
        self.reserve(
            EntityId(decision.actor),
            CraftingDecision::Salvage(Box::new(decision)),
            proposal,
            inventory,
        )
    }
    fn reserve(
        &mut self,
        actor: EntityId,
        decision: CraftingDecision,
        proposal: InventoryProposal,
        inventory: &mut Inventory,
    ) -> Result<u64, CraftError> {
        let operation = inventory
            .reserve(actor, proposal.clone())
            .map_err(|_| CraftError::Busy)?;
        if inventory.claim(operation).is_err() {
            inventory
                .reject(operation)
                .map_err(|_| CraftError::InvalidState)?;
            return Err(CraftError::InvalidState);
        }
        self.pending.insert(
            operation,
            CraftingTicket {
                operation,
                actor,
                decision,
                inventory: proposal,
                registry_reservations: Vec::new(),
                proficiency: None,
            },
        );
        self.outbox.push_back(operation);
        Ok(operation)
    }
    pub(crate) fn attach_proficiency(
        &mut self,
        operation: u64,
        patch: CraftingProficiency,
    ) -> Result<(), CraftError> {
        self.pending
            .get_mut(&operation)
            .ok_or(CraftError::InvalidState)?
            .proficiency = Some(patch);
        Ok(())
    }
    pub(crate) fn take(&mut self) -> Option<CraftingTicket> {
        while let Some(op) = self.outbox.pop_front() {
            if let Some(p) = self.pending.get(&op) {
                self.submitted.insert(op);
                return Some(p.clone());
            }
        }
        None
    }
    pub(crate) fn retry(&mut self, operation: u64) -> Result<(), CraftError> {
        if !self.pending.contains_key(&operation) {
            return Err(CraftError::InvalidState);
        }
        if !self.outbox.contains(&operation) {
            self.outbox.push_back(operation);
        }
        Ok(())
    }
    pub(crate) fn attach_registries(&mut self, operation: u64, ids: Vec<EntityId>) {
        self.pending
            .get_mut(&operation)
            .expect("new crafting reservation")
            .registry_reservations = ids;
    }
    pub(crate) fn pending(&self, operation: u64) -> Option<&CraftingTicket> {
        self.pending.get(&operation)
    }
    pub(crate) fn committed(
        &mut self,
        receipt: &InventoryReceipt,
        inventory: &mut Inventory,
    ) -> Result<CraftingTicket, CraftError> {
        let ticket = self
            .pending
            .get(&receipt.operation)
            .ok_or(CraftError::InvalidState)?;
        if !self.submitted.contains(&receipt.operation) {
            return Err(CraftError::InvalidState);
        }
        if ticket.inventory.changes.len() != receipt.revisions.len() {
            return Err(CraftError::InvalidState);
        }
        inventory
            .confirm(receipt)
            .map_err(|_| CraftError::InvalidState)?;
        self.outbox.retain(|op| *op != receipt.operation);
        self.submitted.remove(&receipt.operation);
        self.pending
            .remove(&receipt.operation)
            .ok_or(CraftError::InvalidState)
    }
    pub(crate) fn reject(
        &mut self,
        operation: u64,
        inventory: &mut Inventory,
    ) -> Result<(), CraftError> {
        if !self.pending.contains_key(&operation) {
            return Err(CraftError::InvalidState);
        }
        inventory
            .reject(operation)
            .map_err(|_| CraftError::InvalidState)?;
        self.pending.remove(&operation);
        self.submitted.remove(&operation);
        self.outbox.retain(|op| *op != operation);
        Ok(())
    }
}
fn finish(
    inventory: &Inventory,
    actor: EntityId,
    changes: Vec<ItemChange>,
) -> Result<InventoryProposal, CraftError> {
    let items: Vec<_> = inventory.items().cloned().collect();
    let containers: Vec<_> = inventory.containers().copied().collect();
    bace_inventory::propose_item_changes(
        actor,
        changes,
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .map_err(|_| CraftError::InvalidState)
}
fn check_item(inventory: &Inventory, item: &CraftItem, actor: u32) -> Result<(), CraftError> {
    let owned = inventory
        .item(EntityId(item.id))
        .ok_or(CraftError::InvalidState)?;
    if owned.revision != item.revision
        || owned.stack != item.stack
        || owned.trade_reserved
        || owned.active_pet
        || owned.is_container
        || !inventory.owned(EntityId(actor), owned.id)
        || inventory.reserved(owned.id)
    {
        return Err(CraftError::Ownership);
    }
    if !matches!(owned.place, ItemPlace::Contained { equipped: 0, .. }) {
        return Err(CraftError::Ownership);
    }
    Ok(())
}

/// Trusted prepared inputs; client packets carry only identities/confirmation.
#[derive(Clone, Debug)]
pub struct TinkerCommandInput {
    pub context: CraftContext,
    pub source: CraftItem,
    pub target: CraftItem,
    pub recipe: Arc<PreparedRecipe>,
}
#[derive(Clone, Debug)]
pub struct SalvageCommandInput {
    pub actor_revision: u64,
    pub operation_id: [u8; 16],
    pub tool: bace_crafting::SalvageTool,
    pub skills: bace_crafting::SalvageSkills,
    pub items: Vec<bace_crafting::SalvageInput>,
    pub bag_templates: Arc<BTreeMap<u32, u32>>,
    pub generated: Vec<InventoryItem>,
}
#[derive(Clone, Debug)]
pub struct CraftingCommand {
    /// Adapter-owned correlation, echoed even when validation fails.
    pub correlation: u64,
    pub action: CraftingCommandKind,
}
#[derive(Clone, Debug)]
pub enum CraftingCommandKind {
    BeginUse {
        context: bace_gameplay_api::ActionContext,
        input: Box<TinkerCommandInput>,
        motion: Arc<bace_motion::PreparedMotionChain>,
        quote: bool,
        lifetime: u64,
    },
    Execute {
        context: bace_gameplay_api::ActionContext,
        input: Box<TinkerCommandInput>,
    },
    Quote {
        context: bace_gameplay_api::ActionContext,
        input: Box<TinkerCommandInput>,
        lifetime: u64,
    },
    Confirm {
        context: bace_gameplay_api::ActionContext,
        input: Box<TinkerCommandInput>,
    },
    Cancel {
        context: bace_gameplay_api::ActionContext,
    },
    Salvage {
        context: bace_gameplay_api::ActionContext,
        input: Box<SalvageCommandInput>,
    },
    Commit {
        receipt: InventoryReceipt,
    },
    Rollback {
        operation: u64,
    },
    Retry {
        operation: u64,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftingOutcome {
    pub correlation: u64,
    pub result: Result<CraftingResult, CraftError>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CraftingResult {
    Animating {
        actor: EntityId,
        style: u32,
        command: u32,
    },
    Quoted(TinkerChance),
    Pending(u64),
    SalvageEmpty(Box<SalvageProposal>),
    Cancelled,
    Committed(Box<CraftingTicket>),
    RolledBack(u64),
    Retried(u64),
}
impl CraftingCommand {
    /// Call before placing owned inputs on a worker queue. Domain validation is
    /// repeated on the simulation owner; this check bounds retained memory.
    pub fn validate_bounds(&self) -> Result<(), CraftError> {
        match &self.action {
            CraftingCommandKind::BeginUse { input, .. }
            | CraftingCommandKind::Execute { input, .. }
            | CraftingCommandKind::Quote { input, .. }
            | CraftingCommandKind::Confirm { input, .. } => {
                for props in [
                    &input.context.properties,
                    &input.source.properties,
                    &input.target.properties,
                ] {
                    if props.len() > 4096 {
                        return Err(CraftError::Capacity);
                    }
                    let bytes = props
                        .values()
                        .try_fold(0usize, |sum, value| {
                            sum.checked_add(match value {
                                bace_crafting::PropertyValue::String(text) => {
                                    text.len().saturating_add(32)
                                }
                                _ => 32,
                            })
                        })
                        .ok_or(CraftError::Capacity)?;
                    if bytes > 65536 {
                        return Err(CraftError::Capacity);
                    }
                }
                input.recipe.validate_bounds()?;
                if input.source.tinker_log.len() > 128 || input.target.tinker_log.len() > 128 {
                    return Err(CraftError::Capacity);
                }
            }
            CraftingCommandKind::Salvage { input, .. } => {
                if input.items.len() > 300
                    || input.generated.len() > 300
                    || input.bag_templates.len() > 256
                {
                    return Err(CraftError::Capacity);
                }
            }
            CraftingCommandKind::Commit { receipt } if receipt.revisions.len() > 4096 => {
                return Err(CraftError::Capacity);
            }
            _ => {}
        }
        Ok(())
    }
}

pub(crate) struct CraftingAnimation {
    pub(crate) correlation: u64,
    pub(crate) context: bace_gameplay_api::ActionContext,
    pub(crate) input: Box<TinkerCommandInput>,
    pub(crate) token: bace_motion::MotionToken,
    pub(crate) quote: bool,
    pub(crate) lifetime: u64,
    pub(crate) registries: Vec<EntityId>,
}
