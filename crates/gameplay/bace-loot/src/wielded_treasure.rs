//! Active pinned WorldObject_Equipment.GenerateWieldedTreasureSets method;
//! the older TreasureWieldedTable implementation is commented out upstream.
use crate::treasure_random::{TreasureError, TreasureRandom, inclusive, unit};
use bace_content::{Property, TreasureWieldedRowV1, WeenieV1};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct WieldedTreasure {
    rows: Vec<TreasureWieldedRowV1>,
    templates: BTreeMap<u32, Arc<WeenieV1>>,
}
impl WieldedTreasure {
    pub fn treasure_type(&self) -> Option<u32> {
        let id = self.rows.first()?.treasure_type;
        self.rows
            .iter()
            .all(|row| row.treasure_type == id)
            .then_some(id)
    }

    pub fn prepare(
        rows: Vec<TreasureWieldedRowV1>,
        mut resolve: impl FnMut(u32) -> Option<Arc<WeenieV1>>,
    ) -> Result<Self, TreasureError> {
        if rows.len() > 4096 {
            return Err(TreasureError::Capacity);
        }
        let mut templates = BTreeMap::new();
        for row in &rows {
            if !row.probability.is_finite()
                || row.probability < 0.0
                || !row.shade.is_finite()
                || !row.stack_size_variance.is_finite()
                || row.stack_size_variance < 0.0
            {
                return Err(TreasureError::Bounds);
            }
            if let std::collections::btree_map::Entry::Vacant(entry) =
                templates.entry(row.weenie_class_id)
            {
                let template = resolve(row.weenie_class_id)
                    .ok_or(TreasureError::MissingTemplate(row.weenie_class_id))?;
                if template.weenie_id != row.weenie_class_id
                    || template.validate(Default::default()).is_err()
                {
                    return Err(TreasureError::InvalidTemplate(row.weenie_class_id));
                }
                entry.insert(template);
            }
        }
        Ok(Self { rows, templates })
    }
    /// Ordered item materialization interleaves variance draws with set draws,
    /// including the unconditional draw at entry to skipped recursive subsets.
    pub fn generate<R: TreasureRandom>(
        &self,
        random: &mut R,
    ) -> Result<Vec<WeenieV1>, TreasureError> {
        let mut cursor = random.clone();
        let mut result = Vec::new();
        self.walk(&mut 0, false, 0, &mut cursor, &mut result)?;
        *random = cursor;
        Ok(result)
    }
    fn walk<R: TreasureRandom>(
        &self,
        index: &mut isize,
        skip: bool,
        depth: usize,
        random: &mut R,
        output: &mut Vec<WeenieV1>,
    ) -> Result<(), TreasureError> {
        if depth > 64 {
            return Err(TreasureError::Capacity);
        }
        let mut roll = unit(random)?;
        let mut probability = 0.0f32;
        let mut rolled = false;
        let mut continued = false;
        while *index >= 0 && (*index as usize) < self.rows.len() {
            let row = &self.rows[*index as usize];
            if row.continues_previous_set {
                if !continued {
                    *index -= 1;
                    return Ok(());
                }
                continued = false;
            }
            let mut skip_next = true;
            if !skip {
                if row.set_start || probability >= 1.0 {
                    roll = unit(random)?;
                    probability = 0.0;
                    rolled = false;
                }
                probability += row.probability;
                if !probability.is_finite() {
                    return Err(TreasureError::Bounds);
                }
                if roll < f64::from(probability) && !rolled {
                    rolled = true;
                    skip_next = false;
                    if output.len() == 4096 {
                        return Err(TreasureError::Capacity);
                    }
                    output.push(materialize(
                        row,
                        &self.templates[&row.weenie_class_id],
                        random,
                    )?);
                }
            }
            if row.has_sub_set {
                *index += 1;
                self.walk(index, skip_next, depth + 1, random, output)?;
                continued = true;
            }
            *index += 1;
        }
        Ok(())
    }
}
fn set<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(entry) = values.iter_mut().find(|entry| entry.id == id) {
        entry.value = value;
    } else {
        values.push(Property { id, value });
        values.sort_by_key(|entry| entry.id);
    }
}
/// WorldObjectFactory subclasses of Stackable at the pinned revision.
pub fn is_stackable(weenie_type: u32) -> bool {
    matches!(weenie_type, 4 | 5 | 9 | 18 | 32 | 38 | 44 | 51)
}
pub fn set_treasure_stack(item: &mut WeenieV1, stack: i32) -> Result<(), TreasureError> {
    if !is_stackable(item.weenie_type) {
        return Ok(());
    }
    let props = &mut item.properties;
    let burden = props
        .ints
        .iter()
        .find(|v| v.id == 13)
        .map_or(0, |v| v.value)
        .checked_mul(stack)
        .ok_or(TreasureError::Bounds)?;
    let value = props
        .ints
        .iter()
        .find(|v| v.id == 15)
        .map_or(0, |v| v.value)
        .checked_mul(stack)
        .ok_or(TreasureError::Bounds)?;
    set(&mut props.ints, 12, stack);
    set(&mut props.ints, 5, burden);
    set(&mut props.ints, 19, value);
    Ok(())
}
fn materialize<R: TreasureRandom>(
    row: &TreasureWieldedRowV1,
    template: &WeenieV1,
    random: &mut R,
) -> Result<WeenieV1, TreasureError> {
    let mut item = template.clone();
    if row.palette_id > 0 {
        set(
            &mut item.properties.ints,
            3,
            i32::try_from(row.palette_id).map_err(|_| TreasureError::Bounds)?,
        );
    }
    if row.shade > 0.0 {
        set(&mut item.properties.floats, 12, f64::from(row.shade));
    }
    if row.stack_size > 0 {
        let stack = if row.stack_size_variance > 0.0 {
            let low =
                ((row.stack_size as f32 * (1.0 - row.stack_size_variance)).round() as i32).max(1);
            inclusive(random, low, row.stack_size)?
        } else {
            row.stack_size
        };
        set_treasure_stack(&mut item, stack)?;
    }
    item.validate(Default::default())
        .map_err(|_| TreasureError::InvalidTemplate(item.weenie_id))?;
    Ok(item)
}
