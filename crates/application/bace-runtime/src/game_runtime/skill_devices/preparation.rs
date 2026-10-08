//! Pinned SkillAlterationDevice and AugmentationDevice source properties. Raw
//! weapon skill identifiers stay raw until the simulation's current MoA lookup.
use super::*;
use bace_character::SkillWieldRequirement;
use bace_content::WeenieV1;
pub(super) fn device(w: &WeenieV1) -> Result<Option<PreparedSkillDevice>, String> {
    let value = |id| {
        w.properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    match w.weenie_type {
        62 => {
            cooldown_seconds(w)?;
            PreparedSkillDevice::alteration(value(185), value(186))
                .map(Some)
                .map_err(|e| format!("skill device source: {e:?}"))
        }
        67 => {
            let kind = value(215);
            if !(7..=11).contains(&kind) {
                return Ok(None);
            }
            cooldown_seconds(w)?;
            let cost = w
                .properties
                .int64s
                .iter()
                .find(|p| p.id == 3)
                .map_or(0, |p| p.value);
            PreparedSkillDevice::augmentation(kind, cost)
                .map(Some)
                .map_err(|e| format!("augmentation source: {e:?}"))
        }
        _ => Ok(None),
    }
}
pub(super) fn requirements(w: &WeenieV1) -> Result<[Option<SkillWieldRequirement>; 4], String> {
    let value = |id| {
        w.properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    let mut result = [None; 4];
    for (index, (kind, skill, difficulty)) in [
        (158, 159, 160),
        (270, 271, 272),
        (273, 274, 275),
        (276, 277, 278),
    ]
    .into_iter()
    .enumerate()
    {
        let id = u32::try_from(value(skill).unwrap_or(0)).map_err(|_| "negative wield skill")?;
        result[index] = match value(kind).unwrap_or(0) {
            1 => Some(SkillWieldRequirement::CurrentSkill(id)),
            2 => Some(SkillWieldRequirement::RawSkill(id)),
            8 => match value(difficulty) {
                Some(value) => Some(SkillWieldRequirement::Training {
                    skill: id,
                    advancement: match value {
                        0 => bace_gameplay_api::SkillAdvancement::Inactive,
                        1 => bace_gameplay_api::SkillAdvancement::Untrained,
                        2 => bace_gameplay_api::SkillAdvancement::Trained,
                        3 => bace_gameplay_api::SkillAdvancement::Specialized,
                        _ => return Err("invalid source wield training class".into()),
                    },
                }),
                None => None,
            },
            _ => None,
        };
    }
    Ok(result)
}
pub(super) fn prompt(
    device: PreparedSkillDevice,
    name: &str,
    definition: &bace_dat::SkillBase,
    class: bace_gameplay_api::SkillAdvancement,
) -> Result<String, String> {
    let (skill, kind, cost) = match device {
        PreparedSkillDevice::Specialize(s) => (
            s,
            bace_replication::skill_devices::SkillDeviceKind::Specialize,
            0,
        ),
        PreparedSkillDevice::Lower(s) => (
            s,
            bace_replication::skill_devices::SkillDeviceKind::Lower,
            0,
        ),
        PreparedSkillDevice::Augment {
            skill,
            experience_cost,
        } => (
            skill,
            bace_replication::skill_devices::SkillDeviceKind::Augment,
            experience_cost,
        ),
    };
    let price = u32::try_from(
        definition
            .specialized_cost
            .checked_sub(definition.trained_cost)
            .ok_or("specialization price overflow")?,
    )
    .map_err(|_| "negative specialization price")?;
    bace_replication::skill_devices::skill_device_prompt(skill, kind, class, price, name, cost)
        .map_err(|e| format!("device prompt: {e:?}"))
}

// The generic requirement owner is evaluated by the simulation on both Use
// and confirmation. Authored side effects remain gated until they join the
// same valuable operation; cooldowns require durable registry publication.
pub(super) fn cooldown_seconds(w: &WeenieV1) -> Result<Option<f64>, String> {
    let p = &w.properties;
    let group = p
        .ints
        .iter()
        .find(|p| p.id == 280)
        .map(|p| p.value)
        .unwrap_or(0);
    let duration = p
        .floats
        .iter()
        .find(|p| p.id == 167)
        .map(|p| p.value)
        .unwrap_or(0.0);
    if !(0..0x8000).contains(&group)
        || !duration.is_finite()
        || !(0.0..=86400.0).contains(&duration)
        || (group == 0) != (duration == 0.0)
    {
        return Err("skill device authored cooldown group/duration".into());
    }
    if p.ints.iter().any(|p| p.id == 119 && p.value == 0)
        || p.instance_ids.iter().any(|p| p.id == 16 && p.value != 0)
        || !p.emotes.is_empty()
    {
        return Err("skill device requires the generic activation requirement/effect owner".into());
    }
    super::activation_response::talk(w, 4096)?;
    Ok((group != 0).then_some(duration))
}
