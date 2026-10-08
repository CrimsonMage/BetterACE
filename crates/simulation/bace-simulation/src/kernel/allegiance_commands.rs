//! Pinned ACE allegiance requests. Valuable transitions are staged for the database owner.
use super::Kernel;
use bace_allegiance::{AllegianceManagement as M, AllegianceNode};
use bace_gameplay_api::{
    ActionContext,
    social::{
        AllegianceNodeSnapshot, AllegianceProfileSnapshot, AllegianceRequest as R,
        SocialError as E, SocialEvent, SocialOutcome,
    },
};
use bace_types::EntityId;
impl Kernel {
    pub fn supply_allegiance_id(&mut self, id: u32) -> Result<(), E> {
        if !(0x80000000..=0xfffffffe).contains(&id)
            || self.allegiances.ids.contains(&id)
            || self
                .allegiances
                .registry
                .groups()
                .any(|m| m.chat_room == id)
        {
            return Err(E::Invalid);
        }
        if self.allegiances.ids.len() >= self.allegiances.capacity {
            return Err(E::Capacity);
        }
        self.allegiances.ids.push_back(id);
        Ok(())
    }
    pub fn request_allegiance(&mut self, context: ActionContext, request: R) -> Result<(), E> {
        if self.social.outcomes.len() >= self.social.capacity
            || self.social.events.len().saturating_add(4) > self.social.capacity
        {
            return Err(E::Capacity);
        }
        if self.allegiances.pending.is_some() {
            return Err(E::Busy);
        }
        let authorization = self.authorize_social(context);
        let retryable = authorization == Err(E::Busy);
        let result = authorization.and_then(|()| self.apply_allegiance(context.actor, request));
        // Pending durable requests acknowledge only after the exact receipt.
        if self.allegiances.pending.is_some() && result.is_ok() {
            self.allegiances.context = Some(context);
        } else {
            self.social.outcomes.push_back(SocialOutcome {
                context,
                result,
                retryable,
            });
        }
        result
    }
    fn allegiance_named(&self, name: &str) -> Result<EntityId, E> {
        self.social
            .directory
            .by_name(name)
            .map(|p| p.identity.character)
            .or_else(|| {
                self.allegiances
                    .registry
                    .nodes()
                    .find(|n| n.name.eq_ignore_ascii_case(name))
                    .map(|n| n.character)
            })
            .ok_or(E::Missing)
    }
    fn allegiance_new_node(&self, actor: EntityId) -> Result<AllegianceNode, E> {
        let p = self
            .social
            .directory
            .presence(actor)
            .filter(|p| p.online)
            .ok_or(E::Offline)?;
        let state = self.characters.native_services(actor).ok_or(E::Missing)?;
        let properties = self.world.properties(actor).ok_or(E::Missing)?;
        let get = |id| match properties.get(bace_entity::PropertyFamily::Int, id) {
            Some(bace_entity::PropertyValue::Int(value)) => {
                u8::try_from(*value).map_err(|_| E::Invalid)
            }
            _ => Err(E::Missing),
        };
        let skills = self.combat.skills.get(&actor).ok_or(E::Missing)?;
        let skill = |id| skills.values.get(&id).map(|s| s.current).ok_or(E::Missing);
        if let Some(node) = self.allegiances.registry.node(actor) {
            let mut node = node.clone();
            node.level = state.level;
            node.leadership = skill(35)?;
            node.loyalty = skill(36)?;
            return Ok(node);
        }
        Ok(AllegianceNode {
            character: actor,
            account: p.identity.account,
            name: p.identity.name.clone(),
            gender: get(113)?,
            heritage: get(188)?,
            patron: None,
            monarch: actor,
            vassals: Vec::new(),
            rank: 1,
            followers: 0,
            level: state.level,
            leadership: skill(35)?,
            loyalty: skill(36)?,
            sworn_at: 0,
            online_seconds: 1,
            may_pass_up: false,
            received_total: 0,
            tithed_total: 0,
            unclaimed: 0,
        })
    }
    fn apply_allegiance(&mut self, actor: EntityId, request: R) -> Result<(), E> {
        let now = self.social_now()?;
        match request {
            R::Swear(patron) => {
                if !self.social_in_range(actor, patron, 2.0) {
                    return Err(E::Invalid);
                }
                let source = self.allegiance_new_node(actor)?;
                let target = self.allegiance_new_node(patron)?;
                let room = self.allegiances.ids.front().copied().ok_or(E::Capacity)?;
                self.allegiances
                    .registry
                    .propose_swear(source, target, now as u64, room)?;
                self.queue_social_confirmation(
                    patron,
                    crate::social::SocialConfirmation::Allegiance {
                        vassal: actor,
                        patron,
                        revision: self.allegiances.registry.revision(),
                    },
                    1,
                    format!(
                        "{} wishes to swear allegiance to you.",
                        self.social
                            .directory
                            .presence(actor)
                            .ok_or(E::Missing)?
                            .identity
                            .name
                    ),
                )?;
                Ok(())
            }
            R::Confirm { token, accepted } => {
                let pending = self
                    .social
                    .confirmations
                    .get(&actor)
                    .ok_or(E::Stale)?
                    .clone();
                if pending.token != token || self.tick > pending.expires_tick {
                    return Err(E::Stale);
                }
                let crate::social::SocialConfirmation::Allegiance {
                    vassal,
                    patron,
                    revision,
                } = pending.confirmation
                else {
                    return Err(E::Invalid);
                };
                if patron != actor {
                    return Err(E::Forbidden);
                }
                self.social.confirmations.remove(&actor);
                if !accepted {
                    return Err(E::Declined);
                }
                if revision != self.allegiances.registry.revision()
                    || !self.social_in_range(vassal, patron, 2.0)
                {
                    return Err(E::Stale);
                }
                let source = self.allegiance_new_node(vassal)?;
                let target = self.allegiance_new_node(patron)?;
                let room = self.allegiances.ids.front().copied().ok_or(E::Capacity)?;
                let patch = self
                    .allegiances
                    .registry
                    .propose_swear(source, target, now as u64, room)?;
                let creates = patch
                    .metadata
                    .iter()
                    .any(|(before, after)| before.is_none() && after.is_some());
                self.allegiances.propose(actor, patch, Vec::new())?;
                if creates {
                    self.allegiances.ids.pop_front();
                }
                Ok(())
            }
            R::Break(target) => {
                let room = self.allegiances.ids.front().copied().ok_or(E::Capacity)?;
                let patch = self
                    .allegiances
                    .registry
                    .propose_break(actor, target, room)?;
                self.allegiances.propose(actor, patch, Vec::new())?;
                self.allegiances.ids.pop_front();
                Ok(())
            }
            R::Update { enabled } => {
                if enabled {
                    self.allegiances.listeners.insert(actor);
                    self.emit_allegiance(actor)
                } else {
                    self.allegiances.listeners.remove(&actor);
                    Ok(())
                }
            }
            R::Info(name) => {
                if self.allegiances.registry.permission(actor) < 2 {
                    return Err(E::Forbidden);
                }
                let target = self.allegiance_named(&name)?;
                let relation = self.allegiance_relation(actor).ok_or(E::NotMember)?;
                if self
                    .allegiance_relation(target)
                    .is_none_or(|r| r.monarch != relation.monarch)
                {
                    return Err(E::NotMember);
                }
                let profile = self.allegiance_profile(target)?;
                self.social.events.push_back(SocialEvent::Allegiance {
                    recipient: actor,
                    info_response: true,
                    profile,
                });
                Ok(())
            }
            R::QueryMotd
            | R::QueryName
            | R::ListOfficers
            | R::ListOfficerTitles
            | R::ListBans
            | R::Lock(4 | 5) => self.query_allegiance(actor, request),
            R::SetMotd(message) => self.allegiance_manage(
                actor,
                M::Motd {
                    message: Some(message),
                    set_by: self
                        .social
                        .directory
                        .presence(actor)
                        .ok_or(E::Missing)?
                        .identity
                        .name
                        .clone(),
                },
            ),
            R::ClearMotd => self.allegiance_manage(
                actor,
                M::Motd {
                    message: None,
                    set_by: self
                        .social
                        .directory
                        .presence(actor)
                        .ok_or(E::Missing)?
                        .identity
                        .name
                        .clone(),
                },
            ),
            R::SetName(name) => self.allegiance_manage(actor, M::Name(Some(name))),
            R::ClearName => self.allegiance_manage(actor, M::Name(None)),
            R::SetOfficer { name, level } => self.allegiance_manage(
                actor,
                M::Officer {
                    actor: self.allegiance_named(&name)?,
                    level: Some(level),
                },
            ),
            R::RemoveOfficer(name) => self.allegiance_manage(
                actor,
                M::Officer {
                    actor: self.allegiance_named(&name)?,
                    level: None,
                },
            ),
            R::ClearOfficers => self.allegiance_manage(actor, M::ClearOfficers),
            R::SetOfficerTitle { level, title } => {
                self.allegiance_manage(actor, M::OfficerTitle { level, title })
            }
            R::ClearOfficerTitles => self.allegiance_manage(actor, M::ClearOfficerTitles),
            R::Lock(action) => self.allegiance_manage(
                actor,
                match action {
                    1 => M::Lock(false),
                    2 => M::Lock(true),
                    3 => M::ToggleLock,
                    6 => M::ClearApproved,
                    _ => return Err(E::Invalid),
                },
            ),
            R::ApproveVassal(name) => {
                self.allegiance_manage(actor, M::Approve(self.allegiance_named(&name)?))
            }
            R::ChatBoot { name, reason: _ } => self.allegiance_manage(
                actor,
                M::ChatGag {
                    actor: self.allegiance_named(&name)?,
                    until: Some(i64::MAX),
                },
            ),
            R::ChatGag { name, enabled } => self.allegiance_manage(
                actor,
                M::ChatGag {
                    actor: self.allegiance_named(&name)?,
                    until: if enabled {
                        Some(now.checked_add(300).ok_or(E::Overflow)?)
                    } else {
                        None
                    },
                },
            ),
            R::AddBan(name) => {
                let target = self.allegiance_named(&name)?;
                if self
                    .allegiance_relation(target)
                    .zip(self.allegiance_relation(actor))
                    .is_some_and(|(a, b)| a.monarch == b.monarch)
                {
                    self.allegiance_boot(actor, target, Some(name))
                } else {
                    self.allegiance_manage(
                        actor,
                        M::Ban {
                            character: target,
                            name,
                            enabled: true,
                        },
                    )
                }
            }
            R::RemoveBan(name) => self.allegiance_manage(
                actor,
                M::Ban {
                    character: self.allegiance_named(&name)?,
                    name,
                    enabled: false,
                },
            ),
            R::Boot { name, account: _ } => {
                self.allegiance_boot(actor, self.allegiance_named(&name)?, None)
            }
            R::HouseAction(_) | R::RecallHometown => Err(E::Invalid), // Session routes these to the authoritative housing/recall owners.
        }
    }
    fn allegiance_manage(&mut self, actor: EntityId, request: M) -> Result<(), E> {
        self.prepare_allegiance_management(actor, request)
            .map(|_| ())
    }
    fn allegiance_boot(
        &mut self,
        actor: EntityId,
        target: EntityId,
        ban: Option<String>,
    ) -> Result<(), E> {
        let room = self.allegiances.ids.front().copied().ok_or(E::Capacity)?;
        let patch = self
            .allegiances
            .registry
            .propose_boot(actor, target, ban, room)?;
        self.allegiances.propose(actor, patch, Vec::new())?;
        self.allegiances.ids.pop_front();
        Ok(())
    }
    fn query_allegiance(&mut self, actor: EntityId, request: R) -> Result<(), E> {
        let n = self.allegiances.registry.node(actor).ok_or(E::NotMember)?;
        let m = self
            .allegiances
            .registry
            .metadata(n.monarch)
            .ok_or(E::Missing)?;
        let text = match request {
            R::QueryMotd => m
                .motd
                .clone()
                .unwrap_or_else(|| "No allegiance message of the day is set.".into()),
            R::QueryName => m.name.clone().unwrap_or_else(|| {
                self.allegiances
                    .registry
                    .node(n.monarch)
                    .map_or(String::new(), |n| n.name.clone())
            }),
            R::ListOfficers => m
                .officers
                .iter()
                .map(|(id, rank)| {
                    format!(
                        "{}: {rank}",
                        self.allegiances
                            .registry
                            .node(*id)
                            .map_or("", |n| n.name.as_str())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            R::ListOfficerTitles => m
                .officer_titles
                .iter()
                .enumerate()
                .map(|(i, title)| {
                    format!(
                        "{}: {}",
                        i + 1,
                        title
                            .as_deref()
                            .unwrap_or(["Speaker", "Seneschal", "Castellan"][i])
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            R::ListBans => m
                .banned_characters
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
            R::Lock(4) => format!(
                "The allegiance is currently {}.",
                if m.locked { "locked" } else { "unlocked" }
            ),
            R::Lock(5) => m
                .approved
                .iter()
                .filter_map(|id| {
                    self.social
                        .directory
                        .presence(*id)
                        .map(|p| p.identity.name.clone())
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return Err(E::Invalid),
        };
        self.social.events.push_back(SocialEvent::System {
            recipient: actor,
            text,
            chat_type: 0,
        });
        Ok(())
    }
    pub fn allegiance_profile(&self, actor: EntityId) -> Result<AllegianceProfileSnapshot, E> {
        let Some(n) = self.allegiances.registry.node(actor) else {
            return Ok(AllegianceProfileSnapshot {
                subject: actor,
                rank: 0,
                total_members: 0,
                total_vassals: 0,
                chat_room: 0,
                name: String::new(),
                sanctuary: None,
                monarch: None,
                records: Vec::new(),
            });
        };
        let m = self
            .allegiances
            .registry
            .metadata(n.monarch)
            .ok_or(E::Missing)?;
        let monarch = self
            .allegiances
            .registry
            .node(n.monarch)
            .ok_or(E::Missing)?;
        let project = |n: &AllegianceNode| AllegianceNodeSnapshot {
            actor: n.character,
            cached: n.received_total.min(u64::from(u32::MAX)) as u32,
            tithed: n.tithed_total.min(u64::from(u32::MAX)) as u32,
            online: self
                .social
                .directory
                .presence(n.character)
                .is_some_and(|p| p.online),
            may_pass_up: n.patron.is_some() && n.may_pass_up,
            gender: n.gender,
            heritage: n.heritage,
            rank: n.rank as u16,
            level: n.level,
            loyalty: n.loyalty as u16,
            leadership: n.leadership as u16,
            name: n.name.clone(),
        };
        let mut records = Vec::with_capacity(13);
        if let Some(patron) = n.patron.filter(|p| *p != n.monarch) {
            records.push((
                n.monarch,
                project(self.allegiances.registry.node(patron).ok_or(E::Missing)?),
            ));
        }
        if let Some(patron) = n.patron {
            records.push((patron, project(n)));
        }
        for child in &n.vassals {
            records.push((
                actor,
                project(self.allegiances.registry.node(*child).ok_or(E::Missing)?),
            ));
        }
        Ok(AllegianceProfileSnapshot {
            subject: actor,
            rank: n.rank,
            total_members: monarch.followers.checked_add(1).ok_or(E::Overflow)?,
            total_vassals: n.followers,
            chat_room: m.chat_room,
            name: m.name.clone().unwrap_or_else(|| monarch.name.clone()),
            sanctuary: m.sanctuary,
            monarch: Some(project(monarch)),
            records,
        })
    }
    pub(in crate::kernel) fn emit_allegiance(&mut self, actor: EntityId) -> Result<(), E> {
        if self.social.events.len() >= self.social.capacity {
            return Err(E::Capacity);
        }
        let profile = self.allegiance_profile(actor)?;
        self.social.events.push_back(SocialEvent::Allegiance {
            recipient: actor,
            info_response: false,
            profile,
        });
        Ok(())
    }
}
