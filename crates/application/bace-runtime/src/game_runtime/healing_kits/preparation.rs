//! Immutable ACE Healer properties. The mutable charge and current Value come
//! from the accepted item; StructureUnitValue comes from the cached template.
use bace_content::WeenieV1;
use bace_entity::EntityVital;
use bace_simulation::PreparedHealingKit;

pub(super) fn profile(item: &WeenieV1, template: &WeenieV1) -> Result<PreparedHealingKit, String> {
    if item.weenie_type != 28 || template.weenie_type != 28 || item.weenie_id != template.weenie_id
    {
        return Err("healing kit source identity".into());
    }
    let int = |source: &WeenieV1, id| {
        source
            .properties
            .ints
            .iter()
            .find(|property| property.id == id)
            .map(|property| property.value)
    };
    let vital = match int(item, 89) {
        Some(1) => EntityVital::Health,
        Some(3) => EntityVital::Stamina,
        Some(5) => EntityVital::Mana,
        _ => return Err("unsupported healing kit BoosterEnum".into()),
    };
    let healkit_mod = item
        .properties
        .floats
        .iter()
        .find(|property| property.id == 100)
        .map(|property| property.value as f32)
        .ok_or("healing kit HealkitMod missing")?;
    if !healkit_mod.is_finite() || !(0.0..=1000.0).contains(&healkit_mod) {
        return Err("healing kit HealkitMod invalid".into());
    }
    let unlimited_use = item
        .properties
        .bools
        .iter()
        .any(|property| property.id == 63 && property.value);
    let max_structure = int(template, 91)
        .map(|value| u32::try_from(value).map_err(|_| "negative template MaxStructure"))
        .transpose()?
        .or(Some(1));
    // WorldObject.StructureUnitValue clamps a negative cached-weenie quotient.
    let template_value = Some(int(template, 19).unwrap_or(0).max(0) as u32);
    if !unlimited_use && (max_structure == Some(0) || int(item, 92).is_none_or(|value| value <= 0))
    {
        return Err("depleted healing kit Structure".into());
    }
    Ok(PreparedHealingKit {
        vital,
        boost_value: int(item, 90).unwrap_or(0),
        healkit_mod,
        unlimited_use,
        max_structure,
        template_value,
        activation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::Property;

    #[test]
    fn ace_healer_source_properties_are_distinct_from_mutable_charges() {
        // ACE Healer.UsesLeft is PropertyInt.Structure (92), while
        // WorldObject.StructureUnitValue reads cached weenie Value (19) and
        // MaxStructure (91), so the latter must not come from the spent item.
        let mut template = WeenieV1 {
            schema_version: 1,
            weenie_id: 123,
            class_name: "kit".into(),
            weenie_type: 28,
            last_modified: None,
            properties: Default::default(),
        };
        template.properties.ints.extend([
            Property { id: 19, value: 100 },
            Property { id: 91, value: 3 },
        ]);
        let mut item = template.clone();
        item.properties.ints.extend([
            Property { id: 89, value: 1 },
            Property { id: 90, value: 20 },
            Property { id: 92, value: 1 },
        ]);
        item.properties
            .ints
            .iter_mut()
            .find(|p| p.id == 19)
            .unwrap()
            .value = 34;
        item.properties.floats.push(Property {
            id: 100,
            value: 1.3,
        });
        let p = profile(&item, &template).unwrap();
        assert_eq!(p.vital, EntityVital::Health);
        assert_eq!(p.boost_value, 20);
        assert_eq!(p.max_structure, Some(3));
        assert_eq!(p.template_value, Some(100));
        assert!(!p.unlimited_use);
        assert!(p.activation.is_none());
    }
}
