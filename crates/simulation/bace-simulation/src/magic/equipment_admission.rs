//! Reserved equipment companions update existing owners after the exact receipt.
use super::*;
impl Magic {
    pub(crate) fn validate_equipment_profile(
        &self,
        actor: EntityId,
        profile: &bace_magic::MagicDamageProfile,
        values: &[bace_character::SkillValues],
    ) -> Result<(), CastRejection> {
        if !self.registry_reserved(actor)
            || self.busy(actor)
            || !self.casters.contains_key(&actor)
            || self
                .damage_profiles
                .get(&actor)
                .is_none_or(|p| p.player != profile.player)
        {
            return Err(CastRejection::Busy);
        }
        bace_magic::validate_magic_damage_profile(profile)
            .map_err(|_| CastRejection::InvalidState)?;
        if profile.wand.as_ref().is_some_and(|w| {
            self.damage_wands
                .get(&w.entity)
                .is_some_and(|(old, _)| old.revision > w.revision)
        }) || [31, 32, 33, 34, 43, 15, 16, 48].iter().any(|id| {
            !values
                .iter()
                .any(|s| s.skill == *id && s.current <= i32::MAX as u32)
        }) {
            return Err(CastRejection::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn adopt_equipment_profile(
        &mut self,
        actor: EntityId,
        mut profile: bace_magic::MagicDamageProfile,
        values: &[bace_character::SkillValues],
        shield: Option<crate::PreparedShield>,
    ) {
        profile.current_enemy = self.damage_profiles[&actor].current_enemy;
        self.publish_damage_profile(actor, profile);
        let skill = |id| {
            values
                .iter()
                .find(|s| s.skill == id)
                .expect("preflighted equipment skill")
        };
        self.refresh_caster_skills(
            actor,
            [34, 33, 31, 32, 43].map(|id| skill(id).current),
            skill(15).current,
            skill(16).current,
        )
        .expect("preflighted caster values");
        let combat_skill = |id| {
            let s = skill(id);
            bace_combat::specialization::CombatSkill {
                advancement: s.advancement,
                base: s.base,
                current: s.current,
            }
        };
        self.refresh_magic_defenses(
            actor,
            MagicDefenseProfile {
                player: true,
                magic_defense: combat_skill(15),
                shield: shield.map(|s| (combat_skill(48), s.magic_absorption)),
            },
        )
        .expect("preflighted shield profile");
        self.invalidate_actor_programs(actor);
    }
}
