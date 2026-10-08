//! Shared reward preparation: fellowship first, pinned GDLE allegiance pass-up second.
use super::Kernel;
use bace_gameplay_api::social::{EarnedExperience, RewardXpKind, SocialError as E};
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
impl Kernel {
    pub fn configure_social_experience(
        &mut self,
        table: std::sync::Arc<bace_character::CharacterLevelTable>,
    ) -> Result<(), E> {
        if self.allegiances.pending.is_some() || !self.allegiances.cached_skills.is_empty() {
            return Err(E::Busy);
        }
        self.allegiances.level_table = Some(table);
        self.npcs.shared_experience = true;
        Ok(())
    }
    pub fn prepare_shared_experience(
        &mut self,
        reward: EarnedExperience,
    ) -> Result<crate::AllegianceTicket, E> {
        self.prepare_shared_experience_batch(&[reward], None)
    }
    pub(in crate::kernel) fn prepare_shared_experience_batch(
        &mut self,
        rewards: &[EarnedExperience],
        rare: Option<bace_gameplay_api::RareDecision>,
    ) -> Result<crate::AllegianceTicket, E> {
        if self.allegiances.pending.is_some() || !self.allegiances.cached_skills.is_empty() {
            return Err(E::Busy);
        }
        if rewards.is_empty()
            || rewards.len() > 256
            || rewards.iter().any(|r| {
                r.amount > i64::MAX as u64 || r.sharing.allegiance != rewards[0].sharing.allegiance
            })
        {
            return Err(E::Invalid);
        }
        let mut shares = Vec::new();
        let mut quest_messages = Vec::new();
        let mut item_amounts = BTreeMap::<EntityId, u64>::new();
        let mut normal_amounts = BTreeMap::<EntityId, u64>::new();
        for reward in rewards {
            let expanded = self.shared_experience_amounts(*reward)?;
            if reward.kind != RewardXpKind::Allegiance {
                for &(actor, amount) in &expanded {
                    let n = normal_amounts.entry(actor).or_default();
                    *n = n.checked_add(amount).ok_or(E::Overflow)?;
                }
            }
            if matches!(reward.kind, RewardXpKind::Kill | RewardXpKind::Quest) {
                let amount = expanded
                    .iter()
                    .find(|(id, _)| *id == reward.source)
                    .map_or(0, |(_, n)| *n);
                let n = item_amounts.entry(reward.source).or_default();
                *n = n.checked_add(amount).ok_or(E::Overflow)?;
            }
            if reward.kind == RewardXpKind::Quest {
                quest_messages.push((
                    reward.source,
                    expanded
                        .iter()
                        .find(|(id, _)| *id == reward.source)
                        .map_or(0, |(_, amount)| *amount),
                ));
            }
            shares.extend(expanded);
        }
        if shares.len() > 1024 {
            return Err(E::Capacity);
        }
        if let Some(r) = &rare {
            self.characters
                .validate_social_rare(r)
                .map_err(|_| E::Stale)?;
            if !shares.iter().any(|(id, _)| id.0 == r.character) {
                shares.push((EntityId(r.character), 0));
            }
        }
        let online: BTreeSet<_> = self.social.directory.online().collect();
        let mut participants: BTreeSet<_> = shares.iter().map(|(id, _)| *id).collect();
        if rewards[0].sharing.allegiance {
            for (actor, _) in &shares {
                let mut cursor = *actor;
                let mut seen = BTreeSet::new();
                while let Some(node) = self.allegiances.registry.node(cursor) {
                    if !online.contains(&cursor) {
                        break;
                    }
                    if !seen.insert(cursor) {
                        return Err(E::Invalid);
                    }
                    let Some(parent) = node.patron else {
                        break;
                    };
                    if online.contains(&parent) {
                        participants.insert(parent);
                    }
                    cursor = parent;
                }
            }
        }
        self.reserve_social_registries(&participants)?;
        let result = self
            .prepare_shared_experience_inner(rewards[0], &shares, &online, rare)
            .and_then(|mut ticket| {
                ticket.quest_messages = quest_messages;
                self.attach_experience_supplements(ticket, &item_amounts, &normal_amounts)
            });
        if result.is_err() {
            self.release_social_registries()?;
        }
        result
    }
    fn prepare_shared_experience_inner(
        &mut self,
        reward: EarnedExperience,
        shares: &[(EntityId, u64)],
        online: &BTreeSet<EntityId>,
        rare: Option<bace_gameplay_api::RareDecision>,
    ) -> Result<crate::AllegianceTicket, E> {
        let table = self
            .allegiances
            .level_table
            .as_ref()
            .ok_or(E::Missing)?
            .clone();
        let mut amounts = BTreeMap::<EntityId, u64>::new();
        let now = u64::try_from(self.social_now()?).map_err(|_| E::Invalid)?;
        let (mut patch, credits) = if reward.sharing.allegiance {
            let proposal = self.allegiances.registry.propose_passup_awards(
                shares,
                now,
                online,
                |actor, amount, _| {
                    let entry = amounts.entry(actor).or_default();
                    *entry = entry.checked_add(amount).ok_or(E::Overflow)?;
                    self.characters
                        .earned_experience(actor, &table, *entry)
                        .map(|change| change.services.after.level)
                        .map_err(|_| E::Invalid)
                },
            )?;
            (proposal.patch, proposal.credits)
        } else {
            for &(actor, amount) in shares {
                let entry = amounts.entry(actor).or_default();
                *entry = entry.checked_add(amount).ok_or(E::Overflow)?;
            }
            (self.allegiances.registry.patch()?, Vec::new())
        };
        let changes: Vec<_> = amounts
            .into_iter()
            .map(|(actor, amount)| {
                self.characters
                    .earned_experience(actor, &table, amount)
                    .map(|change| (actor, change))
                    .map_err(|_| E::Invalid)
            })
            .collect::<Result<_, _>>()?;
        for (actor, change) in &changes {
            if let Some(before) = self.allegiances.registry.node(*actor) {
                if let Some((_, Some(after))) = patch.nodes.iter_mut().find(|(b, a)| {
                    b.as_ref()
                        .or(a.as_ref())
                        .is_some_and(|n| n.character == *actor)
                }) {
                    after.level = change.services.after.level;
                } else if before.level != change.services.after.level {
                    let mut after = before.clone();
                    after.level = change.services.after.level;
                    patch.nodes.push((Some(before.clone()), Some(after)));
                }
            }
        }
        if let Some(r) = &rare {
            let c = &changes
                .iter()
                .find(|(id, _)| id.0 == r.character)
                .ok_or(E::Invalid)?
                .1;
            if r.previous != r.next && c.experience.after_revision == u64::MAX {
                return Err(E::Overflow);
            }
        }
        let mut ticket = self.allegiances.propose(reward.source, patch, credits)?;
        if self
            .characters
            .reserve_social_rewards(ticket.operation, &changes, &table)
            .is_err()
        {
            self.allegiances.pending = None;
            return Err(E::Busy);
        }
        ticket.player_changes = changes;
        ticket.rare = rare;
        self.allegiances.pending = Some(ticket.clone());
        Ok(ticket)
    }
    pub fn prepare_allegiance_login_experience(
        &mut self,
        actor: EntityId,
    ) -> Result<Option<crate::AllegianceTicket>, E> {
        if self.allegiances.pending.is_some() {
            return Err(E::Busy);
        }
        let Some(node) = self.allegiances.registry.node(actor) else {
            return Ok(None);
        };
        let amount = node.unclaimed;
        if amount == 0 {
            return Ok(None);
        }
        self.reserve_social_registries(&BTreeSet::from([actor]))?;
        let result = (|| {
            let table = self
                .allegiances
                .level_table
                .as_ref()
                .ok_or(E::Missing)?
                .clone();
            let change = self
                .characters
                .earned_experience(actor, &table, amount)
                .map_err(|_| E::Invalid)?;
            let proposal = self
                .allegiances
                .registry
                .propose_redemption(actor, change.services.after.level)?;
            let mut ticket = self
                .allegiances
                .propose(actor, proposal.patch, proposal.credits)?;
            let changes = vec![(actor, change)];
            if self
                .characters
                .reserve_social_rewards(ticket.operation, &changes, &table)
                .is_err()
            {
                self.allegiances.pending = None;
                return Err(E::Busy);
            }
            ticket.player_changes = changes;
            self.allegiances.pending = Some(ticket.clone());
            Ok(Some(ticket))
        })();
        if result.is_err() {
            self.release_social_registries()?;
        }
        result
    }
    fn shared_experience_amounts(
        &self,
        reward: EarnedExperience,
    ) -> Result<Vec<(EntityId, u64)>, E> {
        if !reward.sharing.fellowship {
            return Ok(vec![(reward.source, reward.amount)]);
        }
        let Some(group) = self.fellowships.membership(reward.source) else {
            return Ok(vec![(reward.source, reward.amount)]);
        };
        let table = self.allegiances.level_table.as_ref().ok_or(E::Missing)?;
        let (source_cell, source) = self
            .world
            .actor_state(reward.source)
            .map_err(|_| E::Missing)?;
        let mut members = Vec::with_capacity(group.members().len());
        for actor in group.members() {
            let state = self.characters.native_services(*actor).ok_or(E::Missing)?;
            let (cell, pose) = self.world.actor_state(*actor).map_err(|_| E::Missing)?;
            let mut d = pose.position() - source.position();
            if cell.0 & 0xffff < 0x100 && source_cell.0 & 0xffff < 0x100 {
                d.x +=
                    (((cell.0 >> 24) & 255) as f32 - ((source_cell.0 >> 24) & 255) as f32) * 192.0;
                d.y +=
                    (((cell.0 >> 16) & 255) as f32 - ((source_cell.0 >> 16) & 255) as f32) * 192.0;
            }
            members.push(bace_fellowship::FellowRewardMember {
                actor: *actor,
                level: state.level,
                xp_to_next_level: table.next_level_experience(state.level),
                indoor: cell.0 & 0xffff >= 0x100,
                landblock: (cell.0 >> 16) as u16,
                distance_2d: (d.x * d.x + d.y * d.y).sqrt(),
            });
        }
        group
            .split_experience(
                reward.amount,
                reward.source,
                reward.kind == RewardXpKind::Quest,
                false,
                50,
                &members,
            )
            .map_err(|_| E::Invalid)
    }
    fn reserve_social_registries(&mut self, actors: &BTreeSet<EntityId>) -> Result<(), E> {
        if !self.allegiances.registries.is_empty() {
            return Err(E::Busy);
        }
        for actor in actors {
            if self.characters.reserved(*actor)
                || self.inventory.reserved(*actor)
                || self.npcs.reserved(*actor)
                || self.housing.reserved(*actor)
                || self.magic.busy(*actor)
                || self.magic.registry_reserved(*actor)
            {
                return Err(E::Busy);
            }
        }
        let now = self.tick as f64 / 30.0;
        self.magic.prepare_registry_time(now).map_err(|_| E::Busy)?;
        self.sync_registry_revisions().map_err(|_| E::Busy)?;
        for actor in actors {
            if self.magic.registry(*actor).is_some() {
                if self.magic.reserve_registry(*actor, true, now).is_err() {
                    self.release_social_registries()?;
                    return Err(E::Busy);
                }
                self.allegiances.registries.push(*actor);
            }
        }
        Ok(())
    }
    pub(in crate::kernel) fn release_social_registries(&mut self) -> Result<(), E> {
        let now = self.tick as f64 / 30.0;
        while let Some(actor) = self.allegiances.registries.last().copied() {
            self.magic
                .reserve_registry(actor, false, now)
                .map_err(|_| E::Busy)?;
            self.allegiances.registries.pop();
        }
        Ok(())
    }
    pub(in crate::kernel) fn step_allegiance_checkpoint(&mut self) -> Result<(), E> {
        self.step_allegiance_cached_skills()?;
        if self.allegiances.pending.is_some() {
            return Ok(());
        }
        let now = self.tick as f64 / 30.0;
        let Some(elapsed) = self.allegiances.clock.due(now)? else {
            return Ok(());
        };
        let online = self.social.directory.online().collect();
        let patch = self
            .allegiances
            .registry
            .propose_online_checkpoint(elapsed, &online)?;
        if patch.nodes.is_empty() {
            self.allegiances.clock.adopt(now)?;
        } else {
            let actor = patch.nodes[0].1.as_ref().ok_or(E::Invalid)?.character;
            self.allegiances.propose(actor, patch, Vec::new())?;
            self.allegiances.checkpoint = Some(now);
        }
        Ok(())
    }
}
