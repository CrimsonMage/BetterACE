//! Active WCID routing from pinned ACE LootGenerationFactory.RollWcid and Tables/Wcids.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables, treasure_random::inclusive,
    treasure_tables::*,
};
use bace_content::TreasureDeathRowV1;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreasureCategory {
    Item,
    Magic,
    Mundane,
}
pub fn select_treasure<R: TreasureRandom>(
    p: &TreasureDeathRowV1,
    category: TreasureCategory,
    r: &mut R,
) -> Result<TreasureRoll, TreasureError> {
    if !(1..=8).contains(&p.tier) || !p.loot_quality_mod.is_finite() {
        return Err(TreasureError::Bounds);
    }
    let (class, field, index) = match category {
        TreasureCategory::Item => (
            "TreasureProfile_Item",
            "itemProfiles",
            p.item_treasure_type_selection_chances,
        ),
        TreasureCategory::Magic => (
            "TreasureProfile_MagicItem",
            "magicItemProfiles",
            p.magic_item_treasure_type_selection_chances,
        ),
        TreasureCategory::Mundane => (
            "TreasureProfile_Mundane",
            "mundaneProfiles",
            p.mundane_item_type_selection_chances,
        ),
    };
    let count = ace_tables::references(class, field)
        .ok_or_else(|| missing(class, field))?
        .len();
    if index < 1 || index as usize > count {
        return Ok(TreasureRoll::default());
    }
    let item = indexed(class, field, (index - 1) as usize, 0.0, r)?;
    let mut result = TreasureRoll {
        item_type: item,
        ..TreasureRoll::default()
    };
    let t = (p.tier - 1) as usize;
    let q = p.loot_quality_mod;
    let wcid = match item {
        0 => 0,
        1 => en("WeenieClassName", "coinstack")?,
        2 => gem(p.tier, r)?.0,
        3 => indexed("JewelryWcids", "tierChances", t.min(5), 0.0, r)?,
        4 => indexed("GenericWcids", "tierChances", t.min(5), 0.0, r)?,
        5 => {
            result.weapon_type = roll("WeaponTypeChance", "RetailChances", 0.0, r)?;
            weapon(p, &mut result.weapon_type, r)?
        }
        6 => {
            result.armor_type = indexed("ArmorTypeChance", "armorTiers", t, 0.0, r)?;
            armor(p, &mut result.armor_type, r)?
        }
        7 => {
            let group = heritage(p.unknown_chances, true, r)?;
            match group {
                1..=4 => roll(
                    "ClothingWcids",
                    [
                        "ClothingWcids_Aluvian",
                        "ClothingWcids_Gharundim",
                        "ClothingWcids_Sho",
                        "ClothingWcids_Viamontian",
                    ][(group - 1) as usize],
                    0.0,
                    r,
                )?,
                _ => 0,
            }
        }
        8 => sequence("ScrollWcids", "scrollWcids", r)?,
        9 => {
            result.weapon_type = 16;
            indexed("CasterWcids", "casterTiers", t, 0.0, r)?
        }
        10 => indexed("ManaStoneWcids", "manaStoneTiers", t, q, r)?,
        11 => indexed("ConsumeWcids", "consumeTiers", t, q, r)?,
        12 => indexed("HealKitWcids", "healKitTiers", t, q, r)?,
        13 => indexed("LockpickWcids", "lockpickTiers", t, q, r)?,
        14 => component(p, r)?,
        15..=24 => {
            let h = heritage(p.unknown_chances, false, r)?;
            result.item_type = 15;
            result.armor_type = 25;
            if (14..=16).contains(&h) && (16..=24).contains(&item) {
                let (c, f) =
                    reference("SocietyArmorWcids", "societyArmorTables", (h - 14) as usize)?;
                ace_tables::sequence(c, f)
                    .and_then(|v| v.get((item - 16) as usize))
                    .copied()
                    .ok_or_else(|| missing(c, f))? as i32
            } else {
                0
            }
        }
        25 => sequence("CloakWcids", "cloakWcids", r)?,
        26 => pet(p, r)?,
        27 => en("WeenieClassName", "ace49485_encapsulatedspirit")?,
        _ => 0,
    };
    result.wcid = u32::try_from(wcid).map_err(|_| TreasureError::Bounds)?;
    Ok(result)
}
fn weapon<R: TreasureRandom>(
    p: &TreasureDeathRowV1,
    kind: &mut i32,
    r: &mut R,
) -> Result<i32, TreasureError> {
    let t = (p.tier - 1) as usize;
    let typed = match *kind {
        2 | 3 | 5 | 7 | 8 | 9 | 11 => Some(match inclusive(r, 1, 3)? {
            1 => ("HeavyWeaponWcids", "heavyWeaponsTables"),
            2 => ("LightWeaponWcids", "lightWeaponsTables"),
            _ => ("FinesseWeaponWcids", "finesseWeaponsTables"),
        }),
        17 => Some(("TwoHandedWeaponWcids", "twoHandedWeaponTables")),
        _ => None,
    };
    if let Some((c, f)) = typed {
        let (field, k) = pick(
            ace_tables::typed_references(c, f).ok_or_else(|| missing(c, f))?,
            r,
        )?;
        *kind = k as i32;
        let (c, f) = field.split_once('.').ok_or(TreasureError::Bounds)?;
        return roll(c, f, 0.0, r);
    }
    match *kind {
        13 => match heritage(p.unknown_chances, false, r)? {
            1 => indexed("BowWcids_Aluvian", "bowTiers", t, 0.0, r),
            2 => indexed("BowWcids_Gharundim", "bowTiers", t, 0.0, r),
            3 => indexed("BowWcids_Sho", "bowTiers", t, 0.0, r),
            _ => Ok(0),
        },
        14 => indexed("CrossbowWcids", "crossbowTiers", t, 0.0, r),
        15 => indexed("AtlatlWcids", "atlatlTiers", t, 0.0, r),
        16 => indexed("CasterWcids", "casterTiers", t, 0.0, r),
        _ => Ok(0),
    }
}
fn armor<R: TreasureRandom>(
    p: &TreasureDeathRowV1,
    kind: &mut i32,
    r: &mut R,
) -> Result<i32, TreasureError> {
    let family = match *kind {
        4 => Some([
            (4, "PlatemailWcids"),
            (5, "ScalemailWcids"),
            (6, "YoroiWcids"),
            (16, "DiforsaWcids"),
        ]),
        7 => Some([
            (8, "CeldonWcids"),
            (9, "AmuliWcids"),
            (10, "KoujiaWcids"),
            (17, "TenassaWcids"),
        ]),
        12 => Some([
            (13, "LoricaWcids"),
            (14, "NariyidWcids"),
            (15, "ChiranWcids"),
            (18, "AlduressaWcids"),
        ]),
        20 => Some([
            (21, "OlthoiCeldonWcids"),
            (22, "OlthoiAmuliWcids"),
            (23, "OlthoiKoujiaWcids"),
            (24, "OlthoiAlduressaWcids"),
        ]),
        _ => None,
    };
    let field = if let Some(family) = family {
        let h = heritage(p.unknown_chances, true, r)?;
        if !(1..=4).contains(&h) {
            return Ok(0);
        }
        let (k, f) = family[(h - 1) as usize];
        *kind = k;
        f
    } else {
        match *kind {
            1 => "LeatherWcids",
            2 => "StuddedLeatherWcids",
            3 => "ChainmailWcids",
            11 => "CovenantWcids",
            19 => "OlthoiWcids",
            25 => {
                let i = inclusive(r, 1, 3)?;
                *kind = 25 + i;
                [
                    "CelestialHandWcids",
                    "EldrytchWebWcids",
                    "RadiantBloodWcids",
                ][(i - 1) as usize]
            }
            29 => "HaebreanWcids",
            30 => "KnorrAcademyWcids",
            31 => "SedgemailLeatherWcids",
            32 => {
                if p.tier < 6 {
                    "OverRobe_T3_T5_Wcids"
                } else {
                    "OverRobe_T6_T8_Wcids"
                }
            }
            _ => return Ok(0),
        }
    };
    roll("ArmorWcids", field, 0.0, r)
}
fn component<R: TreasureRandom>(p: &TreasureDeathRowV1, r: &mut R) -> Result<i32, TreasureError> {
    if p.tier >= 7
        && roll(
            "SpellComponentWcids",
            "level8SpellComponentChance",
            p.loot_quality_mod,
            r,
        )? != 0
    {
        match inclusive(r, 1, 3)? {
            1 => roll("SpellComponentWcids", "Quills", p.loot_quality_mod, r),
            2 => roll("SpellComponentWcids", "Inks", p.loot_quality_mod, r),
            _ => sequence("SpellComponentWcids", "Glyphs", r),
        }
    } else {
        indexed(
            "SpellComponentWcids",
            "peaTiers",
            (p.tier - 1) as usize,
            p.loot_quality_mod,
            r,
        )
    }
}
fn pet<R: TreasureRandom>(p: &TreasureDeathRowV1, r: &mut R) -> Result<i32, TreasureError> {
    let level = indexed(
        "PetDeviceChance",
        "petLevelChances",
        (p.tier - 1) as usize,
        p.loot_quality_mod,
        r,
    )?;
    let index = [50, 80, 100, 125, 150, 180, 200]
        .iter()
        .position(|&v| v == level)
        .ok_or(TreasureError::Bounds)?;
    let variant = inclusive(r, 0, 35)? as usize;
    let group = [
        "Necromancer_PetDevices",
        "Primalist_PetDevices",
        "Naturalist_PetDevices",
    ][variant / 12];
    let (c, f) = reference("PetDeviceWcids", group, variant % 12)?;
    ace_tables::sequence(c, f)
        .and_then(|v| v.get(index))
        .copied()
        .map(|v| v as i32)
        .ok_or_else(|| missing(c, f))
}
