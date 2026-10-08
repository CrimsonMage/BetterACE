//! GDLE 353cbab Player.cpp:2389-2599, with conservation/durability hardening.
use crate::CraftError;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SalvageTool {
    pub id: u32,
    pub owner: u32,
    pub revision: u64,
    pub is_ust: bool,
    pub reserved: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SalvageSkills {
    pub salvaging: u32,
    pub armor: u32,
    pub weapon: u32,
    pub magic_item: u32,
    pub item: u32,
    pub augmentations: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SalvageInput {
    pub id: u32,
    pub owner: u32,
    pub revision: u64,
    pub stack: u32,
    pub material: u32,
    pub raw_workmanship: u32,
    pub value: u32,
    pub retained: bool,
    pub equipped: bool,
    pub in_trade: bool,
    pub reserved: bool,
    pub is_salvage: bool,
    pub structure: u32,
    pub num_items: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedSalvage {
    Ownership,
    Retained,
    MissingMaterial,
    Workmanship,
    Stacked,
    Equipped,
    InTrade,
    Reserved,
    InvalidBag,
}
#[derive(Clone, Debug)]
pub struct SalvageRequest<'a> {
    pub actor: u32,
    pub actor_revision: u64,
    pub operation_id: [u8; 16],
    pub tool: SalvageTool,
    pub skills: SalvageSkills,
    pub items: &'a [SalvageInput],
    /// Prepared, validated native material -> bag template lookup.
    pub bag_templates: &'a BTreeMap<u32, u32>,
    pub free_slots: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SalvageBag {
    pub template: u32,
    pub material: u32,
    pub units: u32,
    pub value: u32,
    /// Ratio preserved on every split bag. These are metadata, not unit counts.
    pub raw_workmanship: u32,
    pub num_items: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SalvageResult {
    pub material: u32,
    pub units: u32,
    pub workmanship: f64,
    pub skill: u32,
    pub augmentation_bonus: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SalvageProposal {
    pub actor: u32,
    pub expected_actor_revision: u64,
    pub operation_id: [u8; 16],
    pub tool: SalvageTool,
    pub consumed: Vec<(u32, u64)>,
    pub unsuitable: Vec<(u32, UnsupportedSalvage)>,
    pub bags: Vec<SalvageBag>,
    /// Emit one 0x02B4 per entry; if empty, emit one empty-result response.
    /// Include unsuitable IDs once, on the first (or empty) response.
    pub results: Vec<SalvageResult>,
    /// GDLE reports the best tinkering skill if it won for any consumed item.
    pub reporting_skill: u32,
    pub augmentation_bonus: u32,
    /// GDLE's intentional per-material 75,000 value ceiling, reported explicitly.
    pub value_capped: u64,
}

pub fn salvage_amount(skill: u32, workmanship: u32, augmentations: i32) -> Result<u32, CraftError> {
    if workmanship == 0 {
        return Err(CraftError::InvalidState);
    }
    let count = 1.0
        + (f64::from(skill) / 195.0
            * f64::from(workmanship)
            * (1.0 + 0.25 * f64::from(augmentations.clamp(0, 4))))
        .floor();
    if !count.is_finite() || count > f64::from(u32::MAX) {
        return Err(CraftError::Overflow);
    }
    Ok(count as u32)
}

#[derive(Default)]
struct Group {
    units: u32,
    value: u64,
    workmanship: u32,
    num_items: u32,
}

pub fn propose_salvage(request: SalvageRequest<'_>) -> Result<SalvageProposal, CraftError> {
    if request.actor == 0
        || request.actor == u32::MAX
        || request.actor_revision == 0
        || request.operation_id == [0; 16]
        || request.tool.id == 0
        || request.tool.revision == 0
        || !request.tool.is_ust
    {
        return Err(CraftError::InvalidState);
    }
    if request.tool.owner != request.actor {
        return Err(CraftError::Ownership);
    }
    if request.tool.reserved {
        return Err(CraftError::Busy);
    }
    if request.items.len() > 300 || request.bag_templates.len() > 256 {
        return Err(CraftError::Capacity);
    }
    let mut seen = BTreeSet::new();
    let mut groups: BTreeMap<u32, Group> = BTreeMap::new();
    let mut proposal = SalvageProposal {
        actor: request.actor,
        expected_actor_revision: request.actor_revision,
        operation_id: request.operation_id,
        tool: request.tool,
        consumed: Vec::new(),
        unsuitable: Vec::new(),
        bags: Vec::new(),
        results: Vec::new(),
        reporting_skill: 40,
        augmentation_bonus: request.skills.augmentations.clamp(0, 4) as u32 * 25,
        value_capped: 0,
    };
    let skills = request.skills;
    let mut best = (skills.armor, 29);
    for candidate in [
        (skills.weapon, 28),
        (skills.magic_item, 30),
        (skills.item, 18),
    ] {
        if candidate.0 > best.0 {
            best = candidate;
        }
    }
    for item in request.items {
        if item.id == 0 || item.id == u32::MAX || item.revision == 0 || item.id == request.actor {
            return Err(CraftError::InvalidState);
        }
        if item.id == request.tool.id || !seen.insert(item.id) {
            return Err(CraftError::Duplicate);
        }
        let unsuitable = if item.owner != request.actor {
            Some(UnsupportedSalvage::Ownership)
        } else if item.stack != 1 {
            Some(UnsupportedSalvage::Stacked)
        } else if item.retained {
            Some(UnsupportedSalvage::Retained)
        } else if item.equipped {
            Some(UnsupportedSalvage::Equipped)
        } else if item.in_trade {
            Some(UnsupportedSalvage::InTrade)
        } else if item.reserved {
            Some(UnsupportedSalvage::Reserved)
        } else if item.material == 0 || !request.bag_templates.contains_key(&item.material) {
            Some(UnsupportedSalvage::MissingMaterial)
        } else if item.raw_workmanship == 0 {
            Some(UnsupportedSalvage::Workmanship)
        } else if item.is_salvage
            && (item.structure == 0 || item.structure > 100 || item.num_items == 0)
        {
            Some(UnsupportedSalvage::InvalidBag)
        } else {
            None
        };
        if let Some(reason) = unsuitable {
            proposal.unsuitable.push((item.id, reason));
            continue;
        }
        let (units, value, count, skill) = if item.is_salvage {
            (item.structure, u64::from(item.value), item.num_items, 40)
        } else {
            let salvage =
                salvage_amount(skills.salvaging, item.raw_workmanship, skills.augmentations)?;
            let tinkering =
                salvage_amount(best.0, item.raw_workmanship, 0)?.min(item.raw_workmanship);
            let value = (f64::from(item.value)
                * (f64::from(skills.salvaging) / 387.0)
                * (1.0 + f64::from(skills.augmentations.clamp(0, 4)) * 0.25))
                .trunc()
                .max(1.0);
            if value > u64::MAX as f64 {
                return Err(CraftError::Overflow);
            }
            (
                salvage.max(tinkering),
                value as u64,
                1,
                if tinkering > salvage { best.1 } else { 40 },
            )
        };
        let group = groups.entry(item.material).or_default();
        group.units = group.units.checked_add(units).ok_or(CraftError::Overflow)?;
        group.value = group.value.checked_add(value).ok_or(CraftError::Overflow)?;
        group.workmanship = group
            .workmanship
            .checked_add(item.raw_workmanship)
            .ok_or(CraftError::Overflow)?;
        group.num_items = group
            .num_items
            .checked_add(count)
            .ok_or(CraftError::Overflow)?;
        if skill != 40 {
            proposal.reporting_skill = skill;
            proposal.augmentation_bonus = 0;
        }
        proposal.consumed.push((item.id, item.revision));
    }
    let needed = groups.values().try_fold(0usize, |sum, g| {
        sum.checked_add((u64::from(g.units).div_ceil(100)) as usize)
            .ok_or(CraftError::Overflow)
    })?;
    if needed > request.free_slots || needed > 300 {
        return Err(CraftError::Capacity);
    }
    for (material, group) in groups {
        let template = *request
            .bag_templates
            .get(&material)
            .ok_or(CraftError::InvalidState)?;
        if template == 0 {
            return Err(CraftError::InvalidState);
        }
        let total_value = group.value.min(75000) as u32;
        proposal.value_capped = proposal
            .value_capped
            .checked_add(group.value - u64::from(total_value))
            .ok_or(CraftError::Overflow)?;
        let mut remaining = group.units;
        let mut assigned = 0;
        while remaining > 0 {
            let units = remaining.min(100);
            let value = if units == remaining {
                total_value - assigned
            } else {
                (u64::from(total_value) * u64::from(units) / u64::from(group.units)) as u32
            };
            assigned += value;
            remaining -= units;
            proposal.bags.push(SalvageBag {
                template,
                material,
                units,
                value,
                raw_workmanship: group.workmanship,
                num_items: group.num_items,
            });
            proposal.results.push(SalvageResult {
                material,
                units,
                workmanship: f64::from(group.workmanship) / f64::from(group.num_items),
                skill: proposal.reporting_skill,
                augmentation_bonus: proposal.augmentation_bonus,
            });
        }
    }
    Ok(proposal)
}
