//! Detached script metadata after source destruction. This is never a physical
//! actor and cannot authorize movement, collision, targeting or new interaction.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcSourceLocation {
    pub facts: bace_emotes::NpcActorFacts,
    pub cell: bace_types::CellId,
    pub position: bace_geometry::Vec3,
    pub heading: f32,
}
impl NpcSourceLocation {
    pub fn validate(&self) -> Result<(), NpcFailure> {
        if self.cell.0 == 0
            || self.facts.player && !self.facts.creature
            || !self.position.is_finite()
            || !self.heading.is_finite()
        {
            return Err(NpcFailure::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSourceArchive {
    pub source: EntityId,
    pub facts: bace_emotes::NpcActorFacts,
    pub cell: bace_types::CellId,
    pub position: bace_geometry::Vec3,
    pub heading: f32,
    pub properties: bace_entity::EntityProperties,
}
impl NpcSourceArchive {
    pub fn retained_bytes(&self) -> Option<usize> {
        self.properties.retained_bytes()?.checked_add(128)
    }
    pub fn validate(&self) -> Result<(), NpcFailure> {
        if self.source.0 == 0
            || self.cell.0 == 0
            || self.facts.player && !self.facts.creature
            || !self.position.is_finite()
            || !self.heading.is_finite()
            || self.retained_bytes().is_none_or(|n| n > 2 * 1024 * 1024)
        {
            return Err(NpcFailure::InvalidInput);
        }
        Ok(())
    }
}
impl Npcs {
    pub(crate) fn register_archived(
        &mut self,
        snapshot: NpcSourceCheckpoint,
        program: Arc<NativeProgram>,
        use_radius: f32,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        let id = snapshot.source;
        if snapshot.archive.as_ref().is_none_or(|a| a.source != id)
            || self.sources.contains_key(&id)
            || self.sources.len() >= self.capacity
            || !use_radius.is_finite()
        {
            return Err(NpcFailure::InvalidInput);
        }
        self.sources.insert(
            id,
            Source {
                admission_bound: false,
                admission_hold: None,
                admission: None,
                manager: NativeEmoteManager::new(program),
                random: None,
                use_radius,
                clock_offset: 0.0,
                event_id: [0; 16],
                key_version: 0,
                active_operation: 0,
                invocations: BTreeMap::new(),
                recovery_ready: false,
                journal_hold: None,
                held_logical_now: None,
            },
        );
        self.source_order.push(id);
        if let Err(error) = self.restore_source(snapshot, tick) {
            self.sources.remove(&id);
            self.source_order.retain(|source| *source != id);
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn archive_preview(
        &self,
        source: EntityId,
        world: &World,
    ) -> Result<NpcSourceArchive, NpcFailure> {
        if self.archives.contains_key(&source) {
            return Err(NpcFailure::Conflict);
        }
        let (cell, state) = world
            .actor_state(source)
            .map_err(|_| NpcFailure::MissingActor)?;
        if world
            .properties(source)
            .and_then(|p| p.retained_bytes())
            .is_none_or(|n| n > 2 * 1024 * 1024 - 128)
        {
            return Err(NpcFailure::Capacity);
        }
        let archive = NpcSourceArchive {
            source,
            cell,
            position: state.position(),
            heading: state.heading_radians(),
            facts: bace_emotes::NpcActorFacts {
                player: world.combatant(source).is_some_and(|c| c.profile().player),
                creature: world.combatant(source).is_some(),
            },
            properties: world
                .properties(source)
                .ok_or(NpcFailure::MissingContent)?
                .clone(),
        };
        archive.validate()?;
        if self
            .archives
            .values()
            .try_fold(
                archive.retained_bytes().ok_or(NpcFailure::Capacity)?,
                |n, a| n.checked_add(a.retained_bytes()?),
            )
            .is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err(NpcFailure::Capacity);
        }
        Ok(archive)
    }
    pub(crate) fn install_archive(
        &mut self,
        archive: NpcSourceArchive,
        world: &World,
    ) -> Result<(), NpcFailure> {
        archive.validate()?;
        if world.body(archive.source).is_ok() {
            return Err(NpcFailure::Conflict);
        }
        if let Some(existing) = self.archives.get(&archive.source) {
            return if existing == &archive {
                Ok(())
            } else {
                Err(NpcFailure::Conflict)
            };
        }
        let total = self
            .archives
            .values()
            .try_fold(
                archive.retained_bytes().ok_or(NpcFailure::Capacity)?,
                |n, a| n.checked_add(a.retained_bytes()?),
            )
            .ok_or(NpcFailure::Capacity)?;
        if total > 64 * 1024 * 1024 || self.archives.len() >= self.capacity {
            return Err(NpcFailure::Capacity);
        }
        self.archives.insert(archive.source, archive);
        Ok(())
    }
    pub(crate) fn release_archive(&mut self, source: EntityId) -> Result<(), NpcFailure> {
        let state = self.sources.get(&source).ok_or(NpcFailure::MissingActor)?;
        if state.manager.busy()
            || state.manager.has_detached()
            || self
                .pending
                .values()
                .any(|p| p.proposal.context.source == source)
            || self
                .notifications
                .iter()
                .any(|p| p.context.source == source)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        self.archives.remove(&source).ok_or(NpcFailure::Conflict)?;
        self.durable_dirty.remove(&source);
        self.sources.remove(&source);
        self.source_order.retain(|id| *id != source);
        self.quests.remove(&source);
        Ok(())
    }
}
