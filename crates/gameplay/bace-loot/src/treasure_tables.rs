//! Ordered ACE ChanceTable rolls. Source: pinned Factories/Entity/ChanceTable.cs.
use crate::{
    TreasureError, TreasureRandom, ace_tables,
    treasure_random::{inclusive, unit},
};
pub(crate) fn missing(class: &str, field: &str) -> TreasureError {
    TreasureError::MissingSourceTable(format!("{class}.{field}"))
}
pub(crate) fn en(kind: &str, name: &str) -> Result<i32, TreasureError> {
    ace_tables::enum_value(kind, name)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| missing(kind, name))
}
pub(crate) fn chance<R: TreasureRandom>(
    rows: &[(i64, f32)],
    quality: f32,
    r: &mut R,
) -> Result<i32, TreasureError> {
    let roll = unit(r)?;
    let mut total = 0.0f32;
    let mut last = None;
    for &(value, p) in rows {
        if !p.is_finite() || p < 0.0 {
            return Err(TreasureError::Bounds);
        }
        total += p;
        if p > 0.0 {
            last = Some(value);
        }
        if roll < f64::from(total) && total >= quality {
            return i32::try_from(value).map_err(|_| TreasureError::Bounds);
        }
    }
    i32::try_from(last.ok_or(TreasureError::Bounds)?).map_err(|_| TreasureError::Bounds)
}
pub(crate) fn roll<R: TreasureRandom>(
    class: &str,
    field: &str,
    quality: f32,
    r: &mut R,
) -> Result<i32, TreasureError> {
    chance(
        ace_tables::lookup(class, field).ok_or_else(|| missing(class, field))?,
        quality,
        r,
    )
}
pub(crate) fn reference(
    class: &str,
    field: &str,
    index: usize,
) -> Result<(&'static str, &'static str), TreasureError> {
    ace_tables::references(class, field)
        .and_then(|v| v.get(index))
        .and_then(|v| v.split_once('.'))
        .ok_or_else(|| missing(class, field))
}
pub(crate) fn indexed<R: TreasureRandom>(
    class: &str,
    field: &str,
    index: usize,
    quality: f32,
    r: &mut R,
) -> Result<i32, TreasureError> {
    let (c, f) = reference(class, field, index)?;
    roll(c, f, quality, r)
}
pub(crate) fn pick<R: TreasureRandom, T: Clone>(
    values: &[T],
    r: &mut R,
) -> Result<T, TreasureError> {
    if values.is_empty() {
        return Err(TreasureError::Bounds);
    }
    let max = i32::try_from(values.len() - 1).map_err(|_| TreasureError::Capacity)?;
    Ok(values[inclusive(r, 0, max)? as usize].clone())
}
pub(crate) fn sequence<R: TreasureRandom>(
    class: &str,
    field: &str,
    r: &mut R,
) -> Result<i32, TreasureError> {
    i32::try_from(pick(
        ace_tables::sequence(class, field).ok_or_else(|| missing(class, field))?,
        r,
    )?)
    .map_err(|_| TreasureError::Bounds)
}
pub(crate) fn heritage<R: TreasureRandom>(
    profile: i32,
    viamontian: bool,
    r: &mut R,
) -> Result<i32, TreasureError> {
    if !(1..=24).contains(&profile) {
        return inclusive(r, 1, 3);
    }
    let profile = if viamontian && profile == 19 {
        21
    } else {
        profile
    };
    indexed(
        "HeritageChance",
        "heritageProfiles",
        (profile - 1) as usize,
        0.0,
        r,
    )
}
pub(crate) fn gem<R: TreasureRandom>(tier: i32, r: &mut R) -> Result<(i32, i32), TreasureError> {
    let class = indexed(
        "GemClassChance",
        "gemClassChances",
        (tier.clamp(1, 6) - 1) as usize,
        0.0,
        r,
    )?;
    let (c, f) = reference(
        "GemMaterialChance",
        "gemMaterialChances",
        (class - 1) as usize,
    )?;
    let rows = ace_tables::gem(c, f).ok_or_else(|| missing(c, f))?;
    let draw = unit(r)?;
    let mut total = 0.0f32;
    let mut last = None;
    for &(w, m, p) in rows {
        total += p;
        if p > 0.0 {
            last = Some((w, m));
        }
        if draw < f64::from(total) {
            return Ok((w as i32, m as i32));
        }
    }
    let (w, m) = last.ok_or(TreasureError::Bounds)?;
    Ok((w as i32, m as i32))
}
