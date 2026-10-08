//! Immutable referenced portal qualities are distinct from a live world anchor.
//! Shared access policy remains the existing pinned portal check order.
use crate::{PortalAccess, PortalAnchor, PortalError, PortalKind, PortalPosition};
#[derive(Clone, Debug, PartialEq)]
pub struct PortalTemplate {
    pub template: u32,
    pub original_template: Option<u32>,
    pub kind: PortalKind,
    pub destination: Option<PortalPosition>,
    pub minimum_level: u32,
    pub maximum_level: u32,
    pub restrictions: u32,
    pub no_tie: bool,
    pub ignore_pk_timer: bool,
    pub account_requirement: u32,
}
impl PortalTemplate {
    pub fn validate(&self) -> Result<(), PortalError> {
        if self.template == 0 || self.original_template == Some(0) || self.restrictions > 0x3ff {
            return Err(PortalError::Invalid);
        }
        if let Some(destination) = self.destination {
            destination.validate()?;
        }
        Ok(())
    }
    pub fn instantiate(
        &self,
        entity: u32,
        position: PortalPosition,
    ) -> Result<PortalAnchor, PortalError> {
        self.validate()?;
        let anchor = PortalAnchor {
            entity,
            template: self.template,
            original_template: self.original_template,
            kind: self.kind,
            position,
            destination: self.destination,
            minimum_level: self.minimum_level,
            maximum_level: self.maximum_level,
            restrictions: self.restrictions,
            no_tie: self.no_tie,
            ignore_pk_timer: self.ignore_pk_timer,
            account_requirement: self.account_requirement,
        };
        anchor.validate()?;
        Ok(anchor)
    }
    pub fn check(&self, access: PortalAccess) -> Result<(), PortalError> {
        if access.teleporting {
            return Err(PortalError::Teleporting);
        }
        if access.recently_teleported {
            return Err(PortalError::RecentTeleport);
        }
        if access.pk_recent && !self.ignore_pk_timer {
            return Err(PortalError::PkRecent);
        }
        if self.kind == PortalKind::Portal && self.destination.is_none() {
            return Err(PortalError::NoLink);
        }
        if !access.ignore_restrictions {
            if access.level < self.minimum_level {
                return Err(PortalError::TooLow);
            }
            if access.enforce_maximum_level
                && self.maximum_level != 0
                && access.level > self.maximum_level
            {
                return Err(PortalError::TooHigh);
            }
            if self.kind == PortalKind::Portal && self.restrictions == 0 {
                return Err(PortalError::Protected);
            }
            if self.restrictions & 2 != 0 && access.pk_status == 4
                || self.restrictions & 4 != 0 && access.pk_status == 64
                || self.restrictions & 8 != 0 && access.pk_status == 2
            {
                return Err(PortalError::PkRestricted);
            }
            if self.restrictions & 0x40 != 0 && !access.olthoi
                || self.restrictions & 0x80 != 0 && access.olthoi
            {
                return Err(PortalError::OlthoiRestricted);
            }
            if self.restrictions & 0x100 != 0 && access.vitae {
                return Err(PortalError::Vitae);
            }
            if self.restrictions & 0x200 != 0 && !access.account_15_days {
                return Err(PortalError::NewAccount);
            }
            if access.entitlement < self.account_requirement {
                return Err(PortalError::Entitlement);
            }
        }
        if !access.quest_allowed {
            return Err(PortalError::Quest);
        }
        Ok(())
    }
}
