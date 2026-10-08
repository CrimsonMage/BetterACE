//! Authored combat-pet closure. All pack and DAT reads run on a cold worker.
use crate::{
    generator_preparation::CreatureAdmissionPolicy,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
    visibility_assets::{PreparedVisibilityObject, VisibilitySource},
};
use bace_content::{Property, WeenieV1};
use bace_simulation::{PreparedCombatPet, PreparedPassivePet};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use bace_types::EntityId;
use std::sync::Arc;

pub(super) struct PreparedPetSource {
    pub(super) profile: PreparedPetProfile,
    pub(super) visibility: PreparedVisibilityObject,
}
#[derive(Clone)]
pub(super) enum PreparedPetProfile {
    Combat(Arc<PreparedCombatPet>),
    Passive(Arc<PreparedPassivePet>),
}
impl PreparedPetProfile {
    pub(super) fn cooldown_group(&self) -> Option<u16> {
        match self {
            Self::Combat(profile) => profile.cooldown_group,
            Self::Passive(_) => None,
        }
    }
}

pub(super) fn prepare(
    generation: Arc<PackGeneration>,
    manifest: RegionAssetManifest,
    policy: CreatureAdmissionPolicy,
    device: WeenieV1,
    pet: EntityId,
) -> Result<PreparedPetSource, String> {
    if device.weenie_type != 70 || pet.0 < 0x8000_0000 {
        return Err("pet device/source identity".into());
    }
    let int = |id| {
        device
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    let boolean = |id| {
        device
            .properties
            .bools
            .iter()
            .find(|p| p.id == id)
            .is_some_and(|p| p.value)
    };
    let template =
        u32::try_from(int(266).ok_or("pet class missing")?).map_err(|_| "pet class negative")?;
    if template == 0 {
        return Err("pet class zero".into());
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(template),
        })
        .map_err(|error| error.to_string())?
    else {
        return Err("pet class source absent".into());
    };
    let mut source: WeenieV1 =
        bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
    if source.weenie_id != template || !matches!(source.weenie_type, 69 | 71) {
        return Err("pet class subtype mismatch".into());
    }
    let passive = source.weenie_type == 69;
    if passive {
        // ACE Pet.SetEphemeralValues/Init applies these at construction time.
        set(&mut source.properties.bools, 13, true);
        set(&mut source.properties.bools, 29, true);
        set(&mut source.properties.ints, 133, 1);
        set(&mut source.properties.ints, 16, 0);
    }
    let cooldown_group = int(280)
        .map(u16::try_from)
        .transpose()
        .map_err(|_| "pet cooldown group invalid")?
        .filter(|group| *group != 0);
    let cooldown_seconds = device
        .properties
        .floats
        .iter()
        .find(|p| p.id == 167)
        .map_or(0.0, |p| p.value);
    if cooldown_group.is_some() != (cooldown_seconds > 0.0)
        || !cooldown_seconds.is_finite()
        || cooldown_seconds > 86400.0
        || passive && cooldown_group.is_some()
    {
        return Err("pet cooldown source inconsistent".into());
    }
    if int(366).is_some_and(|skill| skill != 54) || int(368).is_some_and(|skill| skill != 54) {
        return Err("unsupported pet skill requirement".into());
    }
    let required =
        |id| u32::try_from(int(id).unwrap_or(0)).map_err(|_| "negative pet requirement".to_owned());
    let requirements = bace_simulation::PetUseRequirements {
        skill_required: int(366).is_some() || int(368).is_some(),
        skill_level: required(367)?,
        level: required(369)?,
        mastery: required(362)?,
        unlimited: boolean(63)
            || passive
                && !device
                    .properties
                    .ints
                    .iter()
                    .any(|property| property.id == 92),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest)?;
    let physical = assets.prepare_template(&source)?;
    let npc = assets.prepare_creature(&source, &physical, policy)?;
    if npc.requires_equipment {
        return Err("combat-pet equipment construction unsupported".into());
    }
    let profile = if passive {
        PreparedPetProfile::Passive(Arc::new(PreparedPassivePet {
            template,
            shape: npc.geometry.shape,
            capabilities: npc.blueprint.capabilities,
            requirements,
        }))
    } else {
        let lifetime = source
            .properties
            .ints
            .iter()
            .find(|p| p.id == 267)
            .map(|p| p.value)
            .filter(|v| *v > 0)
            .ok_or("combat-pet lifespan missing")?;
        PreparedPetProfile::Combat(Arc::new(PreparedCombatPet {
            template,
            shape: npc.geometry.shape,
            capabilities: npc.blueprint.capabilities,
            combat: npc.blueprint.combat,
            lifetime_seconds: f64::from(lifetime),
            visual_range: npc.blueprint.visual_range,
            requirements,
            cooldown_group,
            cooldown_seconds,
        }))
    };
    let mut visibility = assets.prepare_visibility_sources(vec![VisibilitySource {
        entity: pet,
        incarnation: u64::from(pet.0),
        revision: generation.revision().max(1),
        source: &source,
        equipment: vec![],
        missile_combat: false,
    }])?;
    Ok(PreparedPetSource {
        profile,
        visibility: visibility.pop().ok_or("pet appearance missing")?,
    })
}
fn set<T: Clone>(properties: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(property) = properties.iter_mut().find(|property| property.id == id) {
        property.value = value;
    } else {
        properties.push(Property { id, value });
        properties.sort_by_key(|property| property.id);
    }
}
