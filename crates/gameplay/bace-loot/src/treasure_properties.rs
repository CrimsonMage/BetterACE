//! Named property access for source treasure mutations.
use crate::{TreasureError, treasure_tables::en};
use bace_content::{Property, WeenieV1};
pub(crate) fn id(kind: &str, name: &str) -> Result<u32, TreasureError> {
    u32::try_from(en(kind, name)?).map_err(|_| TreasureError::Bounds)
}
pub(crate) fn set<T>(v: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(p) = v.iter_mut().find(|p| p.id == id) {
        p.value = value
    } else {
        v.push(Property { id, value });
        v.sort_by_key(|p| p.id)
    }
}
pub(crate) fn int(w: &WeenieV1, n: &str) -> Result<Option<i32>, TreasureError> {
    let id = id("PropertyInt", n)?;
    Ok(w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value))
}
pub(crate) fn si(w: &mut WeenieV1, n: &str, v: i32) -> Result<(), TreasureError> {
    set(&mut w.properties.ints, id("PropertyInt", n)?, v);
    Ok(())
}
pub(crate) fn ri(w: &mut WeenieV1, n: &str) -> Result<(), TreasureError> {
    let id = id("PropertyInt", n)?;
    w.properties.ints.retain(|p| p.id != id);
    Ok(())
}
pub(crate) fn float(w: &WeenieV1, n: &str) -> Result<Option<f64>, TreasureError> {
    let id = id("PropertyFloat", n)?;
    Ok(w.properties
        .floats
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value))
}
pub(crate) fn sf(w: &mut WeenieV1, n: &str, v: f64) -> Result<(), TreasureError> {
    if !v.is_finite() {
        return Err(TreasureError::Bounds);
    }
    set(&mut w.properties.floats, id("PropertyFloat", n)?, v);
    Ok(())
}
pub(crate) fn rf(w: &mut WeenieV1, n: &str) -> Result<(), TreasureError> {
    let id = id("PropertyFloat", n)?;
    w.properties.floats.retain(|p| p.id != id);
    Ok(())
}
pub(crate) fn did(w: &WeenieV1, n: &str) -> Result<Option<u32>, TreasureError> {
    let id = id("PropertyDataId", n)?;
    Ok(w.properties
        .data_ids
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value))
}
pub(crate) fn sd(w: &mut WeenieV1, n: &str, v: u32) -> Result<(), TreasureError> {
    set(&mut w.properties.data_ids, id("PropertyDataId", n)?, v);
    Ok(())
}
pub(crate) fn rd(w: &mut WeenieV1, n: &str) -> Result<(), TreasureError> {
    let id = id("PropertyDataId", n)?;
    w.properties.data_ids.retain(|p| p.id != id);
    Ok(())
}
pub(crate) fn string(w: &WeenieV1, n: &str) -> Result<Option<String>, TreasureError> {
    let id = id("PropertyString", n)?;
    Ok(w.properties
        .strings
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value.clone()))
}
pub(crate) fn ss(w: &mut WeenieV1, n: &str, v: String) -> Result<(), TreasureError> {
    set(&mut w.properties.strings, id("PropertyString", n)?, v);
    Ok(())
}
pub(crate) fn has_filter(w: &WeenieV1, n: &str) -> Result<bool, TreasureError> {
    let Some(filter) = did(w, "MutateFilter")? else {
        return Ok(false);
    };
    let base = en("MutateFilter", "Base")?;
    let flags = match filter {
        0x0e000012 => {
            base | en("MutateFilter", "ArmorModVsType")? | en("MutateFilter", "EncumbranceVal")?
        }
        0x0e000013 => {
            base | en("MutateFilter", "ArmorModVsType")? | en("MutateFilter", "ShieldValue")?
        }
        0x0e000014 | 0x0e000015 | 0x0e00001d => base | en("MutateFilter", "WeaponTime")?,
        0x0e000016 => base,
        _ => return Ok(false),
    };
    let expected = en("MutateFilter", n)?;
    Ok(flags & expected == expected)
}
pub(crate) fn number(v: f64) -> Result<i32, TreasureError> {
    if v.is_finite() && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
        Ok(v as i32)
    } else {
        Err(TreasureError::Bounds)
    }
}
pub(crate) fn workmanship(w: &WeenieV1, offset: i32) -> Result<f32, TreasureError> {
    Ok(1.0
        + int(w, "ItemWorkmanship")?
            .map(|v| (v - offset) as f32 / 9.0)
            .unwrap_or(0.0))
}
