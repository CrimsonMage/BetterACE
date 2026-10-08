//! Cold ACE relational recipe preparation. Row order is preserved within each
//! source collection; property families execute in RecipeManager.ModifyItem order.
use crate::{
    CraftError, Mutation, MutationKind, Participant, PreparedRecipe, PropertyKey, PropertyKind,
    PropertyValue, RecipeBranch, Requirement, RequirementComparison,
};
use bace_content::*;

/// Exact joined rows for one recipe, without unrelated catalog rows.
#[derive(Default)]
pub struct NativeRecipeRows<'a> {
    pub mods: &'a [RecipeModRowV1],
    pub bool_mods: &'a [RecipeModsBoolRowV1],
    pub int_mods: &'a [RecipeModsIntRowV1],
    pub float_mods: &'a [RecipeModsFloatRowV1],
    pub string_mods: &'a [RecipeModsStringRowV1],
    pub iid_mods: &'a [RecipeModsIIDRowV1],
    pub did_mods: &'a [RecipeModsDIDRowV1],
    pub bool_requirements: &'a [RecipeRequirementsBoolRowV1],
    pub int_requirements: &'a [RecipeRequirementsIntRowV1],
    pub float_requirements: &'a [RecipeRequirementsFloatRowV1],
    pub string_requirements: &'a [RecipeRequirementsStringRowV1],
    pub iid_requirements: &'a [RecipeRequirementsIIDRowV1],
    pub did_requirements: &'a [RecipeRequirementsDIDRowV1],
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeRecipe {
    /// Source skill/difficulty/salvage classification and branch messages retained.
    pub source: RecipeRowV1,
    pub recipe: PreparedRecipe,
    /// Parallel to recipe.requirements, preserving the first failing message.
    pub requirement_messages: Vec<Option<String>>,
}

