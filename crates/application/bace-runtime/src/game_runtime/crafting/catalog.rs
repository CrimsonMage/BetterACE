//! Accepted recipe namespaces only, prepared on bounded cold capacity.
use bace_content::{RecipeRowV1, WeenieV1, WorldRecordV1};
use bace_crafting::{NativeRecipe, NativeRecipeRows, prepare_native_recipe};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::collections::{BTreeMap, BTreeSet};
pub(super) struct Catalog {
    revision: u64,
    cookbook: BTreeMap<(u32, u32), u32>,
    recipes: BTreeMap<u32, RecipeRowV1>,
    rows: Vec<WorldRecordV1>,
}
impl Catalog {
    pub(super) fn matches_generation(&self, generation: &PackGeneration) -> bool {
        generation
            .revision()
            .checked_add(1)
            .is_some_and(|revision| revision == self.revision)
    }
    pub(super) fn load(generation: &PackGeneration) -> Result<Self, String> {
        let mut this = Self {
            // Domain confirmation revisions are nonzero; generation zero is a
            // legitimate accepted base pack, encoded as revision one.
            revision: generation
                .revision()
                .checked_add(1)
                .ok_or("crafting generation exhaustion")?,
            cookbook: BTreeMap::new(),
            recipes: BTreeMap::new(),
            rows: Vec::new(),
        };
        let mut bytes = 0usize;
        let mut count = 0usize;
        for (first, last) in [(16, 16), (24, 37)] {
            let mut cursor = Some(PackKey {
                namespace: first - 1,
                id: u64::MAX,
            });
            'scan: loop {
                let batch = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
                if batch.is_empty() {
                    break;
                }
                for (key, lookup) in batch {
                    cursor = Some(key);
                    if key.namespace > last {
                        break 'scan;
                    }
                    if key.namespace < first {
                        return Err("crafting namespace order".into());
                    }
                    let PackLookup::Record(record) = lookup else {
                        continue;
                    };
                    count += 1;
                    bytes = bytes
                        .checked_add(record.bytes().len())
                        .ok_or("crafting byte overflow")?;
                    if count > 262144 || bytes > 64 * 1024 * 1024 {
                        return Err("crafting catalog capacity".into());
                    }
                    let row = bace_content_tools::decode_world_record(record.bytes())?;
                    if row.namespace() != key.namespace || row.id() != key.id {
                        return Err("crafting record identity".into());
                    }
                    match row {
                        WorldRecordV1::CookBook(row) => {
                            if this
                                .cookbook
                                .insert((row.source_w_c_i_d, row.target_w_c_i_d), row.recipe_id)
                                .is_some()
                            {
                                return Err("duplicate crafting pair".into());
                            }
                        }
                        WorldRecordV1::Recipe(row) => {
                            this.recipes.insert(row.id, row);
                        }
                        other => this.rows.push(other),
                    }
                }
            }
        }
        Ok(this)
    }
    pub(super) fn prepare(
        &self,
        source: &WeenieV1,
        target: &WeenieV1,
    ) -> Result<NativeRecipe, String> {
        let id = self
            .cookbook
            .get(&(source.weenie_id, target.weenie_id))
            .copied()
            .or_else(|| bace_crafting::select_new_tinkering_recipe(source, target))
            .ok_or("no compatible source recipe")?;
        let recipe = self.recipes.get(&id).ok_or("accepted recipe is missing")?;
        if !matches!(recipe.salvage_type, 1 | 2) || !matches!(recipe.skill, 18 | 28 | 29 | 30) {
            return Err("recipe requires generic crafting chance/owner support".into());
        }
        let mods: Vec<_> = self
            .rows
            .iter()
            .filter_map(|r| match r {
                WorldRecordV1::RecipeMod(v) if v.recipe_id == id => Some(v.clone()),
                _ => None,
            })
            .collect();
        let ids: BTreeSet<_> = mods.iter().map(|v| v.id).collect();
        macro_rules! children {
            ($variant:ident) => {
                self.rows
                    .iter()
                    .filter_map(|r| match r {
                        WorldRecordV1::$variant(v) if ids.contains(&v.recipe_mod_id) => {
                            Some(v.clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
        }
        macro_rules! requirements {
            ($variant:ident) => {
                self.rows
                    .iter()
                    .filter_map(|r| match r {
                        WorldRecordV1::$variant(v) if v.recipe_id == id => Some(v.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
        }
        prepare_native_recipe(
            recipe,
            self.revision,
            NativeRecipeRows {
                mods: &mods,
                bool_mods: &children!(RecipeModsBool),
                int_mods: &children!(RecipeModsInt),
                float_mods: &children!(RecipeModsFloat),
                string_mods: &children!(RecipeModsString),
                iid_mods: &children!(RecipeModsIID),
                did_mods: &children!(RecipeModsDID),
                bool_requirements: &requirements!(RecipeRequirementsBool),
                int_requirements: &requirements!(RecipeRequirementsInt),
                float_requirements: &requirements!(RecipeRequirementsFloat),
                string_requirements: &requirements!(RecipeRequirementsString),
                iid_requirements: &requirements!(RecipeRequirementsIID),
                did_requirements: &requirements!(RecipeRequirementsDID),
            },
        )
        .map_err(|e| format!("recipe {id} preparation: {e:?}"))
    }
}

#[cfg(test)]
mod tests;
