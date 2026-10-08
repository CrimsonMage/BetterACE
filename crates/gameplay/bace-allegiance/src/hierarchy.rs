//! ACE hierarchy changes use bounded indexed paths; GDLE oath counters reset at swear.
use crate::{AllegianceMetadata, AllegianceNode, AllegiancePatch, AllegianceRegistry};
use bace_gameplay_api::social::SocialError as E;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
impl AllegianceRegistry {
    pub fn restore(
        nodes: Vec<AllegianceNode>,
        groups: Vec<AllegianceMetadata>,
        revision: u64,
        capacity: usize,
    ) -> Result<Self, E> {
        let mut owner = Self::new(capacity)?;
        if nodes.len() > capacity {
            return Err(E::Capacity);
        }
        for node in nodes {
            node.validate()?;
            if owner.nodes.insert(node.character, node).is_some() {
                return Err(E::Duplicate);
            }
        }
        for group in groups {
            if owner.groups.insert(group.monarch, group).is_some() {
                return Err(E::Duplicate);
            }
        }
        owner.validate_forest()?;
        owner.revision = revision;
        Ok(owner)
    }
    pub fn propose_swear(
        &self,
        source: AllegianceNode,
        patron: AllegianceNode,
        now: u64,
        chat_room: u32,
    ) -> Result<AllegiancePatch, E> {
        source.validate()?;
        patron.validate()?;
        let actor = source.character;
        let target = patron.character;
        if actor == target {
            return Err(E::Invalid);
        }
        let mut source = self.node(actor).cloned().unwrap_or(source);
        let mut patron = self.node(target).cloned().unwrap_or(patron);
        if source.patron.is_some() {
            return Err(E::Duplicate);
        }
        if patron.monarch == source.character {
            return Err(E::Invalid);
        }
        if patron.vassals.len() >= 11 {
            return Err(E::Full);
        }
        let existing = self.metadata(patron.monarch);
        if let Some(group) = existing {
            if group.banned_characters.contains_key(&source.character) {
                return Err(E::Forbidden);
            }
            if group.locked
                && !group.approved.contains(&actor)
                && group.officers.get(&target).copied().unwrap_or(0) < 3
            {
                return Err(E::Locked);
            }
        }
        let mut patch = self.patch()?;
        let mut staged = BTreeMap::new();
        let old_monarch = source.monarch;
        source.patron = Some(target);
        source.sworn_at = now;
        source.online_seconds = 1;
        source.may_pass_up = patron.level >= source.level;
        source.monarch = patron.monarch;
        patron.vassals.push(actor);
        patron.vassals.sort();
        staged.insert(actor, source);
        staged.insert(target, patron.clone());
        let mut todo = staged[&actor].vassals.clone();
        let mut seen = BTreeSet::from([actor]);
        while let Some(id) = todo.pop() {
            if !seen.insert(id) || seen.len() > self.capacity {
                return Err(E::Invalid);
            }
            let mut node = self.node(id).cloned().ok_or(E::Missing)?;
            node.monarch = patron.monarch;
            todo.extend(node.vassals.iter().copied());
            staged.insert(id, node);
        }
        self.recompute_ancestors(target, &mut staged)?;
        patch.nodes = staged
            .into_iter()
            .map(|(id, after)| (self.node(id).cloned(), Some(after)))
            .collect();
        if old_monarch != patron.monarch
            && let Some(group) = self.metadata(old_monarch)
        {
            patch.metadata.push((Some(group.clone()), None));
        }
        if existing.is_none() {
            patch.metadata.push((
                None,
                Some(AllegianceMetadata::new(patron.monarch, chat_room)),
            ));
        }
        self.validate_patch(&patch)?;
        Ok(patch)
    }
    pub fn propose_break(
        &self,
        actor: EntityId,
        target: EntityId,
        chat_room: u32,
    ) -> Result<AllegiancePatch, E> {
        let source = self.node(actor).ok_or(E::Missing)?;
        let target_node = self.node(target).ok_or(E::Missing)?;
        let (patron_id, vassal_id) = if source.patron == Some(target) {
            (target, actor)
        } else if target_node.patron == Some(actor) {
            (actor, target)
        } else {
            return Err(E::Forbidden);
        };
        let mut patron = self.node(patron_id).ok_or(E::Missing)?.clone();
        let mut vassal = self.node(vassal_id).ok_or(E::Missing)?.clone();
        patron.vassals.retain(|id| *id != vassal_id);
        vassal.patron = None;
        vassal.monarch = vassal_id;
        let mut staged = BTreeMap::from([(patron_id, patron), (vassal_id, vassal)]);
        let mut todo = staged[&vassal_id].vassals.clone();
        let mut seen = BTreeSet::from([vassal_id]);
        while let Some(id) = todo.pop() {
            if !seen.insert(id) || seen.len() > self.capacity {
                return Err(E::Invalid);
            }
            let mut node = self.node(id).cloned().ok_or(E::Missing)?;
            node.monarch = vassal_id;
            todo.extend(node.vassals.iter().copied());
            staged.insert(id, node);
        }
        self.recompute_ancestors(patron_id, &mut staged)?;
        let mut patch = self.patch()?;
        patch.nodes = staged
            .into_iter()
            .map(|(id, after)| (self.node(id).cloned(), Some(after)))
            .collect();
        let before = self.metadata(source.monarch).ok_or(E::Missing)?;
        let mut after = before.clone();
        after.officers.retain(|id, _| !seen.contains(id));
        after.chat_gags.retain(|id, _| !seen.contains(id));
        patch.metadata.push((Some(before.clone()), Some(after)));
        patch
            .metadata
            .push((None, Some(AllegianceMetadata::new(vassal_id, chat_room))));
        self.validate_patch(&patch)?;
        Ok(patch)
    }
    fn recompute_ancestors(
        &self,
        mut actor: EntityId,
        staged: &mut BTreeMap<EntityId, AllegianceNode>,
    ) -> Result<(), E> {
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(actor) || seen.len() > self.capacity {
                return Err(E::Invalid);
            }
            let mut node = staged
                .get(&actor)
                .or_else(|| self.node(actor))
                .cloned()
                .ok_or(E::Missing)?;
            let mut highest = 0;
            let mut second = 0;
            let mut followers = 0u32;
            for id in &node.vassals {
                let child = staged
                    .get(id)
                    .or_else(|| self.node(*id))
                    .ok_or(E::Missing)?;
                followers = followers
                    .checked_add(child.followers)
                    .and_then(|v| v.checked_add(1))
                    .ok_or(E::Overflow)?;
                let rank = child.rank;
                if rank >= highest {
                    second = highest;
                    highest = rank;
                } else {
                    second = second.max(rank);
                }
            }
            node.rank = highest.max(second + 1).min(10);
            node.followers = followers;
            let parent = node.patron;
            staged.insert(actor, node);
            let Some(parent) = parent else {
                break;
            };
            actor = parent;
        }
        Ok(())
    }
}
