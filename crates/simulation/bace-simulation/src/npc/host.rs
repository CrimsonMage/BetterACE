use super::*;
use bace_emotes::{NativeEmoteHost, NpcActorFacts};
use bace_entity::{PropertyFamily as F, PropertyValue as V};
use bace_gameplay_api::*;
pub(super) struct Host<'a> {
    pub state: &'a mut Npcs,
    pub fellowships: &'a mut crate::fellowships::Fellowships,
    pub world: &'a mut World,
    pub characters: &'a mut Characters,
    pub random: &'a mut RandomStream,
    pub tick: u64,
    pub services: &'a dyn NpcServiceView,
}
fn family(f: NpcPropertyFamily) -> F {
    match f {
        NpcPropertyFamily::Bool => F::Bool,
        NpcPropertyFamily::Int => F::Int,
        NpcPropertyFamily::Int64 => F::Int64,
        NpcPropertyFamily::Float => F::Float,
        NpcPropertyFamily::String => F::String,
        NpcPropertyFamily::Attribute => F::Attribute,
        NpcPropertyFamily::RawAttribute => F::RawAttribute,
        NpcPropertyFamily::Vital => F::Vital,
        NpcPropertyFamily::RawVital => F::RawVital,
        NpcPropertyFamily::Skill => F::Skill,
        NpcPropertyFamily::RawSkill => F::RawSkill,
        NpcPropertyFamily::SkillAdvancement => F::SkillAdvancement,
    }
}
fn to_value(v: V) -> NpcValue {
    match v {
        V::Bool(v) => NpcValue::Bool(v),
        V::Int(v) => NpcValue::Int(v),
        V::Int64(v) => NpcValue::Int64(v),
        V::Float(v) => NpcValue::Float(v),
        V::String(v) => NpcValue::String(v),
        V::Unsigned(v) => NpcValue::Unsigned(v),
    }
}
fn from_value(v: NpcValue) -> V {
    match v {
        NpcValue::Bool(v) => V::Bool(v),
        NpcValue::Int(v) => V::Int(v),
        NpcValue::Int64(v) => V::Int64(v),
        NpcValue::Float(v) => V::Float(v),
        NpcValue::String(v) => V::String(v),
        NpcValue::Unsigned(v) => V::Unsigned(v),
    }
}
impl Host<'_> {
    pub(super) fn properties(&self, actor: EntityId) -> Option<&bace_entity::EntityProperties> {
        self.world
            .properties(actor)
            .or_else(|| self.state.archives.get(&actor).map(|a| &a.properties))
    }
    fn aggregate_fence(
        &self,
        actor: EntityId,
        changed: bool,
    ) -> Result<Option<NpcAggregateFence>, NpcFailure> {
        self.characters
            .get(actor)
            .map(|p| {
                let before_revision = p.revision();
                Ok(NpcAggregateFence {
                    before_revision,
                    after_revision: before_revision
                        .checked_add(u64::from(changed))
                        .ok_or(NpcFailure::InvalidInput)?,
                })
            })
            .transpose()
    }

    fn actor(&self, c: NpcContext, s: NpcSubject) -> Result<EntityId, NpcFailure> {
        match s {
            NpcSubject::Source => Ok(c.source),
            NpcSubject::Target => c.target.ok_or(NpcFailure::MissingActor),
            _ => Err(NpcFailure::Unsupported),
        }
    }
    fn now(&self) -> Result<u32, NpcFailure> {
        self.state
            .epoch
            .ok_or(NpcFailure::MissingContent)?
            .checked_add(u32::try_from(self.tick / 30).map_err(|_| NpcFailure::InvalidInput)?)
            .ok_or(NpcFailure::InvalidInput)
    }
    fn pending(
        &mut self,
        c: NpcContext,
        effect: NpcEffect,
        completion: NpcCompletion,
    ) -> Result<NpcCompletion, NpcFailure> {
        if self.state.pending.len() >= self.state.capacity
            || self.state.proposals.len() >= self.state.capacity
        {
            return Err(NpcFailure::Capacity);
        }
        if c.target.is_some_and(|actor| {
            self.characters.reserved(actor)
                || self.state.reserved(actor)
                || self.services.reserved(actor)
        }) {
            return Err(NpcFailure::DurabilityPending);
        }
        let ticket = self
            .state
            .next_ticket
            .checked_add(1)
            .filter(|ticket| *ticket <= self.state.ticket_limit)
            .ok_or(NpcFailure::Capacity)?;
        let proposal = NpcProposal {
            ticket,
            context: c,
            effect,
        };
        self.state.pending.insert(
            ticket,
            Pending {
                proposal: proposal.clone(),
                completion,
                adopted: false,
                detached: false,
            },
        );
        self.state.durable_dirty.insert(c.source);
        self.state.proposals.push_back(proposal);
        self.state.next_ticket = ticket;
        Ok(NpcCompletion::Pending { ticket })
    }
}
impl NativeEmoteHost for Host<'_> {
    fn facts(&self, actor: EntityId) -> Option<NpcActorFacts> {
        if self.world.body(actor).is_err() {
            return self.state.archives.get(&actor).map(|a| a.facts);
        }
        let player = self.characters.get(actor).is_some()
            || self
                .world
                .combatant(actor)
                .is_some_and(|p| p.profile().player);
        Some(NpcActorFacts {
            player,
            creature: player || self.world.combatant(actor).is_some(),
        })
    }
    fn draw(&mut self) -> Result<f64, NpcFailure> {
        let word = self.random.next_u64().map_err(|_| NpcFailure::Capacity)?;
        Ok((word >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0))
    }
    fn query(&mut self, c: NpcContext, q: NpcQuery) -> Result<NpcQueryValue, NpcFailure> {
        let boolean = |v| NpcQueryValue::Value(NpcValue::Bool(v));
        match q {
            NpcQuery::TitleCount => Ok(NpcQueryValue::Value(NpcValue::Int64(
                self.characters
                    .native_services(c.target.ok_or(NpcFailure::MissingActor)?)
                    .ok_or(NpcFailure::MissingContent)?
                    .titles
                    .len() as i64,
            ))),
            NpcQuery::ContractsFull => Ok(NpcQueryValue::Value(NpcValue::Bool(
                self.characters
                    .contracts(c.target.ok_or(NpcFailure::MissingActor)?)
                    .ok_or(NpcFailure::MissingContent)?
                    .full(),
            ))),
            NpcQuery::Property {
                subject,
                family: f,
                stat,
            } => {
                let actor = self.actor(c, subject)?;
                if let Some(archive) = self.state.archives.get(&actor) {
                    return Ok(archive
                        .properties
                        .get(family(f), stat)
                        .cloned()
                        .map(to_value)
                        .map(NpcQueryValue::Value)
                        .unwrap_or(NpcQueryValue::Absent));
                }
                if let Some(value) = self
                    .characters
                    .native_services(actor)
                    .and_then(|state| state.scalar(f, stat))
                {
                    return Ok(NpcQueryValue::Value(value));
                }
                if self.characters.get(actor).is_some()
                    && ((f == NpcPropertyFamily::Int && matches!(stat, 23 | 25 | 390))
                        || (f == NpcPropertyFamily::Int64 && stat == 1))
                {
                    if f == NpcPropertyFamily::Int
                        && stat == 23
                        && self.characters.native_services(actor).is_some()
                    {
                        return Ok(NpcQueryValue::Absent);
                    }
                    return Err(NpcFailure::MissingContent);
                }
                if matches!(
                    f,
                    NpcPropertyFamily::Skill
                        | NpcPropertyFamily::RawSkill
                        | NpcPropertyFamily::SkillAdvancement
                        | NpcPropertyFamily::Attribute
                        | NpcPropertyFamily::RawAttribute
                        | NpcPropertyFamily::Vital
                ) {
                    let raw = AttributeId::try_from(stat)
                        .ok()
                        .and_then(|id| {
                            self.characters
                                .get(actor)?
                                .projection(ProgressionTarget::Attribute(id))
                        })
                        .and_then(|p| match p.details {
                            Some(TraitDetails::Attribute { starting_value }) => {
                                starting_value.checked_add(u32::from(p.ranks))
                            }
                            _ => None,
                        });
                    if f == NpcPropertyFamily::RawAttribute
                        && let Some(raw) = raw
                    {
                        return Ok(NpcQueryValue::Value(NpcValue::Unsigned(raw)));
                    }
                    if f == NpcPropertyFamily::Vital {
                        let vital = match stat {
                            1 | 2 => bace_entity::EntityVital::Health,
                            3 | 4 => bace_entity::EntityVital::Stamina,
                            5 | 6 => bace_entity::EntityVital::Mana,
                            _ => return Err(NpcFailure::InvalidInput),
                        };
                        if let Ok(pool) = self.world.vital(actor, vital) {
                            return Ok(NpcQueryValue::Value(NpcValue::Unsigned(
                                if stat % 2 == 0 {
                                    pool.current
                                } else {
                                    pool.maximum
                                },
                            )));
                        }
                    }
                    match self.services.query(
                        c,
                        &NpcQuery::Property {
                            subject,
                            family: f,
                            stat,
                        },
                        raw,
                    ) {
                        Ok(value) => return Ok(value),
                        Err(NpcFailure::Unsupported) => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(self
                    .world
                    .properties(actor)
                    .and_then(|p| p.get(family(f), stat))
                    .cloned()
                    .map(to_value)
                    .map(NpcQueryValue::Value)
                    .unwrap_or(NpcQueryValue::Absent))
            }
            NpcQuery::Quest {
                subject,
                name,
                check,
            } => {
                let progress = if subject == NpcSubject::Fellowship {
                    let Some(id) = c
                        .target
                        .and_then(|actor| self.fellowships.members.get(&actor))
                    else {
                        return Ok(NpcQueryValue::NoFellow);
                    };
                    self.fellowships.quests.get(id).and_then(|q| q.get(&name))
                } else {
                    let actor = self.actor(c, subject)?;
                    if self.characters.get(actor).is_some()
                        && !self.state.quests.contains_key(&actor)
                    {
                        return Err(NpcFailure::MissingContent);
                    }
                    self.state.quests.get(&actor).and_then(|q| q.get(&name))
                };
                let result = match check {
                    NpcQuestCheck::HasAndCannotSolve => {
                        let definition = self.state.definitions.get(
                            &bace_quests::quest_key(&name).map_err(|_| NpcFailure::InvalidInput)?,
                        );
                        progress.is_some()
                            && bace_quests::next_solve(
                                definition,
                                progress.as_ref(),
                                self.now()?,
                                1.0,
                            )
                            .map_err(|_| NpcFailure::InvalidInput)?
                                != bace_quests::QuestEligibility::Ready
                    }
                    NpcQuestCheck::Solves { minimum, maximum } => {
                        bace_quests::has_solves(progress.as_ref(), minimum, maximum)
                    }
                    NpcQuestCheck::Bits { mask, on } => {
                        if on {
                            bace_quests::has_bits(progress.as_ref(), mask)
                        } else {
                            bace_quests::has_no_bits(progress.as_ref(), mask)
                        }
                    }
                };
                Ok(boolean(result))
            }
            NpcQuery::Event(name) => {
                let now = i32::try_from(self.now()?).map_err(|_| NpcFailure::InvalidInput)?;
                Ok(boolean(
                    self.state
                        .events
                        .started(&name, now)
                        .map_err(|_| NpcFailure::InvalidInput)?,
                ))
            }
            NpcQuery::FellowCount => Ok(c
                .target
                .and_then(|actor| self.fellowships.members.get(&actor))
                .and_then(|id| self.fellowships.groups.get(id))
                .map(|f| NpcQueryValue::Value(NpcValue::Int64(f.members().len() as i64)))
                .unwrap_or(NpcQueryValue::NoFellow)),
            query => self.services.query(c, &query, None),
        }
    }
    fn execute(
        &mut self,
        c: NpcContext,
        operation: NpcOperation,
    ) -> Result<NpcCompletion, NpcFailure> {
        let completed = NpcCompletion::Applied { post_delay: 0.0 };
        match operation {
            NpcOperation::Reward {
                kind: NpcRewardKind::Title,
                amount,
                ..
            } => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                let revision = self
                    .characters
                    .get(actor)
                    .ok_or(NpcFailure::MissingActor)?
                    .revision();
                let change = self
                    .characters
                    .native_services(actor)
                    .ok_or(NpcFailure::MissingContent)?
                    .propose_title(
                        revision,
                        u32::try_from(amount).map_err(|_| NpcFailure::InvalidInput)?,
                    )
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.pending(c, NpcEffect::CharacterService { actor, change }, completed)
            }
            NpcOperation::Sanctuary(destination) => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                let revision = self
                    .characters
                    .get(actor)
                    .ok_or(NpcFailure::MissingActor)?
                    .revision();
                let change = self
                    .characters
                    .native_services(actor)
                    .ok_or(NpcFailure::MissingContent)?
                    .propose_sanctuary(revision, destination)
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.pending(c, NpcEffect::CharacterService { actor, change }, completed)
            }
            NpcOperation::Contract { id, add } => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                let before_revision = self
                    .characters
                    .get(actor)
                    .ok_or(NpcFailure::MissingActor)?
                    .revision();
                if !self.state.contract_catalog_loaded {
                    return Err(NpcFailure::MissingContent);
                }
                let change = match self
                    .characters
                    .contracts(actor)
                    .ok_or(NpcFailure::MissingContent)?
                    .propose(id, add, self.state.contract_definitions.contains_key(&id))
                {
                    Ok(change) => change,
                    Err(bace_quests::ContractError::MissingDefinition) => return Ok(completed),
                    Err(bace_quests::ContractError::Full) => {
                        if self.state.notifications.len() >= self.state.capacity {
                            return Err(NpcFailure::Capacity);
                        }
                        let name = self
                            .state
                            .contract_definitions
                            .get(&id)
                            .ok_or(NpcFailure::MissingContent)?;
                        self.state.notifications.push_back(NpcNotification{context:c,operation:NpcOperation::Text{kind:NpcTextKind::Direct,text:format!("You currently have the maximum amount of contracts for this character and cannot take on another! You must abandon at least one contract before you can accept the contract for {name}."),extent:1.0}});
                        return Ok(completed);
                    }
                    Err(_) => return Err(NpcFailure::InvalidInput),
                };
                self.pending(
                    c,
                    NpcEffect::Contract {
                        actor,
                        before_revision,
                        change,
                    },
                    completed,
                )
            }
            NpcOperation::Text { .. } | NpcOperation::Particle { .. } | NpcOperation::Sound(_) => {
                if self.state.notifications.len() >= self.state.capacity {
                    return Err(NpcFailure::Capacity);
                }
                self.state.notifications.push_back(NpcNotification {
                    context: c,
                    operation,
                });
                Ok(completed)
            }
            NpcOperation::Property {
                subject,
                stat,
                mutation,
            } => {
                let actor = self.actor(c, subject)?;
                let requested_family = match &mutation {
                    NpcPropertyMutation::Set { family, .. } => *family,
                    NpcPropertyMutation::AddInt(_) => NpcPropertyFamily::Int,
                };
                if self.characters.get(actor).is_some()
                    && ((requested_family == NpcPropertyFamily::Int
                        && matches!(stat, 23 | 25 | 390))
                        || (requested_family == NpcPropertyFamily::Int64 && stat == 1))
                {
                    let revision = self
                        .characters
                        .get(actor)
                        .ok_or(NpcFailure::MissingActor)?
                        .revision();
                    let change = self
                        .characters
                        .native_services(actor)
                        .ok_or(NpcFailure::MissingContent)?
                        .propose_scalar(revision, requested_family, stat, mutation)
                        .map_err(|_| NpcFailure::InvalidInput)?;
                    return self.pending(
                        c,
                        NpcEffect::CharacterService { actor, change },
                        completed,
                    );
                }
                let properties = self.properties(actor).ok_or(NpcFailure::MissingContent)?;
                let (f, after) = match mutation {
                    NpcPropertyMutation::Set { family, value } => {
                        (self::family(family), value.map(from_value))
                    }
                    NpcPropertyMutation::AddInt(amount) => {
                        let before = match properties.get(F::Int, stat) {
                            Some(V::Int(v)) => *v,
                            None => 0,
                            _ => return Err(NpcFailure::InvalidInput),
                        };
                        (
                            F::Int,
                            Some(V::Int(
                                before.checked_add(amount).ok_or(NpcFailure::InvalidInput)?,
                            )),
                        )
                    }
                };
                let change = properties
                    .propose(f, stat, after)
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.pending(
                    c,
                    NpcEffect::Property {
                        actor,
                        aggregate: self.aggregate_fence(actor, change.before != change.after)?,
                        change,
                    },
                    completed,
                )
            }
            NpcOperation::Quest {
                subject,
                name,
                mutation,
            } => {
                if subject == NpcSubject::Fellowship {
                    let Some(id) = c
                        .target
                        .and_then(|actor| self.fellowships.members.get(&actor))
                        .copied()
                    else {
                        return Ok(if mutation == NpcQuestMutation::Update {
                            NpcCompletion::Branch { category: 30 }
                        } else {
                            completed
                        });
                    };
                    let now = self.now()?;
                    let key =
                        bace_quests::quest_key(&name).map_err(|_| NpcFailure::InvalidInput)?;
                    let quests = &self.fellowships.quests[&id];
                    let definition = self.state.definitions.get(&key);
                    let completion = if mutation == NpcQuestMutation::Update {
                        if quests.get(&name).is_some()
                            && !quests
                                .can_solve(&name, definition, now, 1.0)
                                .map_err(|_| NpcFailure::InvalidInput)?
                        {
                            return Ok(NpcCompletion::Branch { category: 13 });
                        }
                        NpcCompletion::Branch { category: 12 }
                    } else {
                        completed
                    };
                    let kind = if mutation == NpcQuestMutation::Update {
                        bace_quests::QuestMutation::Update
                    } else {
                        bace_quests::QuestMutation::Stamp
                    };
                    let change = quests
                        .propose(&name, kind, definition, now)
                        .map_err(|_| NpcFailure::InvalidInput)?;
                    return self.pending(
                        c,
                        NpcEffect::FellowQuest {
                            fellowship: id,
                            change,
                        },
                        completion,
                    );
                }
                let actor = self.actor(c, subject)?;
                let now = self.now()?;
                let key = bace_quests::quest_key(&name).map_err(|_| NpcFailure::InvalidInput)?;
                if !self.state.quests.contains_key(&actor) {
                    if self.state.quests.len() >= self.state.capacity {
                        return Err(NpcFailure::Capacity);
                    }
                    self.state.quests.insert(
                        actor,
                        QuestRegistry::new(4096).map_err(|_| NpcFailure::Capacity)?,
                    );
                }
                let quests = &self.state.quests[&actor];
                let definition = self.state.definitions.get(&key);
                let mut completion = completed;
                if mutation == NpcQuestMutation::Update {
                    if quests.get(&name).is_some()
                        && !quests
                            .can_solve(&name, definition, now, 1.0)
                            .map_err(|_| NpcFailure::InvalidInput)?
                    {
                        return Ok(NpcCompletion::Branch { category: 13 });
                    }
                    completion = NpcCompletion::Branch { category: 12 };
                }
                let mutation = match mutation {
                    NpcQuestMutation::Update => bace_quests::QuestMutation::Update,
                    NpcQuestMutation::Stamp => bace_quests::QuestMutation::Stamp,
                    NpcQuestMutation::Erase => bace_quests::QuestMutation::Erase,
                    NpcQuestMutation::Increment(n) => bace_quests::QuestMutation::Increment(n),
                    NpcQuestMutation::Decrement(n) => bace_quests::QuestMutation::Decrement(n),
                    NpcQuestMutation::Completions(n) => bace_quests::QuestMutation::Completions(n),
                    NpcQuestMutation::Bits { mask, on } => {
                        bace_quests::QuestMutation::Bits { mask, on }
                    }
                };
                let change = quests
                    .propose(&name, mutation, definition, now)
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.pending(
                    c,
                    NpcEffect::Quest {
                        actor,
                        aggregate: self.aggregate_fence(actor, change.before != change.after)?,
                        change,
                    },
                    completion,
                )
            }
            NpcOperation::Reward {
                kind:
                    kind @ (NpcRewardKind::Experience
                    | NpcRewardKind::NoShareExperience
                    | NpcRewardKind::LevelExperience),
                mut amount,
                percent,
                minimum,
                maximum,
                ..
            } => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                if kind == NpcRewardKind::LevelExperience {
                    let state = self
                        .characters
                        .native_services(actor)
                        .ok_or(NpcFailure::MissingContent)?;
                    let table = self
                        .state
                        .level_table
                        .as_ref()
                        .ok_or(NpcFailure::MissingContent)?;
                    amount = bace_character::level_proportional_xp(
                        table.next_level_experience(state.level),
                        percent,
                        minimum,
                        maximum,
                    )
                    .map_err(|_| NpcFailure::InvalidInput)?;
                }
                if kind == NpcRewardKind::Experience && amount <= 0 {
                    if amount == 0 {
                        return Ok(completed);
                    }
                    let credit = self
                        .characters
                        .get(actor)
                        .ok_or(NpcFailure::MissingActor)?
                        .propose_experience_spend(amount.unsigned_abs())
                        .map_err(|_| NpcFailure::InvalidInput)?;
                    return self.pending(c, NpcEffect::Experience { actor, credit }, completed);
                }
                if matches!(
                    kind,
                    NpcRewardKind::Experience | NpcRewardKind::LevelExperience
                ) && !self.state.shared_experience
                {
                    return Err(NpcFailure::Unsupported);
                }
                self.characters
                    .native_services(actor)
                    .ok_or(NpcFailure::MissingContent)?;
                let (global, quest) = self.state.xp_rates.ok_or(NpcFailure::MissingContent)?;
                let enchantment = self.services.experience_modifier(actor)?;
                // C# long * float rounds to float before the double rate product.
                let scaled =
                    (f64::from(amount as f32 * enchantment) * (global * quest)).round_ties_even();
                if !scaled.is_finite() || scaled >= i64::MAX as f64 {
                    return Err(NpcFailure::InvalidInput);
                }
                if scaled < 0.0 {
                    return Ok(completed);
                } // EarnXP's negative-result guard.
                let properties = self
                    .world
                    .properties(actor)
                    .ok_or(NpcFailure::MissingContent)?;
                if matches!(properties.get(F::Int, 188), Some(V::Int(12 | 13))) {
                    return Err(NpcFailure::Unsupported);
                }
                if !self.state.shared_experience {
                    self.services.experience_admission_supported(actor)?;
                }
                self.pending(
                    c,
                    NpcEffect::QueuedExperience {
                        actor,
                        amount: scaled as u64,
                        share: match kind {
                            NpcRewardKind::Experience => NpcExperienceSharing::All,
                            NpcRewardKind::LevelExperience => NpcExperienceSharing::Allegiance,
                            _ => NpcExperienceSharing::None,
                        },
                        phase: NpcQueuedExperiencePhase::AwaitingAdmission,
                    },
                    completed,
                )
            }
            NpcOperation::Reward {
                kind: NpcRewardKind::Luminance | NpcRewardKind::SpendLuminance,
                amount,
                ..
            } => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                let spend = matches!(
                    operation,
                    NpcOperation::Reward {
                        kind: NpcRewardKind::SpendLuminance,
                        ..
                    }
                );
                let credit = self
                    .characters
                    .get(actor)
                    .ok_or(NpcFailure::MissingActor)?
                    .propose_luminance(amount, spend, bace_character::LuminanceModifiers::default())
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.pending(c, NpcEffect::Luminance { actor, credit }, completed)
            }
            NpcOperation::LockFellow { quest } => {
                let actor = c.target.ok_or(NpcFailure::MissingActor)?;
                let Some(id) = self.fellowships.members.get(&actor).copied() else {
                    return Ok(completed);
                };
                if self.state.notifications.len() >= self.state.capacity {
                    return Err(NpcFailure::Capacity);
                }
                self.fellowships
                    .groups
                    .get_mut(&id)
                    .ok_or(NpcFailure::MissingActor)?
                    .lock_from_emote(actor, true, quest.clone())
                    .map_err(|_| NpcFailure::InvalidInput)?;
                self.state.notifications.push_back(NpcNotification {
                    context: c,
                    operation: NpcOperation::LockFellow { quest },
                });
                Ok(completed)
            }
            NpcOperation::Event { name, start } => {
                self.state
                    .events
                    .set(&name, start)
                    .map_err(|_| NpcFailure::InvalidInput)?;
                Ok(completed)
            }
            operation => self.pending(c, NpcEffect::Service(operation), completed),
        }
    }
    fn text(
        &mut self,
        c: NpcContext,
        message: &str,
        quest: Option<&str>,
    ) -> Result<String, NpcFailure> {
        super::text::format(self, c, message, quest)
    }
}
