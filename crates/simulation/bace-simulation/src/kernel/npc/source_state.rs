//! Live source snapshots are owned by World; player aggregates retain their own save owner.
use super::Kernel;
use crate::{NpcEffect, NpcSourceCheckpoint};
use bace_gameplay_api::NpcFailure as E;
impl Kernel {
    pub(super) fn capture_npc_source_state(
        &self,
        mut snapshot: NpcSourceCheckpoint,
    ) -> Result<NpcSourceCheckpoint, E> {
        if snapshot.archive.is_some() {
            snapshot.inventory = None;
            snapshot.location = None;
            snapshot.properties = None;
            return Ok(snapshot);
        }
        if self.characters.get(snapshot.source).is_some() {
            snapshot.location = None;
            snapshot.properties = None;
            snapshot.source_quests = None;
            return Ok(snapshot);
        }
        let mut properties = self
            .world
            .properties(snapshot.source)
            .cloned()
            .ok_or(E::MissingActor)?;
        for pending in &snapshot.pending {
            if pending.adopted
                && let NpcEffect::Property {
                    actor,
                    change,
                    aggregate: None,
                } = &pending.proposal.effect
                && *actor == snapshot.source
                && properties.revision() < change.after_revision
            {
                properties.adopt(change.clone()).map_err(|_| E::Conflict)?;
            }
        }
        if properties
            .retained_bytes()
            .is_none_or(|n| n > 2 * 1024 * 1024)
        {
            return Err(E::Capacity);
        }
        self.validate_npc_scalar_properties(snapshot.source, &properties)
            .map_err(|_| E::InvalidInput)?;
        let (cell, state) = self
            .world
            .actor_state(snapshot.source)
            .map_err(|_| E::MissingActor)?;
        snapshot.location = Some(crate::npc::NpcSourceLocation {
            cell,
            position: state.position(),
            heading: state.heading_radians(),
            facts: bace_emotes::NpcActorFacts {
                player: false,
                creature: self.world.combatant(snapshot.source).is_some(),
            },
        });
        snapshot.properties = Some(properties);
        Ok(snapshot)
    }
    pub(super) fn restore_idle_npc_source_state(
        &mut self,
        snapshot: NpcSourceCheckpoint,
    ) -> Result<(), E> {
        if snapshot.archive.is_some()
            || !snapshot.pending.is_empty()
            || !snapshot.vm.work.is_empty()
            || !snapshot.vm.pending.is_empty()
            || !snapshot.vm.detached.is_empty()
            || self.characters.get(snapshot.source).is_some()
        {
            return Err(E::InvalidInput);
        }
        let actor = snapshot.source;
        let quests = snapshot
            .source_quests
            .map(|(revision, rows)| {
                bace_quests::QuestRegistry::restore_snapshot(revision, rows)
                    .map_err(|_| E::InvalidInput)
            })
            .transpose()?;
        if let Some(properties) = &snapshot.properties {
            if properties
                .retained_bytes()
                .is_none_or(|n| n > 2 * 1024 * 1024)
                || self.world.properties(actor).is_none()
            {
                return Err(E::InvalidInput);
            }
            self.validate_npc_scalar_properties(actor, properties)
                .map_err(|_| E::InvalidInput)?;
        }
        self.npcs.restore_idle_quests(actor, quests)?;
        if let Some(properties) = snapshot.properties {
            *self.world.properties_mut(actor).ok_or(E::MissingActor)? = properties;
            self.refresh_npc_scalar_source(actor)
                .map_err(|_| E::InvalidInput)?;
        }
        Ok(())
    }
    pub(super) fn restore_npc_source_state(
        &mut self,
        snapshot: NpcSourceCheckpoint,
    ) -> Result<(), E> {
        let actor = snapshot.source;
        if snapshot.archive.is_some() && snapshot.properties.is_some() {
            return Err(E::InvalidInput);
        }
        if self.characters.get(actor).is_some()
            && (snapshot.properties.is_some() || snapshot.source_quests.is_some())
        {
            return Err(E::Conflict);
        }
        let properties = snapshot.properties.clone();
        if let Some(properties) = &properties {
            if properties
                .retained_bytes()
                .is_none_or(|n| n > 2 * 1024 * 1024)
            {
                return Err(E::Capacity);
            }
            self.validate_npc_scalar_properties(actor, properties)
                .map_err(|_| E::InvalidInput)?;
            if self.world.properties_mut(actor).is_none() {
                return Err(E::MissingActor);
            }
        }
        self.npcs.restore_source(snapshot, self.tick)?;
        if let Some(properties) = properties {
            *self
                .world
                .properties_mut(actor)
                .expect("source property owner preflight") = properties;
            self.refresh_npc_scalar_source(actor)
                .map_err(|_| E::InvalidInput)?;
        }
        Ok(())
    }
}
