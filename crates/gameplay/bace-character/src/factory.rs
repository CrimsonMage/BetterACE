//! Official ACE PlayerFactory.Create and SexCG accessors at
//! 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only, ACE contributors.
//! In-memory proposal only: IDs/names are not reserved and nothing is persisted.
use crate::*;
use bace_content::{Attribute, Position, Property, SecondaryAttribute, Skill, WeenieV1};
use bace_gameplay_api::SkillAdvancement;

fn set<T>(properties: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(property) = properties.iter_mut().find(|p| p.id == id) {
        property.value = value;
    } else {
        properties.push(Property { id, value });
        properties.sort_by_key(|p| p.id);
    }
}
fn integer(state: &WeenieV1, id: u32) -> Option<i32> {
    state
        .properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}
fn palette(palettes: &[u32], hue: f64) -> Result<u32, FactoryError> {
    if !hue.is_finite() || !(0.0..=1.0).contains(&hue) {
        return Err(FactoryError::InvalidHue);
    }
    if palettes.is_empty() || palettes.len() > 4096 {
        return Err(FactoryError::MissingAsset);
    }
    let index = ((palettes.len() as f64 - 0.000001) * hue) as usize;
    let id = palettes[index.min(palettes.len() - 1)];
    if id >> 24 != 4 {
        return Err(FactoryError::InvalidContent);
    }
    Ok(id)
}
fn position(position: &Position) -> Result<(), FactoryError> {
    let values = [
        position.position_x,
        position.position_y,
        position.position_z,
        position.rotation_w,
        position.rotation_x,
        position.rotation_y,
        position.rotation_z,
    ];
    if position.obj_cell_id == 0 || values.iter().any(|n| !n.is_finite()) {
        return Err(FactoryError::InvalidPosition);
    }
    let norm = position.rotation_w.powi(2)
        + position.rotation_x.powi(2)
        + position.rotation_y.powi(2)
        + position.rotation_z.powi(2);
    if (norm - 1.0).abs() > 0.01 {
        return Err(FactoryError::InvalidPosition);
    }
    Ok(())
}
fn stack(state: &mut WeenieV1, count: i32) -> Result<(), FactoryError> {
    let burden = integer(state, 13)
        .unwrap_or(0)
        .checked_mul(count)
        .ok_or(FactoryError::Overflow)?;
    let value = integer(state, 15)
        .unwrap_or(0)
        .checked_mul(count)
        .ok_or(FactoryError::Overflow)?;
    set(&mut state.properties.ints, 12, count);
    set(&mut state.properties.ints, 5, burden);
    set(&mut state.properties.ints, 19, value);
    Ok(())
}
fn selected<T>(choices: &[T], index: u32) -> Result<&T, FactoryError> {
    choices
        .get(index as usize)
        .ok_or(FactoryError::InvalidSelection)
}

