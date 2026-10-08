//! Pinned ACE Player_Allegiance officer permissions and metadata operations.
use crate::{AllegiancePatch, AllegianceRegistry};
use bace_gameplay_api::social::{AllegianceSanctuary, SocialError as E};
use bace_types::EntityId;
#[derive(Clone, Debug)]
pub enum AllegianceManagement {
    Name(Option<String>),
    Motd {
        message: Option<String>,
        set_by: String,
    },
    Officer {
        actor: EntityId,
        level: Option<u32>,
    },
    ClearOfficers,
    OfficerTitle {
        level: u32,
        title: String,
    },
    ClearOfficerTitles,
    Lock(bool),
    ToggleLock,
    Approve(EntityId),
    ClearApproved,
    Ban {
        character: EntityId,
        name: String,
        enabled: bool,
    },
    ChatGag {
        actor: EntityId,
        until: Option<i64>,
    },
    Sanctuary(AllegianceSanctuary),
}
impl AllegianceRegistry {
    pub fn propose_management(
        &self,
        actor: EntityId,
        request: AllegianceManagement,
    ) -> Result<AllegiancePatch, E> {
        let node = self.node(actor).ok_or(E::NotMember)?;
        let before = self.metadata(node.monarch).ok_or(E::Missing)?;
        let mut after = before.clone();
        let permission = before.permission(actor);
        let required = match &request {
            AllegianceManagement::Motd { .. } | AllegianceManagement::ChatGag { .. } => 1,
            AllegianceManagement::ClearOfficers => 4,
            AllegianceManagement::Ban { .. }
            | AllegianceManagement::Officer { .. }
            | AllegianceManagement::Lock(_)
            | AllegianceManagement::ToggleLock
            | AllegianceManagement::ClearApproved
            | AllegianceManagement::Sanctuary(_) => 2,
            _ => 3,
        };
        if permission < required {
            return Err(E::Forbidden);
        }
        match request {
            AllegianceManagement::Name(value) => {
                if value.as_ref().is_some_and(|s| s.len() > 1024) {
                    return Err(E::Capacity);
                }
                after.name = value;
            }
            AllegianceManagement::Motd { message, set_by } => {
                if message.as_ref().is_some_and(|s| s.len() > 4096) || set_by.len() > 256 {
                    return Err(E::Capacity);
                }
                after.motd = message;
                after.motd_set_by = Some(set_by);
            }
            AllegianceManagement::Officer {
                actor: target,
                level,
            } => {
                if target == node.monarch
                    || self.node(target).is_none_or(|n| n.monarch != node.monarch)
                {
                    return Err(E::Forbidden);
                }
                if level.is_some_and(|r| !(1..=3).contains(&r)) {
                    return Err(E::Invalid);
                }
                if permission == 2
                    && (level.unwrap_or(0) > 1
                        || before.officers.get(&target).copied().unwrap_or(0) > 1)
                {
                    return Err(E::Forbidden);
                }
                if let Some(level) = level {
                    after.officers.insert(target, level);
                } else {
                    after.officers.remove(&target);
                }
            }
            AllegianceManagement::ClearOfficers => after.officers.clear(),
            AllegianceManagement::OfficerTitle { level, title } => {
                if !(1..=3).contains(&level) || title.len() > 256 {
                    return Err(E::Invalid);
                }
                after.officer_titles[level as usize - 1] = Some(title);
            }
            AllegianceManagement::ClearOfficerTitles => after.officer_titles = Default::default(),
            AllegianceManagement::Lock(value) => after.locked = value,
            AllegianceManagement::ToggleLock => after.locked = !after.locked,
            AllegianceManagement::Approve(target) => {
                if self
                    .node(target)
                    .is_some_and(|n| n.patron.is_some() || !n.vassals.is_empty())
                {
                    return Err(E::Duplicate);
                }
                if after.approved.len() >= 1024 {
                    return Err(E::Capacity);
                }
                if !after.approved.insert(target) {
                    return Err(E::Duplicate);
                }
            }
            AllegianceManagement::ClearApproved => after.approved.clear(),
            AllegianceManagement::Ban {
                character,
                name,
                enabled,
            } => {
                if character.0 == 0 || character == node.monarch || name.len() > 100 {
                    return Err(E::Invalid);
                }
                if enabled {
                    if after.banned_characters.len() >= 1024 {
                        return Err(E::Capacity);
                    }
                    after.banned_characters.insert(character, name);
                } else {
                    after.banned_characters.remove(&character);
                }
            }
            AllegianceManagement::ChatGag {
                actor: target,
                until,
            } => {
                if target == actor
                    || target == node.monarch
                    || self.node(target).is_none_or(|n| n.monarch != node.monarch)
                {
                    return Err(E::Forbidden);
                }
                if before.chat_gags.get(&target) == Some(&i64::MAX) {
                    return Err(E::Duplicate);
                }
                if until.is_none() && !before.chat_gags.contains_key(&target) {
                    return Err(E::Missing);
                }
                if let Some(until) = until {
                    after.chat_gags.insert(target, until);
                } else {
                    after.chat_gags.remove(&target);
                }
            }
            AllegianceManagement::Sanctuary(position) => {
                let norm: f32 = position.rotation.iter().map(|v| v * v).sum();
                if position.cell == 0
                    || position
                        .origin
                        .iter()
                        .chain(position.rotation.iter())
                        .any(|x| !x.is_finite())
                    || (norm - 1.0).abs() > 0.001
                {
                    return Err(E::Invalid);
                }
                after.sanctuary = Some(position);
            }
        }
        let mut patch = self.patch()?;
        patch.metadata.push((Some(before.clone()), Some(after)));
        Ok(patch)
    }
}
impl AllegianceRegistry {
    /// AccountBoot is ignored by the pinned ACE handler; the selected character's subtree splits.
    pub fn propose_boot(
        &self,
        actor: EntityId,
        target: EntityId,
        ban: Option<String>,
        chat_room: u32,
    ) -> Result<AllegiancePatch, E> {
        let source = self.node(actor).ok_or(E::NotMember)?;
        let target_node = self.node(target).ok_or(E::Missing)?;
        if self.permission(actor) < 2
            || target == actor
            || target == source.monarch
            || target_node.monarch != source.monarch
        {
            return Err(E::Forbidden);
        }
        let patron = target_node.patron.ok_or(E::Invalid)?;
        let mut patch = self.propose_break(patron, target, chat_room)?;
        if let Some(name) = ban {
            if name.len() > 100 {
                return Err(E::Invalid);
            }
            let after = patch
                .metadata
                .iter_mut()
                .filter_map(|(_, after)| after.as_mut())
                .find(|m| m.monarch == source.monarch)
                .ok_or(E::Missing)?;
            if after.banned_characters.len() >= 1024 {
                return Err(E::Capacity);
            }
            if after.banned_characters.insert(target, name).is_some() {
                return Err(E::Duplicate);
            }
        }
        Ok(patch)
    }
}
