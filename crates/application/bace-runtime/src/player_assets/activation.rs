//! Cold source-property decoding only; the simulation evaluates these predicates
//! against accepted character qualities before equipment admission.
use bace_content::WeenieV1;
use bace_inventory::{ActivationObject, ActivationRequirements};
pub fn prepare_item_activation_requirements(
    item: &WeenieV1,
) -> Result<ActivationRequirements, String> {
    let int = |key| {
        item.properties
            .ints
            .iter()
            .find(|p| p.id == key)
            .map(|p| p.value)
    };
    let did = |key| {
        item.properties
            .data_ids
            .iter()
            .find(|p| p.id == key)
            .map(|p| p.value as i32)
    };
    // Constructor subtype follows WorldObjectFactory, not ItemType or a guessed
    // numeric range. AI (16), for example, constructs GenericObject in ACE.
    let object = match item.weenie_type {
        0 => return Err("undefined source activation subtype".into()),
        10 | 12 | 15 | 61 | 69 | 71 => ActivationObject::Creature {
            olthoi: int(2) == Some(1),
            vendor: item.weenie_type == 12,
            looks_like_object: item.properties.bools.iter().any(|p| p.id == 83 && p.value),
        },
        25 => ActivationObject::Lifestone,
        8 | 20 | 21 | 22 | 38 | 55 | 56 | 57 | 60 | 62 | 63 | 65 | 67 => {
            ActivationObject::RestrictedOlthoi
        }
        2..=7
        | 9
        | 13
        | 14
        | 18
        | 19
        | 23
        | 24
        | 26..=29
        | 30
        | 32..=35
        | 37
        | 39
        | 40
        | 44
        | 51
        | 53
        | 59
        | 64
        | 70 => ActivationObject::Ordinary,
        _ => ActivationObject::RestrictedOlthoi,
    };
    Ok(ActivationRequirements {
        difficulty: int(109),
        item_skill: did(37),
        item_skill_level: int(115),
        skill: int(366),
        skill_level: int(367),
        specialized: int(368),
        item_specialized: did(41),
        level: int(369),
        attribute: int(257),
        attribute_level: int(258),
        vital: int(259),
        vital_level: int(260),
        heritage: int(188).unwrap_or(0) as u32,
        cooldown: int(280),
        object,
    })
}
