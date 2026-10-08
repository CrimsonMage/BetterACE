//! Source scroll retry rule, bounded so malformed prepared spell data cannot hang.
use crate::{
    TreasureError, TreasureRandom, ace_tables, death_treasure::MutationContext,
    treasure_random::inclusive, treasure_tables::*,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn scroll(&mut self) -> Result<WeenieV1, TreasureError> {
        let level = if self.profile.tier >= 5 {
            7
        } else {
            indexed(
                "ScrollLevelChance",
                "scrollLevelChances",
                (self.profile.tier - 1) as usize,
                self.profile.loot_quality_mod,
                self.random,
            )?
        };
        let names = [
            "creatureSpells",
            "lifeSpells",
            "itemSpells",
            "warSpells",
            "voidSpells",
        ];
        let mut lists = [&[][..]; 5];
        let mut count = 0;
        for (i, n) in names.iter().enumerate() {
            lists[i] = ace_tables::sequence("ScrollSpells", n)
                .ok_or_else(|| missing("ScrollSpells", n))?;
            count += lists[i].len();
        }
        for _ in 0..4096 {
            let mut index = inclusive(
                self.random,
                0,
                i32::try_from(count).map_err(|_| TreasureError::Capacity)? - 1,
            )? as usize;
            let mut base = None;
            for list in lists {
                if index < list.len() {
                    base = Some(list[index]);
                    break;
                }
                index -= list.len();
            }
            let base = base.ok_or(TreasureError::Bounds)?;
            let levels = ace_tables::spell_levels(base)
                .filter(|v| v.len() == 8)
                .ok_or(TreasureError::MissingAsset("SpellProgression", base as u32))?;
            let spell = *levels
                .get((level - 1) as usize)
                .ok_or(TreasureError::Bounds)? as u32;
            if spell == 0 {
                continue;
            }
            let id = *self
                .assets
                .scrolls_by_spell
                .get(&spell)
                .ok_or(TreasureError::MissingAsset("ScrollWeenie", spell))?;
            return self.template(id);
        }
        Err(TreasureError::Capacity)
    }
}
