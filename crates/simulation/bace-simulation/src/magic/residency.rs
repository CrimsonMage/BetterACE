//! Source region shutdown retires launched spell projectiles through their owner.
use super::*;
impl Magic {
    /// Bounded partial progress retains removal projections before geometry can
    /// be evicted. Pending damage/enchantment events retain their normal order.
    pub(crate) fn retire_region_projectiles(&mut self, world: &mut World, landblock: u16) -> bool {
        let ids: Vec<_> = self
            .flying
            .keys()
            .copied()
            .filter(|&id| {
                world
                    .projectile(id)
                    .is_some_and(|p| p.cell.0 >> 16 == u32::from(landblock))
            })
            .take(32)
            .collect();
        for id in ids {
            if self.events.len() >= self.capacity {
                return false;
            }
            self.flying.remove(&id);
            world.remove_projectile(id);
            self.events.push_back(MagicEvent::ProjectileRemoved {
                actor: id,
                tick: self.event_tick,
            });
        }
        !self.flying.keys().any(|&id| {
            world
                .projectile(id)
                .is_some_and(|p| p.cell.0 >> 16 == u32::from(landblock))
        })
    }
}
impl Magic {
    /// Shutdown-only cleanup after identity owners and durable registries have
    /// retired. Cold spell definitions and unused allocated IDs carry no work;
    /// registry holds, errors, effects and exact outputs must still drain.
    pub(crate) fn clear_idle_assets(&mut self, world: &World) -> bool {
        if self.has_state()
            || !self.component_operations.is_empty()
            || !self.claimed_emote_outcomes.is_empty()
            || !self.completed_item_procs.is_empty()
            || self
                .registry_clocks
                .values()
                .any(|clock| clock.reserved || clock.error.is_some())
            || self.casters.keys().any(|actor| world.body(*actor).is_ok())
        {
            return false;
        }
        self.ids.clear();
        self.spells.clear();
        self.projectile_shapes.clear();
        self.casters.clear();
        self.object_casters.clear();
        self.server_mana.clear();
        self.spell_categories.clear();
        self.recovery.clear();
        self.actor_components.clear();
        self.actor_program_spells.clear();
        self.validated_actor_programs.clear();
        self.vitae_template = None;
        self.vitae_sequences.clear();
        self.monster_ai.clear();
        self.defense_profiles.clear();
        self.damage_profiles.clear();
        self.damage_wands.clear();
        self.damage_spell_levels.clear();
        self.damage_spell_flags.clear();
        self.item_types.clear();
        self.item_spell_qualities.clear();
        self.spell_target_masks.clear();
        self.item_targets.clear();
        self.registry_clocks.clear();
        self.enchantment_metadata.clear();
        self.periodic_profiles.clear();
        self.server_sequences.clear();
        true
    }
}
