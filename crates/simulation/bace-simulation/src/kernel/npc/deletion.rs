//! Source retention binds actual retirement to its continuation. Removal itself
//! must use the inventory/population/generator owners before this completion.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, NpcSourceCheckpoint, npc::NpcDeleteSourceTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
impl Kernel {
    pub fn prepare_npc_deletion(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<NpcDeleteSourceTicket, E> {
        self.npcs.validate_service(expected)?;
        if !matches!(
            expected.effect,
            NpcEffect::Service(NpcOperation::DeleteSelf)
        ) || self.npcs.deletions.contains_key(&expected.ticket)
        {
            return Err(E::Conflict);
        }
        let actor = expected.context.source;
        if self.characters.get(actor).is_some()
            || self
                .world
                .combatant(actor)
                .is_some_and(|c| c.profile().player)
        {
            return Err(E::Unsupported);
        }
        if self.combat.physical_proc_pending(actor)
            || !self.magic.can_retire_npc(actor)
            || self.inventory.reserved(actor)
            || self.magic.registry_reserved(actor)
                && !self.npcs.deletion_registries.contains(&expected.ticket)
        {
            return Err(E::DurabilityPending);
        }
        let origin = self.population.generated_origin(actor);
        let item = self.inventory.item(actor).is_some();
        if !item {
            if self.world.retirement_hold(actor).is_none() {
                self.population
                    .can_remove_scripted(actor, origin, &self.world)
                    .map_err(|_| E::DurabilityPending)?;
            }
            if self.generators.machines.contains_key(&actor) {
                return Err(E::Unsupported);
            }
            self.validate_transient_npc_equipment(actor)?;
        }
        let hold = self
            .world
            .hold_retirement(actor, expected.ticket)
            .map_err(|_| E::DurabilityPending)?;
        let result = (|| {
            self.prepare_inventory_time()
                .map_err(|_| E::DurabilityPending)?;
            if self.combat.physical_proc_pending(actor) || !self.magic.can_retire_npc(actor) {
                return Err(E::DurabilityPending);
            }
            if !item
                && self.magic.registry(actor).is_some()
                && !self.npcs.deletion_registries.contains(&expected.ticket)
            {
                self.magic
                    .reserve_registry(actor, true, self.tick as f64 / 30.0)
                    .map_err(|_| E::DurabilityPending)?;
                self.npcs.deletion_registries.insert(expected.ticket);
            }
            let archive = self.npcs.archive_preview(actor, &self.world)?;
            let installed = self
                .npcs
                .archives
                .values()
                .try_fold(0usize, |n, a| n.checked_add(a.retained_bytes()?))
                .ok_or(E::Capacity)?;
            let staged = self
                .npcs
                .deletions
                .values()
                .try_fold(archive.retained_bytes().ok_or(E::Capacity)?, |n, p| {
                    n.checked_add(p.archive.retained_bytes()?)
                })
                .and_then(|n| n.checked_add(installed))
                .ok_or(E::Capacity)?;
            if staged > 64 * 1024 * 1024 {
                return Err(E::Capacity);
            }
            let retirement = if item {
                let (identity, member) = self
                    .generators
                    .machines
                    .values()
                    .find_map(|m| {
                        m.member(actor)
                            .map(|member| (m.definition().identity, member))
                    })
                    .ok_or(E::MissingContent)?;
                let effect = bace_gameplay_api::GeneratorLifecycleEffect::DestroyMember {
                    generator: identity,
                    member,
                    recursive: true,
                    include_dead: true,
                    from_unload: false,
                };
                Some(
                    self.claim_scripted_retirement(&effect, expected.ticket)
                        .map_err(|_| E::DurabilityPending)?,
                )
            } else {
                None
            };
            Ok(NpcDeleteSourceTicket {
                npc: expected.clone(),
                archive,
                hold,
                origin,
                retirement,
            })
        })();
        match result {
            Ok(ticket) => {
                self.npcs.deletions.insert(expected.ticket, ticket.clone());
                Ok(ticket)
            }
            Err(error) => {
                if self.npcs.deletion_registries.contains(&expected.ticket) {
                    self.magic
                        .reserve_registry(actor, false, self.tick as f64 / 30.0)
                        .map_err(|_| E::DurabilityPending)?;
                    self.npcs.deletion_registries.remove(&expected.ticket);
                }
                self.world
                    .release_retirement(hold)
                    .map_err(|_| E::Conflict)?;
                Err(error)
            }
        }
    }
    fn validate_transient_npc_equipment(&self, actor: bace_types::EntityId) -> Result<(), E> {
        let mut ids = std::collections::BTreeSet::new();
        for root in self.inventory.items().filter(|i|matches!(i.place,bace_inventory::ItemPlace::Contained{container,..}if container==actor)){
            ids.extend(self.inventory.generated_tree(root.id).map_err(|_|E::DurabilityPending)?);
            if ids.len()>1024{return Err(E::Capacity);}
        }
        let ids: Vec<_> = ids.into_iter().collect();
        // This path can retire unsaved gear directly. Durable gear requires an
        // inventory tombstone stage; never archive the source while leaving its
        // durable children live in storage.
        if ids.iter().any(|id| !self.inventory.item_transient(*id)) {
            return Err(E::Unsupported);
        }
        let mut inventory = self.inventory.clone();
        if !ids.is_empty() {
            inventory
                .remove_generated_tree(&ids)
                .map_err(|_| E::DurabilityPending)?;
        }
        inventory
            .retire_generated_container(actor)
            .map_err(|_| E::DurabilityPending)
    }
    pub fn preview_npc_deletion_checkpoint(
        &self,
        ticket: &NpcDeleteSourceTicket,
        logical_now: f64,
    ) -> Result<NpcSourceCheckpoint, E> {
        if self.npcs.deletions.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        let mut checkpoint = self.preview_npc_committed_checkpoint(
            &ticket.npc,
            NpcCompletion::Applied { post_delay: 0.0 },
            logical_now,
        )?;
        checkpoint.archive = Some(ticket.archive.clone());
        checkpoint.properties = None;
        checkpoint.location = None;
        checkpoint.inventory = None;
        Ok(checkpoint)
    }
    pub fn complete_npc_deletion(&mut self, ticket: &NpcDeleteSourceTicket) -> Result<(), E> {
        if self.npcs.deletions.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        if self.world.contains_identity(ticket.npc.context.source)
            || self
                .inventory
                .item(ticket.npc.context.source)
                .is_some_and(|i| i.place != bace_inventory::ItemPlace::Removed)
        {
            return Err(E::DurabilityPending);
        }
        self.npcs.validate_service(&ticket.npc)?;
        self.npcs
            .install_archive(ticket.archive.clone(), &self.world)?;
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.deletions.remove(&ticket.npc.ticket);
        self.npcs.deletion_retired.remove(&ticket.npc.ticket);
        self.npcs.deletion_committed.remove(&ticket.npc.ticket);
        self.confirm_npc_committed(&ticket.npc)
    }
    pub fn confirm_npc_retirement_committed(
        &mut self,
        ticket: &NpcDeleteSourceTicket,
        receipt: &crate::InventoryReceipt,
    ) -> Result<(), E> {
        if self.npcs.deletions.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        let retirement = ticket.retirement.as_ref().ok_or(E::Unsupported)?;
        if retirement.inventory.operation != receipt.operation {
            return Err(E::Conflict);
        }
        if !self.npcs.deletion_retired.contains(&ticket.npc.ticket) {
            if !self.magic.can_retire_npc(ticket.npc.context.source) {
                return Err(E::DurabilityPending);
            }
            self.inventory
                .validate_receipt(receipt)
                .map_err(|_| E::Conflict)?;
            self.npcs.deletion_committed.insert(ticket.npc.ticket);
            let accepted = self
                .confirm_generated_retirement_with_hold(receipt, Some(ticket.hold))
                .map_err(|_| E::DurabilityPending)?;
            if accepted != *retirement {
                return Err(E::Conflict);
            }
            self.npcs.deletion_retired.insert(ticket.npc.ticket);
        }
        self.retire_npc_combat_assets(ticket.npc.context.source)
            .map_err(|_| E::DurabilityPending)?;
        self.magic
            .retire_npc(ticket.npc.context.source)
            .map_err(|_| E::DurabilityPending)?;
        if let bace_gameplay_api::GeneratorLifecycleEffect::DestroyMember {
            generator,
            member,
            ..
        } = &retirement.effect
        {
            if self
                .generators
                .machines
                .get(&generator.entity)
                .is_some_and(|m| !m.accepts_identity(*generator) && m.owns_member(member.entity))
            {
                return Err(E::Conflict);
            }
            self.notify_generated_entity(
                member.entity,
                bace_gameplay_api::GeneratorNotification::Destruction,
            )
            .map_err(|_| E::DurabilityPending)?;
        }
        self.complete_npc_deletion(ticket)
    }
    pub fn confirm_npc_transient_deletion(
        &mut self,
        ticket: &NpcDeleteSourceTicket,
    ) -> Result<(), E> {
        if self.npcs.deletions.get(&ticket.npc.ticket) != Some(ticket)
            || ticket.retirement.is_some()
        {
            return Err(E::Conflict);
        }
        self.npcs.deletion_committed.insert(ticket.npc.ticket);
        let actor = ticket.npc.context.source;
        if !self.npcs.deletion_retired.contains(&ticket.npc.ticket) {
            if self.combat.physical_proc_pending(actor) || !self.magic.can_retire_npc(actor) {
                return Err(E::DurabilityPending);
            }
            self.population
                .can_remove_scripted_held(ticket.hold, ticket.origin, &self.world)
                .map_err(|_| E::DurabilityPending)?;
            let inherited = if self.npcs.deletion_registries.contains(&ticket.npc.ticket) {
                vec![actor]
            } else {
                vec![]
            };
            self.retire_generated_creature_equipment_with_holds(actor, &inherited)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.deletion_registries.remove(&ticket.npc.ticket);
            self.population
                .remove_scripted_held(ticket.hold, ticket.origin, &mut self.world)
                .map_err(|_| E::DurabilityPending)?;
            self.combat.retire_actor(actor);
            self.npcs.deletion_retired.insert(ticket.npc.ticket);
        }
        self.retire_npc_combat_assets(actor)
            .map_err(|_| E::DurabilityPending)?;
        self.magic
            .retire_npc(actor)
            .map_err(|_| E::DurabilityPending)?;
        if let Some(origin) = ticket.origin {
            if self
                .generators
                .machines
                .get(&origin.generator)
                .is_some_and(|m| {
                    !m.accepts_revision(origin.incarnation, origin.content_revision)
                        && m.owns_member(actor)
                })
            {
                return Err(E::Conflict);
            }
            self.notify_generated_entity(
                actor,
                bace_gameplay_api::GeneratorNotification::Destruction,
            )
            .map_err(|_| E::DurabilityPending)?;
        }
        self.complete_npc_deletion(ticket)
    }
    pub fn reject_npc_deletion(&mut self, ticket: &NpcDeleteSourceTicket) -> Result<(), E> {
        if self.npcs.deletions.get(&ticket.npc.ticket) != Some(ticket)
            || self.world.body(ticket.npc.context.source).is_err()
            || self.npcs.deletion_committed.contains(&ticket.npc.ticket)
        {
            return Err(E::Conflict);
        }
        if let Some(retirement) = &ticket.retirement {
            self.reject_generated_retirement_inner(retirement.inventory.operation)
                .map_err(|_| E::DurabilityPending)?;
        }
        if self.npcs.deletion_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.npc.context.source, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.deletion_registries.remove(&ticket.npc.ticket);
        }
        self.world
            .release_retirement(ticket.hold)
            .map_err(|_| E::Conflict)?;
        self.npcs.deletions.remove(&ticket.npc.ticket);
        Ok(())
    }
}
