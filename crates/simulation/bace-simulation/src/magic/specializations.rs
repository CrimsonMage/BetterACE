//! Prepared specialization consumers. Formulas are pinned ACE SpellProjectile
//! GetShieldMod/GetAbsorbMod and Creature_Rating.GetSpecDefenseBonus; their scalar
//! implementations and original-source oracle live in bace-combat.
use super::*;
use bace_combat::specialization::{
    CombatSkill, DefenseKind, shield_magic_modifier, specialized_defense_rating,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicDefenseProfile {
    pub player: bool,
    pub magic_defense: CombatSkill,
    /// Present only when authoritative equipment/combat mode selects a shield.
    /// Missile launchers and wands use a different absorption path in ACE.
    pub shield: Option<(CombatSkill, f32)>,
}
impl Magic {
    pub(crate) fn refresh_magic_defenses(
        &mut self,
        actor: EntityId,
        profile: MagicDefenseProfile,
    ) -> Result<(), CastRejection> {
        if !self.registries.contains_key(&actor) {
            return Err(CastRejection::MissingActor);
        }
        if let Some((skill, cap)) = profile.shield {
            shield_magic_modifier(skill, 0.0, cap).map_err(|_| CastRejection::InvalidState)?;
        }
        self.defense_profiles.insert(actor, profile);
        Ok(())
    }
    pub(super) fn projectile_damage(
        &self,
        source: EntityId,
        target: EntityId,
        projectile: EntityId,
        damage: u32,
        world: &World,
    ) -> Result<u32, CastRejection> {
        let Some(profile) = self.defense_profiles.get(&target) else {
            return Ok(damage);
        };
        let rating =
            specialized_defense_rating(profile.player, DefenseKind::Magic, profile.magic_defense);
        let mut shield_mod = 1.0;
        if let Some((skill, cap)) = profile.shield
            && world.combatant(target).is_some_and(|c| c.mode() == 2)
        {
            let (_, state) = world
                .actor_state(target)
                .map_err(|_| CastRejection::MissingActor)?;
            let shot = world
                .projectile(projectile)
                .ok_or(CastRejection::MissingActor)?;
            let offset = shot.body.position() - state.position();
            let angle = heading_delta(state.heading_radians(), (-offset.x).atan2(offset.y))
                .map_err(|_| CastRejection::InvalidState)?
                .to_degrees();
            shield_mod = shield_magic_modifier(skill, angle, cap)
                .map_err(|_| CastRejection::InvalidState)?;
            if profile.player && self.casters.get(&source).is_some_and(|c| c.player) {
                // ACE PvP Aegis reduction: 72% effectiveness in melee/missile.
                shield_mod = 1.0 - (1.0 - shield_mod) * 0.72;
            }
        }
        Ok(
            ((damage as f32 * shield_mod) * (100.0 / (100.0 + rating as f32))).round_ties_even()
                as u32,
        )
    }
    pub(crate) fn apply_dirty_fighting(
        &mut self,
        impact: crate::combat::DirtyFightingImpact,
        world: &World,
        now: f64,
    ) -> Result<(), CastRejection> {
        if impact.count == 0 || impact.count > impact.spells.len() {
            return Err(CastRejection::InvalidState);
        }
        if world.actor_state(impact.attacker).is_err() || world.actor_state(impact.target).is_err()
        {
            return Err(CastRejection::MissingActor);
        }
        self.apply_prepared_enchantments(
            impact.attacker,
            impact.target,
            &impact.spells[..impact.count],
            now,
        )
    }

    /// Apply the already-decided combat proc without casting, mana, components or
    /// extra RNG. On any refusal the caller retains the entire bounded request.
    /// Both spells (high Dirty Fighting) commit together or neither is changed.
    pub(crate) fn apply_prepared_enchantments(
        &mut self,
        source: EntityId,
        target: EntityId,
        spells: &[u32],
        now: f64,
    ) -> Result<(), CastRejection> {
        if source.0 == 0
            || source.0 == u32::MAX
            || spells.is_empty()
            || spells.len() > 2
            || spells.len() == 2 && spells[0] == spells[1]
        {
            return Err(CastRejection::InvalidState);
        }
        if !now.is_finite() || now < self.current_time {
            return Err(CastRejection::InvalidState);
        }
        let clock = self
            .registry_clocks
            .get(&target)
            .ok_or(CastRejection::MissingAssets)?;
        if clock.reserved || clock.error.is_some() || clock.active && clock.next_due <= now {
            return Err(CastRejection::Busy);
        }
        if self.events.len() + spells.len() > self.capacity {
            return Err(CastRejection::Capacity);
        }
        let registry = self
            .registries
            .get(&target)
            .ok_or(CastRejection::MissingAssets)?;
        // At most two proc effects, infrequent relative to ticks. A bounded
        // candidate preserves all-or-nothing semantics for stacked refreshes.
        let mut candidate = EnchantmentRegistry::restore(
            registry.capacity(),
            registry.revision(),
            registry.entries().to_vec(),
        )
        .map_err(|_| CastRejection::InvalidState)?;
        let mut events = Vec::with_capacity(spells.len());
        for &spell in spells {
            let prepared = self
                .spells
                .get(&spell)
                .ok_or(CastRejection::MissingAssets)?;
            let SpellEffect::Enchantment(spec) = &prepared.spell.effect else {
                return Err(CastRejection::MissingAssets);
            };
            let metadata = *self
                .enchantment_metadata
                .get(&spell)
                .ok_or(CastRejection::MissingAssets)?;
            let update = candidate
                .add(
                    EnchantmentEntry {
                        spell,
                        caster: source.0,
                        school: prepared.spell.school,
                        spec: spec.clone(),
                        start_time: 0.0,
                        is_set_spell: spec.set_id.is_some(),
                        is_level8_aura: false,
                        metadata,
                    },
                    now,
                    false,
                )
                .map_err(|_| CastRejection::InvalidState)?;
            events.push(MagicEvent::Enchantment {
                actor: target,
                entry: update.entry,
            });
        }
        self.registries.insert(target, candidate);
        self.events.extend(events);
        Ok(())
    }
}
