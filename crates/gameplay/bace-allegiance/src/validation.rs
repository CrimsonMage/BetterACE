//! Full-forest cold validation and changed-row validation use indexed IDs, never graph pointers.
use crate::{AllegianceMetadata, AllegianceNode, AllegiancePatch, AllegianceRegistry};
use bace_gameplay_api::social::SocialError as E;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
impl AllegianceMetadata {
    pub fn validate(&self) -> Result<(), E> {
        if self.monarch.0 == 0
            || self.chat_room == 0
            || self.name.as_ref().is_some_and(|s| s.len() > 1024)
            || self.motd.as_ref().is_some_and(|s| s.len() > 4096)
            || self.motd_set_by.as_ref().is_some_and(|s| s.len() > 256)
            || self.officer_titles.iter().flatten().any(|s| s.len() > 256)
            || self.officers.len() > 4096
            || self.approved.len() > 1024
            || self.banned_characters.len() > 1024
            || self.chat_gags.len() > 4096
        {
            return Err(E::Capacity);
        }
        if self
            .officers
            .iter()
            .any(|(id, rank)| id.0 == 0 || *id == self.monarch || !(1..=3).contains(rank))
            || self.approved.contains(&EntityId(0))
            || self
                .banned_characters
                .iter()
                .any(|(id, name)| id.0 == 0 || name.len() > 100)
            || self.chat_gags.contains_key(&EntityId(0))
        {
            return Err(E::Invalid);
        }
        if let Some(p) = self.sanctuary {
            let norm: f32 = p.rotation.iter().map(|v| v * v).sum();
            if p.cell == 0
                || p.origin
                    .iter()
                    .chain(p.rotation.iter())
                    .any(|v| !v.is_finite())
                || (norm - 1.0).abs() > 0.001
            {
                return Err(E::Invalid);
            }
        }
        Ok(())
    }
}
fn cached(
    node: &AllegianceNode,
    mut get: impl FnMut(EntityId) -> Option<(u32, u32)>,
) -> Result<(), E> {
    let mut highest = 0;
    let mut second = 0;
    let mut followers = 0u32;
    for id in &node.vassals {
        let (rank, count) = get(*id).ok_or(E::Missing)?;
        followers = followers
            .checked_add(count)
            .and_then(|v| v.checked_add(1))
            .ok_or(E::Overflow)?;
        if rank >= highest {
            second = highest;
            highest = rank;
        } else {
            second = second.max(rank);
        }
    }
    if node.rank != highest.max(second + 1).min(10) || node.followers != followers {
        return Err(E::Invalid);
    }
    Ok(())
}
impl AllegianceRegistry {
    pub(crate) fn validate_forest(&self) -> Result<(), E> {
        let mut rooms = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        for group in self.groups.values() {
            group.validate()?;
            if !rooms.insert(group.chat_room) {
                return Err(E::Duplicate);
            }
            let root = self.node(group.monarch).ok_or(E::Missing)?;
            if root.patron.is_some() || root.monarch != root.character {
                return Err(E::Invalid);
            }
            stack.push((root.character, false));
        }
        while let Some((id, exit)) = stack.pop() {
            let node = self.node(id).ok_or(E::Missing)?;
            if exit {
                cached(node, |id| self.node(id).map(|n| (n.rank, n.followers)))?;
                continue;
            }
            if !visited.insert(id) {
                return Err(E::Invalid);
            }
            stack.push((id, true));
            for child in &node.vassals {
                let child_node = self.node(*child).ok_or(E::Missing)?;
                if child_node.patron != Some(id) || child_node.monarch != node.monarch {
                    return Err(E::Invalid);
                }
                stack.push((*child, false));
            }
        }
        if visited.len() != self.nodes.len() {
            return Err(E::Invalid);
        }
        for group in self.groups.values() {
            for id in group.officers.keys().chain(group.chat_gags.keys()) {
                if self.node(*id).is_none_or(|n| n.monarch != group.monarch) {
                    return Err(E::Invalid);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn validate_patch_links(&self, patch: &AllegiancePatch) -> Result<(), E> {
        let nodes: BTreeMap<_, _> = patch
            .nodes
            .iter()
            .map(|(before, after)| {
                (
                    before
                        .as_ref()
                        .or(after.as_ref())
                        .expect("validated patch identity")
                        .character,
                    after.as_ref(),
                )
            })
            .collect();
        let groups: BTreeMap<_, _> = patch
            .metadata
            .iter()
            .map(|(before, after)| {
                (
                    before
                        .as_ref()
                        .or(after.as_ref())
                        .expect("validated metadata identity")
                        .monarch,
                    after.as_ref(),
                )
            })
            .collect();
        let node = |id| nodes.get(&id).copied().unwrap_or_else(|| self.node(id));
        let group = |id| {
            groups
                .get(&id)
                .copied()
                .unwrap_or_else(|| self.metadata(id))
        };
        for (before, after) in &patch.nodes {
            let Some(n) = after else {
                if before
                    .as_ref()
                    .is_some_and(|n| n.patron.is_some() || !n.vassals.is_empty())
                {
                    return Err(E::Invalid);
                }
                continue;
            };
            if group(n.monarch).is_none() {
                return Err(E::Missing);
            }
            if let Some(parent) = n.patron {
                let p = node(parent).ok_or(E::Missing)?;
                if p.monarch != n.monarch || !p.vassals.contains(&n.character) {
                    return Err(E::Invalid);
                }
            } else if n.monarch != n.character {
                return Err(E::Invalid);
            }
            for child in &n.vassals {
                if node(*child)
                    .is_none_or(|c| c.patron != Some(n.character) || c.monarch != n.monarch)
                {
                    return Err(E::Invalid);
                }
            }
            cached(n, |id| node(id).map(|n| (n.rank, n.followers)))?;
            if before.as_ref().is_none_or(|old| old.patron != n.patron) {
                let mut cursor = n.character;
                let mut seen = BTreeSet::new();
                loop {
                    if !seen.insert(cursor) {
                        return Err(E::Invalid);
                    }
                    let current = node(cursor).ok_or(E::Missing)?;
                    if let Some(parent) = current.patron {
                        cursor = parent;
                    } else {
                        if cursor != n.monarch {
                            return Err(E::Invalid);
                        }
                        break;
                    }
                }
            }
        }
        let mut new_rooms = BTreeSet::new();
        for (_, after) in &patch.metadata {
            if let Some(m) = after {
                m.validate()?;
                if !new_rooms.insert(m.chat_room)
                    || self.groups.values().any(|old| {
                        old.monarch != m.monarch
                            && old.chat_room == m.chat_room
                            && !groups.contains_key(&old.monarch)
                    })
                {
                    return Err(E::Duplicate);
                }
                if node(m.monarch).is_none_or(|n| n.patron.is_some()) {
                    return Err(E::Invalid);
                }
                for id in m.officers.keys().chain(m.chat_gags.keys()) {
                    if node(*id).is_none_or(|n| n.monarch != m.monarch) {
                        return Err(E::Invalid);
                    }
                }
            }
        }
        Ok(())
    }
}
