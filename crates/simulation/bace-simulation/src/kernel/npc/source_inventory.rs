//! A source journal captures exact gear identities and registry images while
//! the existing inventory and enchantment owners retain their reservations.
use super::Kernel;
use crate::npc::NpcSourceInventorySnapshot;
use bace_gameplay_api::NpcFailure as E;
use bace_types::EntityId;
pub(super) fn release_action(action: &crate::NpcServiceAction) -> Option<(EntityId, u64, bool)> {
    use crate::NpcServiceAction as A;
    let proposal = match action {
        A::CommitPlayerEffect { proposal }
        | A::CommitCast { proposal, .. }
        | A::CommitMotion { proposal, .. }
        | A::AdmitExperience { proposal }
        | A::AdmitService { proposal, .. } => Some((proposal, true)),
        A::ReleaseJournal { proposal } => Some((proposal, false)),
        A::CommitTrainingCredits { ticket } => Some((&ticket.npc, true)),
        A::RejectTrainingCredits { ticket } => Some((&ticket.npc, false)),
        A::CommitSpellbook { ticket } => Some((&ticket.npc, true)),
        A::RejectSpellbook { ticket } => Some((&ticket.npc, false)),
        A::CommitSkillReset { ticket } => Some((&ticket.npc, true)),
        A::RejectSkillReset { ticket } => Some((&ticket.npc, false)),
        A::CommitInventory { ticket, .. } => Some((&ticket.npc, true)),
        A::RejectInventory { ticket } => Some((&ticket.npc, false)),
        A::CommitTeleport { ticket, .. } => Some((&ticket.npc, true)),
        A::RejectTeleport { ticket } => Some((&ticket.npc, false)),
        A::CommitHandIn { ticket, .. } => {
            return Some((ticket.request.source, ticket.inventory.operation, true));
        }
        A::RejectHandIn { ticket } => {
            return Some((ticket.request.source, ticket.inventory.operation, false));
        }
        A::CommitIdle { source, operation } => return Some((*source, *operation, true)),
        A::ReleaseIdle { source, operation } => return Some((*source, *operation, false)),
        _ => None,
    }?;
    Some((proposal.0.context.source, proposal.0.ticket, proposal.1))
}
impl Kernel {
    pub(in crate::kernel) fn commit_npc_source_inventory(
        &mut self,
        proposal: &crate::NpcProposal,
    ) -> Result<(), E> {
        self.release_npc_source_inventory(proposal.context.source, proposal.ticket, true)
    }
    pub(super) fn capture_npc_source_inventory(
        &mut self,
        source: EntityId,
    ) -> Result<Option<NpcSourceInventorySnapshot>, E> {
        let Some(ticket) = self.npcs.source_hold(source) else {
            return Ok(None);
        };
        if self.world.body(source).is_err() || self.characters.get(source).is_some() {
            return Ok(None);
        }
        if let Some(snapshot) = self.npcs.source_inventory.get(&source) {
            return if snapshot.ticket == ticket {
                Ok(Some(snapshot.clone()))
            } else {
                Err(E::DurabilityPending)
            };
        }
        if self.generated_enchantments.pending_for(source)
            || self.combat.physical_proc_pending(source)
        {
            return Err(E::DurabilityPending);
        }
        self.prepare_inventory_time()
            .map_err(|_| E::DurabilityPending)?;
        let ids = self
            .inventory
            .npc_descendants(source)
            .map_err(|_| E::Capacity)?;
        let now = self.tick as f64 / 30.0;
        let mut held = Vec::new();
        let result = (|| {
            for id in std::iter::once(source).chain(ids.iter().copied()) {
                if self.inventory.reserved(id) {
                    return Err(E::DurabilityPending);
                }
                if self.magic.registry(id).is_none() {
                    continue;
                }
                if self.magic.registry_reserved(id) {
                    return Err(E::DurabilityPending);
                }
                self.magic
                    .reserve_registry(id, true, now)
                    .map_err(|_| E::DurabilityPending)?;
                held.push(id);
            }
            self.sync_registry_revisions()
                .map_err(|_| E::DurabilityPending)?;
            if held
                .iter()
                .try_fold(0usize, |n, id| {
                    n.checked_add(self.magic.registry(*id).map_or(0, |r| r.entries().len()))
                })
                .is_none_or(|n| n > 65536)
            {
                return Err(E::Capacity);
            }
            self.inventory
                .hold_npc_source_items(source, ticket, &ids)
                .map_err(|_| E::DurabilityPending)?;
            let items = ids
                .iter()
                .map(|&id| {
                    let registry = self.magic.registry(id);
                    Ok(crate::RegionUnloadItem {
                        item: self.inventory.item(id).ok_or(E::Conflict)?.clone(),
                        transient: self.inventory.item_transient(id),
                        container: self.inventory.container(id).copied(),
                        registry_revision: registry.map(|r| r.revision()),
                        enchantments: registry.map_or_else(Vec::new, |r| r.entries().to_vec()),
                        position: None,
                        corpse: None,
                    })
                })
                .collect::<Result<Vec<_>, E>>()?;
            let registry = self.magic.registry(source);
            let root_item = self
                .inventory
                .item(source)
                .map(|item| {
                    let mut item = item.clone();
                    item.revision = item.revision.checked_add(1).ok_or(E::Capacity)?;
                    Ok(crate::RegionUnloadItem {
                        item,
                        transient: self.inventory.item_transient(source),
                        container: self.inventory.container(source).copied(),
                        registry_revision: registry.map(|r| r.revision()),
                        enchantments: vec![],
                        position: Some(
                            self.accepted_portal_position(source)
                                .map_err(|_| E::MissingActor)?,
                        ),
                        corpse: None,
                    })
                })
                .transpose()?;
            let death_items = self
                .population
                .npc_loot_initial_ids(source)
                .unwrap_or(&[])
                .iter()
                .copied()
                .filter(|id| ids.contains(id))
                .collect();
            Ok(NpcSourceInventorySnapshot {
                root_item,
                source,
                ticket,
                origin: self.population.generated_origin(source),
                items,
                death_items,
                source_registry_revision: registry.map(|r| r.revision()),
                source_enchantments: registry.map_or_else(Vec::new, |r| r.entries().to_vec()),
            })
        })();
        match result {
            Ok(snapshot) => {
                self.npcs.source_inventory.insert(source, snapshot.clone());
                Ok(Some(snapshot))
            }
            Err(error) => {
                self.inventory.release_npc_source_items(source, ticket);
                for id in held {
                    self.magic
                        .reserve_registry(id, false, now)
                        .expect("same-tick NPC source registry rollback");
                }
                Err(error)
            }
        }
    }
    pub(super) fn release_npc_source_inventory(
        &mut self,
        source: EntityId,
        ticket: u64,
        committed: bool,
    ) -> Result<(), E> {
        let Some(snapshot) = self.npcs.source_inventory.get(&source) else {
            return Ok(());
        };
        if snapshot.ticket != ticket {
            return Err(E::Conflict);
        }
        if committed {
            self.inventory
                .adopt_npc_source_items(source, ticket, snapshot.root_item.as_ref())
                .map_err(|_| E::Conflict)?;
        }
        let now = self.tick as f64 / 30.0;
        if snapshot.source_registry_revision.is_some() {
            self.magic
                .reserve_registry(source, false, now)
                .expect("exact held NPC source registry");
        }
        for item in &snapshot.items {
            if item.registry_revision.is_some() {
                self.magic
                    .reserve_registry(item.item.id, false, now)
                    .expect("exact held NPC source registry");
            }
        }
        self.inventory.release_npc_source_items(source, ticket);
        self.npcs.source_inventory.remove(&source);
        Ok(())
    }
}
