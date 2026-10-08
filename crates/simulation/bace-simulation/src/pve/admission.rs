//! Prepared static-creature ownership joins the same Population as generated NPCs.
use super::*;
pub(crate) struct PreparedNpcAdmission {
    actor: EntityId,
    npc: Npc,
    combatant: Combatant,
    policy: AceCreatureLootPolicy,
    event: [u8; 16],
}
impl Population {
    pub(crate) fn prepare_region_npcs(
        &self,
        roots: &[crate::generators::PreparedGeneratorRoot],
        epoch: u64,
        revision: u64,
        now: u64,
    ) -> Result<Vec<PreparedNpcAdmission>, PveError> {
        let mut out = Vec::new();
        for root in roots {
            let Some(template) = &root.creature else {
                continue;
            };
            if out.len() >= self.capacity - self.npcs.len() {
                return Err(PveError::Capacity);
            }
            if self.reserves_identity(root.entity)
                || self.npcs.contains_key(&root.entity)
                || out
                    .iter()
                    .any(|p: &PreparedNpcAdmission| p.actor == root.entity)
            {
                return Err(PveError::Duplicate);
            }
            let mut blueprint = template.blueprint.clone();
            if blueprint.combat.player
                || blueprint.corpse_template == 0
                || blueprint.think_interval == 0
                || !blueprint.visual_range.is_finite()
                || !(0.0..=192.0).contains(&blueprint.visual_range)
                || blueprint.visual_range == 0.0
                || [
                    blueprint.think_interval,
                    blueprint.death_animation_ticks,
                    blueprint.respawn_ticks,
                    blueprint.corpse_decay_ticks,
                ]
                .into_iter()
                .any(|n| n > MAX_TIMER_TICKS || now.checked_add(n).is_none())
                || !blueprint.loot.is_empty()
                || blueprint.xp_override.is_some_and(|x| x < 0)
            {
                return Err(PveError::InvalidProfile);
            }
            if template.requires_equipment != root.loadout.is_some() {
                return Err(PveError::InvalidProfile);
            }
            let mut policy = template
                .ace_loot
                .clone()
                .filter(|_| template.loot.is_none())
                .ok_or(PveError::InvalidProfile)?;
            if self.native.root.is_none()
                || policy.profile_id == 0
                || policy.profile_revision == [0; 32]
                || policy.content_generation == [0; 32]
                || policy.creature_level == 0
                || policy.rare.is_some() != policy.rare_profile_revision.is_some()
            {
                return Err(PveError::InvalidProfile);
            }
            if let Some(loadout) = &root.loadout {
                policy.initial_items = loadout.death_items.clone();
                policy.initial_parents = loadout.death_parents.clone();
                policy.initial_ids = loadout.death_ids.clone();
            }
            let mut geometry = template.geometry.clone();
            geometry.heading = 2.0 * root.location.rotation[2].atan2(root.location.rotation[3]);
            geometry
                .locomotion
                .profile
                .interpret(
                    bace_motion::LocomotionControls::default(),
                    geometry.run_rate,
                )
                .map_err(|_| PveError::InvalidProfile)?;
            blueprint.cell = CellId(root.location.cell);
            blueprint.position = Vec3::new(
                root.location.origin[0],
                root.location.origin[1],
                root.location.origin[2],
            );
            let combatant = Combatant::new(blueprint.combat.clone())
                .and_then(|c| {
                    c.with_resources(Some(template.resources[0]), Some(template.resources[1]))
                })
                .map_err(|_| PveError::InvalidProfile)?;
            let mut event = [0; 16];
            event[..4].copy_from_slice(&root.entity.0.to_le_bytes());
            event[4..12].copy_from_slice(&epoch.to_le_bytes());
            event[12..].copy_from_slice(&(revision as u32).to_le_bytes());
            let origin = GeneratedNpcOrigin {
                generator: root.entity,
                incarnation: epoch,
                content_revision: revision,
                profile: 0,
                child_incarnation: epoch,
            };
            out.push(PreparedNpcAdmission {
                actor: root.entity,
                npc: Npc {
                    combat_ai: ace::source_combat_ai(&policy.source),
                    geometry: Some(geometry),
                    origin: Some(origin),
                    blueprint,
                    next_think: now,
                    sequence: 0,
                    home: None,
                    home_heading: None,
                    returning_since: None,
                    leash: MonsterLeash::default(),
                    next_attack: now,
                    target: None,
                    thought_at: u64::MAX,
                },
                combatant,
                policy,
                event,
            });
        }
        Ok(out)
    }
    /// Caller atomically admitted exactly these bodies and preflighted uniqueness.
    pub(crate) fn adopt_region_npcs(
        &mut self,
        admissions: Vec<PreparedNpcAdmission>,
        world: &mut World,
    ) {
        for admission in admissions {
            world
                .register_combatant(admission.actor, admission.combatant)
                .expect("prepared unique region actor");
            self.native.aces.insert(
                admission.actor,
                ace::AceContext {
                    originals: admission
                        .policy
                        .initial_ids
                        .iter()
                        .copied()
                        .zip(admission.policy.initial_items.iter().cloned())
                        .collect(),
                    policy: admission.policy,
                    event: admission.event,
                },
            );
            self.npcs.insert(admission.actor, admission.npc);
        }
    }
}
