//! ACE Portal.CheckUseRequirements and WorldObject_Magic portal links, with
//! GDLE metadata indices. Positions are owned values, never aliased mutable poses.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalPosition {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
impl PortalPosition {
    pub fn validate(self) -> Result<(), PortalError> {
        if self.cell == 0
            || self
                .origin
                .iter()
                .chain(&self.rotation)
                .any(|n| !n.is_finite())
        {
            return Err(PortalError::Invalid);
        }
        let norm = self
            .rotation
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>();
        if !(0.999..=1.001).contains(&norm) {
            return Err(PortalError::Invalid);
        }
        Ok(())
    }
    pub fn heading(self) -> f32 {
        let [w, x, y, z] = self.rotation;
        (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalError {
    MissingAssets,
    Invalid,
    Capacity,
    Conflict,
    NoLink,
    WrongKind,
    NoTie,
    NoRecall,
    NoSummon,
    TooLow,
    TooHigh,
    PkRecent,
    PkRestricted,
    OlthoiRestricted,
    Vitae,
    NewAccount,
    Entitlement,
    Quest,
    Teleporting,
    RecentTeleport,
    Protected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalKind {
    Portal,
    Lifestone,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PortalAnchor {
    pub entity: u32,
    pub template: u32,
    pub original_template: Option<u32>,
    pub kind: PortalKind,
    pub position: PortalPosition,
    pub destination: Option<PortalPosition>,
    pub minimum_level: u32,
    pub maximum_level: u32,
    pub restrictions: u32,
    pub no_tie: bool,
    pub ignore_pk_timer: bool,
    pub account_requirement: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalAccess {
    pub level: u32,
    pub pk_status: u32,
    pub pk_recent: bool,
    pub olthoi: bool,
    pub vitae: bool,
    pub account_15_days: bool,
    pub entitlement: u32,
    pub quest_allowed: bool,
    pub teleporting: bool,
    pub recently_teleported: bool,
    pub ignore_restrictions: bool,
    pub enforce_maximum_level: bool,
}
impl PortalAnchor {
    pub fn validate(&self) -> Result<(), PortalError> {
        self.position.validate()?;
        if let Some(destination) = self.destination {
            destination.validate()?;
        }
        if self.entity == 0
            || self.template == 0
            || self.original_template == Some(0)
            || self.restrictions > 0x3ff
        {
            return Err(PortalError::Invalid);
        }
        Ok(())
    }
    pub fn definition(&self) -> crate::PortalTemplate {
        crate::PortalTemplate {
            template: self.template,
            original_template: self.original_template,
            kind: self.kind,
            destination: self.destination,
            minimum_level: self.minimum_level,
            maximum_level: self.maximum_level,
            restrictions: self.restrictions,
            no_tie: self.no_tie,
            ignore_pk_timer: self.ignore_pk_timer,
            account_requirement: self.account_requirement,
        }
    }
    pub fn check(&self, access: PortalAccess) -> Result<(), PortalError> {
        self.definition().check(access)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PortalLinks {
    revision: u64,
    positions: BTreeMap<u16, PortalPosition>,
    templates: BTreeMap<u16, u32>,
    summoned: BTreeSet<u16>,
    summoned_presence: BTreeSet<u16>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PortalLinkMutation {
    pub before_revision: u64,
    pub after_revision: u64,
    pub position_slot: u16,
    pub before: Option<PortalPosition>,
    pub after: PortalPosition,
    pub data_id: Option<(u16, Option<u32>, u32)>,
    pub tied_summoned: bool,
}
impl PortalLinks {
    pub fn new(
        revision: u64,
        positions: &[(u16, PortalPosition)],
        templates: &[(u16, u32)],
    ) -> Result<Self, PortalError> {
        if positions.len() > 5 || templates.len() > 3 {
            return Err(PortalError::Capacity);
        }
        let mut result = Self {
            revision,
            positions: BTreeMap::new(),
            templates: BTreeMap::new(),
            summoned: BTreeSet::new(),
            summoned_presence: BTreeSet::new(),
        };
        for &(slot, value) in positions {
            if !matches!(slot, 4 | 8 | 9 | 15 | 16)
                || result.positions.insert(slot, value).is_some()
            {
                return Err(PortalError::Invalid);
            }
            value.validate()?;
        }
        for &(slot, value) in templates {
            // Present zero is valid persisted sparse state, distinct from absent.
            // Only proposals for a new live link require a nonzero template.
            if !matches!(slot, 31 | 47 | 48) || result.templates.insert(slot, value).is_some() {
                return Err(PortalError::Invalid);
            }
        }
        Ok(result)
    }
    pub fn restore_summoned_flags(&mut self, primary: bool, secondary: bool) {
        self.restore_summoned_options(Some(primary), Some(secondary));
    }
    pub fn restore_summoned_options(&mut self, primary: Option<bool>, secondary: Option<bool>) {
        self.summoned.clear();
        self.summoned_presence.clear();
        for (slot, value) in [(31, primary), (48, secondary)] {
            if let Some(value) = value {
                self.summoned_presence.insert(slot);
                if value {
                    self.summoned.insert(slot);
                }
            }
        }
    }
    pub fn summoned_flag(&self, slot: u16) -> Option<bool> {
        self.summoned_presence
            .contains(&slot)
            .then(|| self.summoned.contains(&slot))
    }
    pub fn synchronize_sanctuary(
        &mut self,
        position: Option<PortalPosition>,
    ) -> Result<bool, PortalError> {
        if let Some(position) = position {
            position.validate()?;
        }
        if self.position(4) == position {
            return Ok(false);
        }
        let revision = self.revision.checked_add(1).ok_or(PortalError::Capacity)?;
        if let Some(position) = position {
            self.positions.insert(4, position);
        } else {
            self.positions.remove(&4);
        }
        self.revision = revision;
        Ok(true)
    }
    pub fn tied_summoned(&self, slot: u16) -> bool {
        self.summoned.contains(&slot)
    }
    pub fn positions(&self) -> impl Iterator<Item = (u16, PortalPosition)> + '_ {
        self.positions.iter().map(|(&k, &v)| (k, v))
    }
    pub fn templates(&self) -> impl Iterator<Item = (u16, u32)> + '_ {
        self.templates.iter().map(|(&k, &v)| (k, v))
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn position(&self, slot: u16) -> Option<PortalPosition> {
        self.positions.get(&slot).copied()
    }
    pub fn template(&self, slot: u16) -> Option<u32> {
        self.templates.get(&slot).copied()
    }
    pub fn propose_link(
        &self,
        index: u32,
        anchor: &PortalAnchor,
        access: PortalAccess,
    ) -> Result<PortalLinkMutation, PortalError> {
        anchor.validate()?;
        anchor.check(access)?;
        if access.olthoi {
            return Err(PortalError::OlthoiRestricted);
        }
        let (slot, after, data_id) = match index {
            1 if anchor.kind == PortalKind::Lifestone => (15, anchor.position, None),
            2 | 3 if anchor.kind == PortalKind::Portal => {
                if anchor.no_tie || anchor.restrictions & 0x20 != 0 {
                    return Err(PortalError::NoTie);
                }
                let slot = if index == 2 { 8 } else { 16 };
                let did = if index == 2 { 31 } else { 48 };
                (
                    slot,
                    anchor.destination.ok_or(PortalError::NoLink)?,
                    Some((
                        did,
                        self.template(did),
                        anchor.original_template.unwrap_or(anchor.template),
                    )),
                )
            }
            _ => return Err(PortalError::WrongKind),
        };
        Ok(PortalLinkMutation {
            before_revision: self.revision,
            after_revision: self.revision.checked_add(1).ok_or(PortalError::Capacity)?,
            position_slot: slot,
            before: self.position(slot),
            after,
            data_id,
            tied_summoned: anchor.original_template.is_some(),
        })
    }
    pub fn adopt(&mut self, change: &PortalLinkMutation) -> Result<(), PortalError> {
        let valid_did = match change.position_slot {
            4 | 15 => change.data_id.is_none(),
            8 => change
                .data_id
                .is_some_and(|(slot, _, value)| slot == 31 && value != 0),
            16 => change
                .data_id
                .is_some_and(|(slot, _, value)| slot == 48 && value != 0),
            _ => false,
        };
        if !valid_did
            || change.before_revision != self.revision
            || change.after_revision != self.revision.checked_add(1).ok_or(PortalError::Capacity)?
            || self.position(change.position_slot) != change.before
            || change
                .data_id
                .is_some_and(|(slot, before, _)| self.template(slot) != before)
        {
            return Err(PortalError::Conflict);
        }
        change.after.validate()?;
        self.positions.insert(change.position_slot, change.after);
        if let Some((slot, _, value)) = change.data_id {
            self.templates.insert(slot, value);
            self.summoned_presence.insert(slot);
            if change.tied_summoned {
                self.summoned.insert(slot);
            } else {
                self.summoned.remove(&slot);
            }
        }
        self.revision = change.after_revision;
        Ok(())
    }
    pub fn recall(&self, index: u32) -> Result<PortalPosition, PortalError> {
        let slot = match index {
            1 => 4,
            2 => 15,
            3 => 9,
            4 => 8,
            5 => 16,
            _ => return Err(PortalError::Invalid),
        };
        self.position(slot).ok_or(PortalError::NoLink)
    }
}
