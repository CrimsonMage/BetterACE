//! Immutable zone and startup residency policies, independent of TOML adapters.
use crate::PortalPosition;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldPolicies {
    no_relog: BTreeSet<u16>,
    no_death_item_drop: BTreeSet<u16>,
    no_kill_experience: BTreeSet<u16>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldPolicyError {
    Bounds,
    Duplicate,
    Invalid,
}
impl WorldPolicies {
    pub fn new(
        no_relog: &[u16],
        no_death_item_drop: &[u16],
        no_kill_experience: &[u16],
    ) -> Result<Self, WorldPolicyError> {
        fn checked(values: &[u16]) -> Result<BTreeSet<u16>, WorldPolicyError> {
            if values.len() > 4096 {
                return Err(WorldPolicyError::Bounds);
            }
            let result: BTreeSet<_> = values.iter().copied().collect();
            if result.len() != values.len() {
                return Err(WorldPolicyError::Duplicate);
            }
            Ok(result)
        }
        Ok(Self {
            no_relog: checked(no_relog)?,
            no_death_item_drop: checked(no_death_item_drop)?,
            no_kill_experience: checked(no_kill_experience)?,
        })
    }
    pub fn forbids_relog(&self, cell: u32) -> bool {
        self.no_relog.contains(&((cell >> 16) as u16))
    }
    pub fn prevents_death_item_loss(&self, cell: u32) -> bool {
        self.no_death_item_drop.contains(&((cell >> 16) as u16))
    }
    pub fn suppresses_kill_experience(&self, cell: u32) -> bool {
        self.no_kill_experience.contains(&((cell >> 16) as u16))
    }
    /// ACE exempts Sentinel/Admin weenie types, not arbitrary elevated accounts.
    pub fn login_destination(
        &self,
        location: Option<PortalPosition>,
        sanctuary: Option<PortalPosition>,
        sentinel_or_admin: bool,
    ) -> Result<Option<PortalPosition>, WorldPolicyError> {
        if sentinel_or_admin || !location.is_some_and(|p| self.forbids_relog(p.cell)) {
            return Ok(None);
        }
        let Some(sanctuary) = sanctuary else {
            return Ok(None);
        };
        sanctuary
            .validate()
            .map_err(|_| WorldPolicyError::Invalid)?;
        Ok(Some(sanctuary))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionPreload {
    pub landblock: u16,
    pub permanent: bool,
    pub include_adjacent: bool,
}

/// One immediate outdoor ring; dungeon classification must come from DAT assets.
pub fn adjacent_landblocks(landblock: u16, dungeon: bool) -> Vec<u16> {
    if dungeon {
        return Vec::new();
    }
    let x = i32::from(landblock >> 8);
    let y = i32::from(landblock & 255);
    [
        (0, 1),
        (0, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (1, 1),
        (-1, -1),
        (1, -1),
    ]
    .into_iter()
    .filter_map(|(dx, dy)| {
        let (x, y) = (x + dx, y + dy);
        if (0..=255).contains(&x) && (0..=255).contains(&y) {
            Some(((x as u16) << 8) | y as u16)
        } else {
            None
        }
    })
    .collect()
}
pub fn apartment_landblocks() -> impl Iterator<Item = u16> {
    (0x72_u16..=0x99).map(|x| x << 8).chain(0x5360..=0x5369)
}
