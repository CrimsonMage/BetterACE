//! ACE RecipeManager.ShowDialog/BroadcastTinkering and NameWithMaterial.
use crate::{CraftContext, CraftError, CraftItem, PropertyKey, PropertyKind, PropertyValue};
use std::collections::BTreeMap;
fn string(props: &BTreeMap<PropertyKey, PropertyValue>, id: u32) -> Option<&str> {
    match props.get(&PropertyKey {
        kind: PropertyKind::String,
        id,
    }) {
        Some(PropertyValue::String(v)) => Some(v),
        _ => None,
    }
}
fn name(item: &CraftItem, materials: &BTreeMap<u32, String>) -> Result<String, CraftError> {
    let name = string(&item.properties, 1).ok_or(CraftError::InvalidState)?;
    let Some(PropertyValue::Int(material)) = item.properties.get(&PropertyKey {
        kind: PropertyKind::Int,
        id: 131,
    }) else {
        return Ok(name.to_owned());
    };
    let material = materials
        .get(&(*material as u32))
        .ok_or(CraftError::InvalidState)?;
    Ok(format!("{material} {}", name.replace(material, "")))
}
pub fn tinker_confirmation_text(probability: f64, augmented: bool) -> Result<String, CraftError> {
    if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
        return Err(CraftError::InvalidState);
    }
    let mut text = format!(
        "You determine that you have a {} percent chance to succeed.",
        (probability * 100.0).round()
    );
    if augmented {
        text.push_str("\n5 percent is due to your augmentation.")
    }
    Ok(text)
}
pub fn tinker_result_text(
    context: &CraftContext,
    source: &CraftItem,
    target: &CraftItem,
    materials: &BTreeMap<u32, String>,
    success: bool,
) -> Result<String, CraftError> {
    let player = string(&context.properties, 1).ok_or(CraftError::InvalidState)?;
    let mut source_name = name(source, materials)?;
    if source_name.ends_with(')')
        && let Some(start) = source_name.rfind(" (")
        && source_name[start + 2..source_name.len() - 1]
            .bytes()
            .all(|b| b.is_ascii_digit())
        && start + 3 < source_name.len()
    {
        source_name.truncate(start);
    }
    let target_name = name(target, materials)?;
    let inscription = match (string(&target.properties, 7), string(&target.properties, 8)) {
        (Some(_), Some(scribe)) => format!(" inscribed by {scribe}"),
        _ => String::new(),
    };
    // .NET custom Single formatting uses seven significant decimal digits
    // before applying the #.00 format. Workmanship is admitted in [1,10].
    let decimal = (f64::from(context.chance.tool_workmanship) * 1_000_000.0).round() / 1_000_000.0;
    let display = (decimal * 100.0).round() / 100.0;
    Ok(format!(
        "{player} {} the {source_name} (workmanship {:.2}) to the {target_name}{inscription}.{}",
        if success {
            "successfully applies"
        } else {
            "fails to apply"
        },
        display,
        if success {
            ""
        } else {
            " The target is destroyed."
        }
    ))
}