/// Result creation and vital deltas need their own aggregate owners. This path
/// rejects them explicitly instead of returning a recipe that silently omits them.
pub fn prepare_native_recipe(
    source: &RecipeRowV1,
    revision: u64,
    rows: NativeRecipeRows<'_>,
) -> Result<NativeRecipe, CraftError> {
    if source.id == 0 || revision == 0 {
        return Err(CraftError::InvalidState);
    }
    if source.success_w_c_i_d != 0
        || source.fail_w_c_i_d != 0
        || rows
            .mods
            .iter()
            .any(|m| m.health != 0 || m.stamina != 0 || m.mana != 0)
    {
        return Err(CraftError::Unsupported);
    }
    let chances = [
        source.success_destroy_source_chance,
        source.success_destroy_target_chance,
        source.fail_destroy_source_chance,
        source.fail_destroy_target_chance,
    ];
    if chances
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(CraftError::InvalidState);
    }
    if rows.mods.len() > 256 {
        return Err(CraftError::Capacity);
    }
    let mut ids = std::collections::BTreeSet::new();
    for m in rows.mods {
        if m.recipe_id != source.id || m.id == 0 || !ids.insert(m.id) {
            return Err(CraftError::InvalidState);
        }
    }
    macro_rules! validate_mods {($($field:ident),*)=>{$(if rows.$field.len()>256{return Err(CraftError::Capacity)}
        let mut seen=std::collections::BTreeSet::new();for row in rows.$field {if !ids.contains(&row.recipe_mod_id)||!seen.insert(row.id){return Err(CraftError::InvalidState)}})*}}
    validate_mods!(
        bool_mods,
        int_mods,
        float_mods,
        string_mods,
        iid_mods,
        did_mods
    );
    let branch = |success| RecipeBranch {
        consume_source: if success {
            source.success_destroy_source_amount
        } else {
            source.fail_destroy_source_amount
        },
        consume_target: if success {
            source.success_destroy_target_amount
        } else {
            source.fail_destroy_target_amount
        },
        destroy_source_chance: if success { chances[0] } else { chances[2] },
        destroy_target_chance: if success { chances[1] } else { chances[3] },
        mutations: Vec::new(),
    };
    if rows
        .string_requirements
        .iter()
        .any(|r| r.value.is_none() && !matches!(r.r#enum, 7 | 8))
    {
        return Err(CraftError::Unsupported);
    }
    let mut recipe = PreparedRecipe {
        id: source.id,
        revision,
        requirements: Vec::new(),
        success: branch(true),
        failure: branch(false),
        increment_tinker_count: false,
        proficiency: (source.skill > 0 && source.difficulty > 0)
            .then_some((source.skill, source.difficulty)),
    };
    let mut messages = Vec::new();
    macro_rules! requirements {
        ($index:expr,$field:ident,$variant:ident,$convert:expr) => {{
            if rows.$field.len() > 256 {
                return Err(CraftError::Capacity);
            }
            let mut seen = std::collections::BTreeSet::new();
            for row in rows.$field {
                if row.recipe_id != source.id
                    || !(0..=2).contains(&row.index)
                    || !seen.insert(row.id)
                {
                    return Err(CraftError::InvalidState);
                }
            }
            for row in rows.$field.iter().filter(|r| r.index == $index) {
                let value = PropertyValue::$variant(($convert)(&row.value));
                let req = prepare_requirement(row.index, row.stat, row.r#enum, value)?;
                validate_text(row.message.as_deref())?;
                recipe.requirements.push(req);
                messages.push(row.message.clone());
            }
        }};
    }
    for index in 0..=2 {
        requirements!(index, bool_requirements, Bool, |v: &bool| *v);
        requirements!(index, int_requirements, Int, |v: &i32| *v);
        requirements!(index, float_requirements, Float, |v: &f64| *v);
        requirements!(index, string_requirements, String, |v: &Option<String>| v
            .clone()
            .unwrap_or_default());
        requirements!(index, iid_requirements, InstanceId, |v: &u32| *v);
        requirements!(index, did_requirements, DataId, |v: &u32| *v);
    }
    for m in rows.mods {
        let branch = if m.executes_on_success {
            &mut recipe.success
        } else {
            &mut recipe.failure
        };
        macro_rules! mods {
            ($field:ident,$variant:ident,$convert:expr) => {
                for row in rows.$field.iter().filter(|r| r.recipe_mod_id == m.id) {
                    let value = PropertyValue::$variant(($convert)(&row.value));
                    branch.mutations.push(prepare_mod(
                        row.index, row.stat, row.r#enum, row.source, value,
                    )?);
                }
            };
        }
        mods!(bool_mods, Bool, |v: &bool| *v);
        mods!(int_mods, Int, |v: &i32| *v);
        mods!(float_mods, Float, |v: &f64| *v);
        for row in rows.string_mods.iter().filter(|r| r.recipe_mod_id == m.id) {
            let mut mutation = prepare_mod(
                row.index,
                row.stat,
                row.r#enum,
                row.source,
                PropertyValue::String(row.value.clone().unwrap_or_default()),
            )?;
            if row.r#enum == 1 && row.value.is_none() {
                mutation.kind = MutationKind::Remove;
            }
            branch.mutations.push(mutation);
        }
        mods!(iid_mods, InstanceId, |v: &u32| *v);
        mods!(did_mods, DataId, |v: &u32| *v);
        if m.data_id != 0 {
            let id = m.data_id as u32;
            if !crate::recipe_script_supported(id) {
                return Err(CraftError::Unsupported);
            }
            branch.mutations.push(Mutation {
                participant: Participant::Target,
                key: PropertyKey {
                    kind: PropertyKind::Int,
                    id: 171,
                },
                kind: MutationKind::Script(id),
            });
        }
    }
    for text in [
        &source.success_message,
        &source.fail_message,
        &source.success_destroy_source_message,
        &source.success_destroy_target_message,
        &source.fail_destroy_source_message,
        &source.fail_destroy_target_message,
    ] {
        validate_text(text.as_deref())?
    }
    let retained_text = messages
        .iter()
        .filter_map(|s| s.as_ref())
        .map(String::len)
        .sum::<usize>()
        + [
            &source.success_message,
            &source.fail_message,
            &source.success_destroy_source_message,
            &source.success_destroy_target_message,
            &source.fail_destroy_source_message,
            &source.fail_destroy_target_message,
        ]
        .iter()
        .filter_map(|s| s.as_ref())
        .map(String::len)
        .sum::<usize>();
    if retained_text > 65536 || source.last_modified.len() > 256 {
        return Err(CraftError::Capacity);
    }
    recipe.validate_bounds()?;
    Ok(NativeRecipe {
        source: source.clone(),
        recipe,
        requirement_messages: messages,
    })
}
fn validate_text(value: Option<&str>) -> Result<(), CraftError> {
    if value.is_some_and(|v| v.len() > 4096) {
        Err(CraftError::Capacity)
    } else {
        Ok(())
    }
}
fn participant(index: i8) -> Result<Participant, CraftError> {
    Ok(match index {
        0 | 4 | 3 | 7 => Participant::Target,
        1 | 5 => Participant::Source,
        2 | 6 => Participant::Actor,
        _ => return Err(CraftError::InvalidState),
    })
}
fn prepare_requirement(
    index: i8,
    stat: i32,
    operation: i32,
    value: PropertyValue,
) -> Result<Requirement, CraftError> {
    use RequirementComparison as C;
    if !(1..=65535).contains(&stat) || !value.valid() {
        return Err(CraftError::InvalidState);
    }
    let comparison = match operation {
        0 => C::AtMost,
        1 => C::Greater,
        2 => C::AtLeast,
        3 => C::Less,
        4 | 5 => C::Equal,
        6 => C::NotEqual,
        7 => C::Present,
        8 => C::Absent,
        9 => C::HasAnyBits,
        10 => C::DoesNotHaveAllBits,
        _ => return Err(CraftError::Unsupported),
    };
    if matches!(value, PropertyValue::String(_)) && !(4..=8).contains(&operation) {
        return Err(CraftError::Unsupported);
    }
    Ok(Requirement {
        participant: participant(index)?,
        key: PropertyKey {
            kind: value.kind(),
            id: stat as u32,
        },
        comparison,
        value,
        absent_is_default: operation != 5,
    })
}
fn prepare_mod(
    index: i8,
    stat: i32,
    operation: i32,
    source: i32,
    value: PropertyValue,
) -> Result<Mutation, CraftError> {
    if !(1..=65535).contains(&stat) || !value.valid() {
        return Err(CraftError::InvalidState);
    }
    let mut participant = participant(index)?;
    let mut key = PropertyKey {
        kind: value.kind(),
        id: stat as u32,
    };
    let kind = match operation {
        1 => MutationKind::Set(value),
        2 if matches!(value, PropertyValue::Int(_) | PropertyValue::Float(_)) => {
            MutationKind::Add(value)
        }
        3 if !matches!(value, PropertyValue::Bool(_)) => {
            participant = Participant::Target;
            let source = match source {
                0 => Participant::Actor,
                1 => Participant::Source,
                _ => return Err(CraftError::Unsupported),
            };
            if key.kind == PropertyKind::InstanceId && matches!(key.id, 31 | 38) {
                MutationKind::CopyIdentity(source)
            } else {
                MutationKind::Copy {
                    participant: source,
                    key,
                }
            }
        }
        7 if matches!(value, PropertyValue::Int(_)) => {
            key.kind = PropertyKind::SpellBook;
            MutationKind::Set(PropertyValue::SpellBook(true))
        }
        8 => match value {
            PropertyValue::Int(v) => MutationKind::SetBitsOn(v),
            _ => return Err(CraftError::Unsupported),
        },
        9 => match value {
            PropertyValue::Int(v) => MutationKind::SetBitsOff(v),
            _ => return Err(CraftError::Unsupported),
        },
        _ => return Err(CraftError::Unsupported),
    };
    Ok(Mutation {
        participant,
        key,
        kind,
    })
}

impl NativeRecipe {
    /// First failed source-ordered requirement, preserving the authored text.
    pub fn requirement_failure_message<'a>(
        &'a self,
        context: &crate::CraftContext,
        source: &crate::CraftItem,
        target: &crate::CraftItem,
    ) -> Result<Option<&'a str>, CraftError> {
        for (i, requirement) in self.recipe.requirements.iter().enumerate() {
            let properties = match requirement.participant {
                Participant::Actor => &context.properties,
                Participant::Source => &source.properties,
                Participant::Target => &target.properties,
            };
            match requirement.check(properties) {
                Ok(()) => {}
                Err(CraftError::Requirement) => {
                    return Ok(self
                        .requirement_messages
                        .get(i)
                        .and_then(|s| s.as_deref())
                        .filter(|s| !s.is_empty()));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}
