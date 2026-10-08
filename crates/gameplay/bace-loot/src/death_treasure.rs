//! ACE death treasure orchestration. All outputs and cursor changes publish atomically.
use crate::{
    TreasureAssets, TreasureCategory, TreasureError, TreasureRandom, TreasureRoll, select_treasure,
    treasure_random::inclusive,
};
use bace_content::{TreasureDeathRowV1, WeenieV1};
use std::sync::Arc;
pub struct DeathTreasure {
    profile: TreasureDeathRowV1,
    assets: Arc<TreasureAssets>,
    aetheria_drop_rate: f32,
}
pub(crate) struct MutationContext<'a, R> {
    pub assets: &'a TreasureAssets,
    pub profile: &'a TreasureDeathRowV1,
    pub random: &'a mut R,
}
impl DeathTreasure {
    pub fn treasure_type(&self) -> u32 {
        self.profile.treasure_type
    }

    pub fn prepare(
        profile: TreasureDeathRowV1,
        assets: Arc<TreasureAssets>,
        aetheria_drop_rate: f32,
    ) -> Result<Self, TreasureError> {
        if !(1..=8).contains(&profile.tier)
            || !profile.loot_quality_mod.is_finite()
            || !aetheria_drop_rate.is_finite()
        {
            return Err(TreasureError::Bounds);
        }
        for (min, max) in [
            (profile.item_min_amount, profile.item_max_amount),
            (profile.magic_item_min_amount, profile.magic_item_max_amount),
            (
                profile.mundane_item_min_amount,
                profile.mundane_item_max_amount,
            ),
        ] {
            if min < 0 || max < min || max > 1365 {
                return Err(TreasureError::Capacity);
            }
        }
        if assets.templates.len() > 65536 {
            return Err(TreasureError::Capacity);
        }
        assets.validate()?;
        for (&id, w) in &assets.templates {
            if id != w.weenie_id || w.validate(Default::default()).is_err() {
                return Err(TreasureError::InvalidTemplate(id));
            }
        }
        Ok(Self {
            profile,
            assets,
            aetheria_drop_rate,
        })
    }
    pub fn generate<R: TreasureRandom>(
        &self,
        random: &mut R,
    ) -> Result<Vec<WeenieV1>, TreasureError> {
        let mut cursor = random.clone();
        let mut ctx = MutationContext {
            assets: &self.assets,
            profile: &self.profile,
            random: &mut cursor,
        };
        let mut output = Vec::new();
        let p = &self.profile;
        for (category, chance, min, max) in [
            (
                TreasureCategory::Item,
                p.item_chance,
                p.item_min_amount,
                p.item_max_amount,
            ),
            (
                TreasureCategory::Magic,
                p.magic_item_chance,
                p.magic_item_min_amount,
                p.magic_item_max_amount,
            ),
            (
                TreasureCategory::Mundane,
                p.mundane_item_chance,
                p.mundane_item_min_amount,
                p.mundane_item_max_amount,
            ),
        ] {
            if inclusive(ctx.random, 1, 100)? > chance {
                continue;
            }
            let count = inclusive(ctx.random, min, max)?;
            for _ in 0..count {
                let roll = select_treasure(p, category, ctx.random)?;
                if roll.wcid != 0 {
                    output.push(ctx.materialize(roll, category == TreasureCategory::Magic)?);
                }
            }
            if category == TreasureCategory::Mundane
                && let Some(item) = ctx.mundane_addon(self.aetheria_drop_rate)?
            {
                output.push(item)
            }
        }
        *random = cursor;
        Ok(output)
    }
}
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn template(&self, id: u32) -> Result<WeenieV1, TreasureError> {
        self.assets
            .templates
            .get(&id)
            .map(|v| (**v).clone())
            .ok_or(TreasureError::MissingTemplate(id))
    }
    pub(crate) fn materialize(
        &mut self,
        mut roll: TreasureRoll,
        magical: bool,
    ) -> Result<WeenieV1, TreasureError> {
        if roll.item_type == 8 {
            return self.scroll();
        }
        let mut w = self.template(roll.wcid)?;
        roll.base_armor_level = crate::treasure_properties::int(&w, "ArmorLevel")?.unwrap_or(0);
        match roll.item_type {
            1 => {
                let (lo, hi) = [
                    (5, 50),
                    (10, 200),
                    (10, 500),
                    (25, 1000),
                    (50, 5000),
                    (250, 5000),
                    (250, 5000),
                    (250, 5000),
                ][(self.profile.tier - 1) as usize];
                let n = inclusive(self.random, lo, hi)?;
                crate::set_treasure_stack(&mut w, n)?;
            }
            2 => self.mutate_gem(&mut w, magical, &mut roll)?,
            3 => {
                if roll.base_armor_level > 0 {
                    self.mutate_armor(&mut w, magical, &mut roll)?
                } else {
                    self.mutate_jewelry(&mut w, magical, &mut roll)?
                }
            }
            4 => self.mutate_dinnerware(&mut w, magical, &mut roll)?,
            5 | 9 => self.mutate_weapon(&mut w, magical, &mut roll)?,
            6 | 7 | 15 => self.mutate_armor(&mut w, magical, &mut roll)?,
            25 => self.mutate_cloak(&mut w, &roll)?,
            26 => self.mutate_pet(&mut w)?,
            _ => {}
        }
        w.validate(Default::default())
            .map_err(|_| TreasureError::InvalidTemplate(w.weenie_id))?;
        Ok(w)
    }
}
