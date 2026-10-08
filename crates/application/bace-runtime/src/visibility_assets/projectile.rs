//! Entity-free cold projectile description. Binding an allocated identity is a
//! pure operation; accepted launch pose and observer bindings remain World-owned.
use super::*;
#[derive(Clone, Debug)]
pub struct PreparedProjectileVisibilityTemplate {
    source: Arc<WeenieV1>,
    model: PreparedEntryModel,
    state: EntryObjectState,
    spell: bool,
}
impl PreparedProjectileVisibilityTemplate {
    pub fn prepare(
        source: Arc<WeenieV1>,
        assets: &EntryAppearanceAssets<'_>,
    ) -> Result<Self, String> {
        source
            .validate(Default::default())
            .map_err(|e| e.to_string())?;
        if crate::generator_preparation::is_creature_template(source.weenie_type) {
            return Err("projectile template cannot be a creature".into());
        }
        let model = prepare_item_model(&source, assets)?;
        let state = state(&source, assets, false)?;
        if state.parent.is_some() || !state.children.is_empty() {
            return Err("projectile template has attached live identities".into());
        }
        Ok(Self {
            source,
            model,
            state,
            spell: false,
        })
    }
    /// GDLE SpellProjectile constructor overrides the template's transient
    /// presentation qualities; these are neither saved pose nor new entities.
    pub fn prepare_spell(
        source: Arc<WeenieV1>,
        assets: &EntryAppearanceAssets<'_>,
    ) -> Result<Self, String> {
        let mut source = (*source).clone();
        for id in [1, 19, 24] {
            if let Some(value) = source.properties.bools.iter_mut().find(|p| p.id == id) {
                value.value = true;
            } else {
                source
                    .properties
                    .bools
                    .push(bace_content::Property { id, value: true });
            }
        }
        if let Some(value) = source.properties.ints.iter_mut().find(|p| p.id == 1) {
            value.value = 0;
        } else {
            source
                .properties
                .ints
                .push(bace_content::Property { id: 1, value: 0 });
        }
        let mut prepared = Self::prepare(Arc::new(source), assets)?;
        prepared.state.physics_state = 0x28b48;
        if prepared
            .source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 14 && p.value)
        {
            prepared.state.physics_state |= 0x400;
        }
        prepared.spell = true;
        Ok(prepared)
    }
    pub fn template(&self) -> u32 {
        self.source.weenie_id
    }
    pub fn instantiate(
        &self,
        entity: EntityId,
        incarnation: u64,
        revision: u64,
    ) -> Result<PreparedVisibilityObject, String> {
        if !(0x80000000..=0xfffffffe).contains(&entity.0) || incarnation == 0 || revision == 0 {
            return Err("projectile presentation identity/revision".into());
        }
        let mut description = prepare_entry_object(
            entity.0,
            &self.source,
            self.model.clone(),
            self.state.clone(),
        )?;
        if self.spell {
            description.physics.options.friction = Some(1.0);
            description.physics.options.elasticity = Some(0.0);
            description.physics.options.default_script = Some(0x5a);
            description.physics.options.default_script_intensity = Some(1.0);
        }
        Ok(PreparedVisibilityObject {
            incarnation,
            revision,
            description: Arc::new(description),
            children: Vec::new(),
        })
    }
}
