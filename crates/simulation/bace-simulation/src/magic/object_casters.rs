//! Resource-free instant world-object sources are distinct from creatures. A
//! door/trap keeps its real body and qualities; no combatant or vital is created.
use super::*;
impl Magic {
    pub(crate) fn register_object_caster(
        &mut self,
        actor: EntityId,
        source: &bace_content::WeenieV1,
        world: &World,
    ) -> Result<(), CastRejection> {
        if source
            .properties
            .strings
            .iter()
            .any(|p| p.id == 1 && p.value.len() > 1024)
            || world.combatant(actor).is_some()
            || world.body(actor).is_err()
            || !source.properties.skills.is_empty()
            || !source.properties.attributes.is_empty()
        {
            return Err(CastRejection::MissingAssets);
        }
        source
            .validate(Default::default())
            .map_err(|_| CastRejection::InvalidState)?;
        if self.casters.contains_key(&actor) && !self.object_casters.contains_key(&actor) {
            return Err(CastRejection::InvalidState);
        }
        if !self.casters.contains_key(&actor) && self.casters.len() >= 4096
            || !self.registries.contains_key(&actor) && self.registries.len() >= 4096
        {
            return Err(CastRejection::Capacity);
        }
        if self.registry_reserved(actor) {
            return Err(CastRejection::Busy);
        }
        // Missing native skill/attribute qualities have source zero defaults;
        // objects with actual skill tables require the creature/DAT owner join.
        let damage = bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
            player: false,
            weenie: source,
            equipment: &[],
            skills: &[],
            base_attributes: [0; 6],
            base_shield_skill: 0,
        })
        .map_err(|_| CastRejection::InvalidState)?;
        if !self.registries.contains_key(&actor) {
            self.register_registry(
                actor,
                EnchantmentRegistry::new(512).map_err(|_| CastRejection::Capacity)?,
                true,
                self.current_time,
            )
            .map_err(|(error, _)| error)?;
        }
        self.casters.insert(
            actor,
            MagicCaster {
                player: false,
                known_spells: BTreeSet::new(),
                school_skills: [0; 5],
                magic_defense: 0,
                mana_conversion: 0,
                components_required: false,
                safe_components: true,
            },
        );
        self.object_casters.insert(actor, Arc::new(source.clone()));
        self.recovery.entry(actor).or_default();
        self.publish_damage_profile(actor, damage);
        Ok(())
    }
    pub(crate) fn has_object_caster(&self, actor: EntityId) -> bool {
        self.object_casters.contains_key(&actor)
    }
    pub(crate) fn validate_object_properties(
        &self,
        actor: EntityId,
        properties: &bace_entity::EntityProperties,
    ) -> Result<(), CastRejection> {
        let Some(source) = self.object_casters.get(&actor) else {
            return Ok(());
        };
        let source = crate::npc_combat_assets::overlay_npc_source_qualities(source, properties)
            .map_err(|_| CastRejection::InvalidState)?;
        bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
            player: false,
            weenie: &source,
            equipment: &[],
            skills: &[],
            base_attributes: [0; 6],
            base_shield_skill: 0,
        })
        .map_err(|_| CastRejection::InvalidState)?;
        Ok(())
    }
    pub(crate) fn refresh_object_properties(
        &mut self,
        actor: EntityId,
        world: &World,
    ) -> Result<(), CastRejection> {
        let Some(source) = self.object_casters.get(&actor) else {
            return Ok(());
        };
        let properties = world
            .properties(actor)
            .ok_or(CastRejection::MissingAssets)?;
        let updated = crate::npc_combat_assets::overlay_npc_source_qualities(source, properties)
            .map_err(|_| CastRejection::InvalidState)?;
        if updated != **source {
            let damage =
                bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
                    player: false,
                    weenie: &updated,
                    equipment: &[],
                    skills: &[],
                    base_attributes: [0; 6],
                    base_shield_skill: 0,
                })
                .map_err(|_| CastRejection::InvalidState)?;
            self.object_casters.insert(actor, Arc::new(updated));
            self.publish_damage_profile(actor, damage);
        }
        Ok(())
    }
    pub(super) fn spell_permission(
        &self,
        policy: &Combat,
        source: EntityId,
        target: EntityId,
        helpful: bool,
        world: &World,
    ) -> Result<(), CastRejection> {
        if !self.object_casters.contains_key(&source) {
            return policy.spell_permission(source, target, helpful, world);
        }
        world
            .body(source)
            .map_err(|_| CastRejection::MissingActor)?;
        if source == target && helpful {
            return Ok(());
        }
        let target_state = world
            .combatant(target)
            .ok_or(CastRejection::InvalidTarget)?;
        if !helpful
            && (target_state.lifestone_protected()
                || policy
                    .physical_profile(target)
                    .is_some_and(|p| !p.attackable || p.immune || p.lifestone_protected))
        {
            return Err(CastRejection::InvalidTarget);
        }
        Ok(())
    }
}