/// Prepare supported humanoid character creation, without I/O or client privilege
/// flags. All required assets must be present; missing starting gear is an error,
/// never an IOU or a partially successful character. Caller must admit geometry
/// and supply DateOfBirth (PropertyString 43) from explicit server creation time,
/// then commit identity/player/items together before announcing success. This
/// proposal does not read a clock or claim full Player constructor parity.
pub fn prepare_character(
    request: &CharacterCreateRequest,
    assets: &PreparedCreationAssets,
    name: &NameApproval,
    item_ids: &[u32],
) -> Result<PreparedCharacter, FactoryError> {
    if request.account == 0
        || request.entity == 0
        || request.name.is_empty()
        || request.name.len() > 100
        || request.name.chars().any(char::is_control)
    {
        return Err(FactoryError::Identity);
    }
    if !name.matches(&request.name) {
        return Err(FactoryError::NameNotApproved);
    }
    if !(1..=11).contains(&request.heritage) || request.heritage != assets.heritage {
        return Err(FactoryError::UnsupportedHeritage);
    }
    if assets.genders.len() > 8
        || assets.templates.len() > 256
        || assets.starts.len() > 256
        || assets.items.len() > 4096
        || assets.skill_gear.len() > 4096
        || assets.skill_spells.len() > 4096
        || item_ids.len() > 1024
    {
        return Err(FactoryError::Capacity);
    }
    if assets.human.schema_version != 1
        || assets.human.weenie_type != 10
        || assets.human_revision == 0
    {
        return Err(FactoryError::InvalidContent);
    }
    assets
        .human
        .validate(Default::default())
        .map_err(|_| FactoryError::InvalidContent)?;
    for (index, id) in item_ids.iter().enumerate() {
        if *id == 0 || *id == request.entity || item_ids[..index].contains(id) {
            return Err(FactoryError::DuplicateId);
        }
    }
    let allocation = assets
        .rules
        .validate(&request.allocation)
        .map_err(FactoryError::Allocation)?;
    let gender = assets
        .genders
        .iter()
        .find(|g| g.gender == request.gender)
        .ok_or(FactoryError::InvalidSelection)?;
    if gender.gender != 1 && gender.gender != 2 {
        return Err(FactoryError::InvalidSelection);
    }
    let template = selected(&assets.templates, request.template_index)?;
    if template.title > i32::MAX as u32 {
        return Err(FactoryError::InvalidContent);
    }
    let start = assets
        .starts
        .iter()
        .find(|s| s.area == request.start_area)
        .ok_or(FactoryError::InvalidSelection)?;
    position(&start.location)?;
    position(&start.instantiation)?;
    let appearance = &request.appearance;
    let hair = selected(&gender.hair, appearance.hair_style)?;
    let eyes = selected(&gender.eyes, appearance.eyes)?;
    let eyes = if hair.bald { eyes.bald } else { eyes.normal };
    let nose = selected(&gender.noses, appearance.nose)?;
    let mouth = selected(&gender.mouths, appearance.mouth)?;
    let skin_palette = palette(&gender.skin_palettes, appearance.skin_hue)?;
    let hair_palette = palette(
        selected(&gender.hair_palette_sets, appearance.hair_color)?,
        appearance.hair_hue,
    )?;
    let eye_palette = *selected(&gender.eye_palettes, appearance.eye_color)?;
    if eye_palette >> 24 != 4 || gender.scale == 0 {
        return Err(FactoryError::InvalidContent);
    }
    let mut state = assets.human.clone();
    let p = &mut state.properties;
    // Player(Weenie, ...) persistent constructor defaults at the ACE pin.
    // A human's create-list is not an additional source of starter possessions.
    for id in [1, 2] {
        if !p.int64s.iter().any(|property| property.id == id) {
            set(&mut p.int64s, id, 0);
        }
    }
    set(&mut p.bools, 19, true);
    p.create_list.clear();
    // Never copy a template's birth date. The lifecycle adapter must supply
    // PropertyString.DateOfBirth from its explicit server creation time before
    // durable creation; the pure domain must not read a wall clock.
    p.strings.retain(|property| property.id != 43);
    for (id, value) in [
        (188, request.heritage as i32),
        (113, request.gender as i32),
        (23, allocation.total_skill_credits as i32),
        (24, allocation.available_skill_credits as i32),
        (261, template.title as i32),
    ] {
        set(&mut p.ints, id, value);
    }
    for (id, value) in [
        (1, name.normalized_name().to_owned()),
        (3, if gender.gender == 1 { "Male" } else { "Female" }.into()),
        (4, assets.heritage_name.clone()),
        (5, template.name.clone()),
    ] {
        set(&mut p.strings, id, value);
    }
    let setup = if hair.alternate_setup != 0 {
        hair.alternate_setup
    } else {
        gender.setup
    };
    for (id, value) in [
        (1, setup),
        (2, gender.motion),
        (3, gender.sound),
        (4, gender.combat),
        (6, gender.palette_base),
        (22, gender.physics),
        (9, eyes.new),
        (12, eyes.old),
        (10, nose.new),
        (13, nose.old),
        (11, mouth.new),
        (14, mouth.old),
        (17, skin_palette),
        (15, hair_palette),
        (16, eye_palette),
    ] {
        if value == 0 {
            return Err(FactoryError::MissingAsset);
        }
        set(&mut p.data_ids, id, value);
    }
    if let Some(head) = hair.head_object {
        set(&mut p.data_ids, 18, head);
    }
    if hair.multiple_parts {
        set(&mut p.ints, 9012, appearance.hair_style as i32);
    }
    if gender.scale != 100 {
        set(&mut p.floats, 39, f64::from(gender.scale as f32 / 100.0));
    }
    set(&mut p.floats, 12, appearance.skin_hue);
    let wire_attrs = allocation.attributes.wire_order();
    let attrs = [
        wire_attrs[0],
        wire_attrs[1],
        wire_attrs[3],
        wire_attrs[2],
        wire_attrs[4],
        wire_attrs[5],
    ];
    for (index, value) in attrs.iter().enumerate() {
        let id = index as u32 + 1;
        let old = p
            .attributes
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.value.clone())
            .unwrap_or_default();
        set(
            &mut p.attributes,
            id,
            Attribute {
                init_level: *value,
                ..old
            },
        );
    }
    for (index, formula) in assets.vital_formulas.iter().enumerate() {
        let id = index as u32 * 2 + 1;
        let old = p
            .secondary_attributes
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.value.clone())
            .unwrap_or_default();
        let bonus = if !formula.enabled {
            0
        } else {
            if formula.divisor == 0
                || !(1..=6).contains(&formula.attribute1)
                || formula.attribute2 > 6
            {
                return Err(FactoryError::InvalidContent);
            }
            let mut value = attrs[formula.attribute1 as usize - 1];
            if formula.attribute2 != 0 {
                value = value
                    .checked_add(attrs[formula.attribute2 as usize - 1])
                    .ok_or(FactoryError::Overflow)?;
            }
            (value as f32 / formula.divisor as f32).round() as u32
        };
        let current = old
            .init_level
            .checked_add(old.level_from_cp)
            .and_then(|v| v.checked_add(bonus))
            .ok_or(FactoryError::Overflow)?;
        set(
            &mut p.secondary_attributes,
            id,
            SecondaryAttribute {
                current_level: current,
                ..old
            },
        );
    }
    for (index, skill) in allocation.skills.iter().enumerate() {
        if let Some(skill) = skill {
            let id = index as i32;
            let old = p
                .skills
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.value.clone())
                .unwrap_or_default();
            let value = Skill {
                level_from_pp: skill.ranks,
                sac: skill.advancement as u32,
                pp: skill.experience_spent,
                init_level: skill.initial_level,
                ..old
            };
            if let Some(old) = p.skills.iter_mut().find(|s| s.id == id) {
                old.value = value;
            } else {
                p.skills.push(Property { id, value });
            }
        }
    }
    p.skills.sort_by_key(|s| s.id);
    for (id, value) in [
        (1, start.location.clone()),
        (3, start.instantiation.clone()),
        (4, start.location.clone()),
    ] {
        set(&mut p.positions, id, value);
    }
    set(&mut p.bools, 107, true);
    let (melee, ranged, augmentation) = match request.heritage {
        1 => (6, 8, 326),
        2 => (7, 12, 326),
        3 => (1, 8, 326),
        4 => (2, 9, 326),
        5 | 10 => (1, 9, 298),
        6 => (4, 9, 310),
        7 => (5, 10, 299),
        8 => (3, 10, 230),
        9 => (2, 12, 296),
        11 => (3, 10, 233),
        _ => unreachable!("heritage checked"),
    };
    set(&mut p.ints, 354, melee);
    set(&mut p.ints, 355, ranged);
    set(&mut p.ints, augmentation, 1);
    let mut possessions = Vec::new();
    // Gear Knights use their body style rather than starting clothing.
    if request.heritage != 6 {
        for (slot, selection) in appearance.clothing.iter().enumerate() {
            if slot == 0 && selection.style == u32::MAX {
                continue;
            }
            if !selection.hue.is_finite() || !(0.0..=1.0).contains(&selection.hue) {
                return Err(FactoryError::InvalidHue);
            }
            if !gender.clothing_colors.contains(&selection.color)
                || selection.color > i32::MAX as u32
            {
                return Err(FactoryError::InvalidSelection);
            }
            let id = *selected(&gender.clothing[slot], selection.style)?;
            let mut item = possession(assets, id, request.entity, item_ids, &possessions)?;
            if item.state.weenie_type != 2 {
                return Err(FactoryError::InvalidContent);
            }
            let icon = assets
                .items
                .iter()
                .find(|i| i.template.weenie_id == id)
                .and_then(|i| {
                    i.clothing_icons
                        .iter()
                        .find(|(color, _)| *color == selection.color)
                })
                .map(|(_, icon)| *icon)
                .ok_or(FactoryError::MissingAsset)?;
            set(&mut item.state.properties.data_ids, 8, icon);
            set(&mut item.state.properties.ints, 3, selection.color as i32);
            set(&mut item.state.properties.floats, 12, selection.hue);
            item.equipped = integer(&item.state, 9).ok_or(FactoryError::InvalidContent)? as u32;
            if item.equipped == 0
                || possessions.iter().any(|i: &PreparedPossession| {
                    // ACE Creature_Equipment.GetEquippedItems selects clothing
                    // by ClothingPriority coverage, not ValidLocations.
                    integer(&i.state, 4).is_some_and(|existing| {
                        existing as u32 & integer(&item.state, 4).unwrap_or(0) as u32 != 0
                    })
                })
            {
                return Err(FactoryError::InvalidContent);
            }
            set(&mut item.state.properties.ints, 10, item.equipped as i32);
            item.state.properties.instance_ids.retain(|p| p.id != 2);
            set(&mut item.state.properties.instance_ids, 3, request.entity);
            possessions.push(item);
        }
    }
    for gear in &assets.skill_gear {
        if gear.heritage.is_some_and(|h| h != request.heritage) {
            continue;
        }
        let Some(Some(skill)) = allocation.skills.get(gear.skill as usize) else {
            continue;
        };
        if !matches!(
            skill.advancement,
            SkillAdvancement::Trained | SkillAdvancement::Specialized
        ) {
            continue;
        }
        if gear.count == 0 || gear.count > i32::MAX as u32 {
            return Err(FactoryError::InvalidContent);
        }
        if let Some(item) = possessions
            .iter_mut()
            .find(|i| i.equipped == 0 && i.state.weenie_id == gear.template)
        {
            if integer(&item.state, 11).unwrap_or(1) > 1 {
                let total = integer(&item.state, 12)
                    .unwrap_or(1)
                    .checked_add(gear.count as i32)
                    .ok_or(FactoryError::Overflow)?;
                stack(&mut item.state, total)?;
            }
            continue;
        }
        let mut item = possession(
            assets,
            gear.template,
            request.entity,
            item_ids,
            &possessions,
        )?;
        if let (Some(_), Some(max)) = (integer(&item.state, 12), integer(&item.state, 11)) {
            if max <= 0 {
                return Err(FactoryError::InvalidContent);
            }
            stack(&mut item.state, (gear.count as i32).min(max))?;
        }
        let melee_weapon = item.state.weenie_type == 6;
        possessions.push(item);
        // Skill ID 49 is DualWield in the pinned skill catalog.
        if melee_weapon
            && allocation.skills[49].is_some_and(|s| {
                matches!(
                    s.advancement,
                    SkillAdvancement::Trained | SkillAdvancement::Specialized
                )
            })
        {
            possessions.push(possession(
                assets,
                gear.template,
                request.entity,
                item_ids,
                &possessions,
            )?);
        }
    }
    for spell in &assets.skill_spells {
        let Some(Some(skill)) = allocation.skills.get(spell.skill as usize) else {
            continue;
        };
        if skill.advancement == SkillAdvancement::Specialized
            || (skill.advancement == SkillAdvancement::Trained && !spell.specialized_only)
        {
            if spell.spell == 0 || spell.spell > i32::MAX as u32 {
                return Err(FactoryError::InvalidContent);
            }
            if !state
                .properties
                .spell_book
                .iter()
                .any(|s| s.id == spell.spell as i32)
            {
                state.properties.spell_book.push(Property {
                    id: spell.spell as i32,
                    value: 2.0,
                });
            }
        }
    }
    state
        .validate(Default::default())
        .map_err(|_| FactoryError::InvalidContent)?;
    Ok(PreparedCharacter {
        account: request.account,
        entity: request.entity,
        name: name.normalized_name().to_owned(),
        template_revision: assets.human_revision,
        state,
        metadata: CharacterMetadata {
            hair_texture: hair.texture.map_or(0, |t| t.new),
            default_hair_texture: hair.texture.map_or(0, |t| t.old),
            titles: vec![template.title],
            current_title: template.title,
            options1: 1_355_064_650,
            options2: 9_733_888 | 0x02000000,
        },
        possessions,
    })
}
fn possession(
    assets: &PreparedCreationAssets,
    template: u32,
    owner: u32,
    ids: &[u32],
    existing: &[PreparedPossession],
) -> Result<PreparedPossession, FactoryError> {
    if existing.len() >= 1024 {
        return Err(FactoryError::Capacity);
    }
    let entity = *ids
        .get(existing.len())
        .ok_or(FactoryError::InsufficientIds)?;
    let item = assets
        .items
        .iter()
        .find(|i| i.template.weenie_id == template)
        .ok_or(FactoryError::MissingAsset)?;
    if item.revision == 0 {
        return Err(FactoryError::InvalidContent);
    }
    let mut state = item.template.clone();
    state
        .validate(Default::default())
        .map_err(|_| FactoryError::InvalidContent)?;
    state.properties.positions.clear();
    state
        .properties
        .instance_ids
        .retain(|p| p.id != 2 && p.id != 3);
    set(&mut state.properties.instance_ids, 2, owner);
    Ok(PreparedPossession {
        entity,
        template_revision: item.revision,
        equipped: 0,
        state,
    })
}
