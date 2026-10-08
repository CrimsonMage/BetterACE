//! Deterministic offline emote runner. Only effects applied to its local model
//! return success; other effects stop with an explicit unsupported result.
use bace_content::WeenieTemplate;
use bace_emotes::{
    NativeEmoteHost, NativeEmoteManager, NativeLimits, NativeProgram, NativeStep, NativeTrigger,
    NpcActorFacts,
};
use bace_gameplay_api::{
    NpcCompletion, NpcContext, NpcFailure, NpcOperation, NpcQuery, NpcQueryValue, NpcQuestCheck,
    NpcQuestMutation, NpcRewardKind, NpcValue,
};
use bace_types::EntityId;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) struct Trace {
    pub lines: Vec<String>,
    pub inventory: BTreeMap<u32, u32>,
    pub quests: BTreeMap<String, i32>,
    pub xp: i64,
}

struct Host {
    now: f64,
    lines: Vec<String>,
    inventory: BTreeMap<u32, u32>,
    quests: BTreeMap<String, i32>,
    xp: i64,
}

impl Host {
    fn log(&mut self, text: impl AsRef<str>) {
        self.lines
            .push(format!("{:>7.3}s  {}", self.now, text.as_ref()));
    }
}

impl NativeEmoteHost for Host {
    fn facts(&self, _: EntityId) -> Option<NpcActorFacts> {
        Some(NpcActorFacts {
            player: true,
            creature: true,
        })
    }
    fn draw(&mut self) -> Result<f64, NpcFailure> {
        Ok(0.5)
    }
    fn text(&mut self, _: NpcContext, text: &str, _: Option<&str>) -> Result<String, NpcFailure> {
        Ok(text.to_owned())
    }
    fn query(&mut self, _: NpcContext, query: NpcQuery) -> Result<NpcQueryValue, NpcFailure> {
        let value = match query {
            NpcQuery::ItemCount { template } => {
                NpcValue::Int64(i64::from(*self.inventory.get(&template).unwrap_or(&0)))
            }
            NpcQuery::Quest { name, check, .. } => {
                let count = *self.quests.get(&name).unwrap_or(&0);
                let yes = match check {
                    NpcQuestCheck::HasAndCannotSolve | NpcQuestCheck::Solves { .. } => {
                        self.log("Quest solve query needs a quest definition and clock");
                        return Err(NpcFailure::Unsupported);
                    }
                    NpcQuestCheck::Bits { mask, on } => ((count & mask) != 0) == on,
                };
                NpcValue::Bool(yes)
            }
            unsupported => {
                self.log(format!("Unsupported query: {unsupported:?}"));
                return Err(NpcFailure::Unsupported);
            }
        };
        self.log(format!("Query → {value:?}"));
        Ok(NpcQueryValue::Value(value))
    }
    fn execute(
        &mut self,
        _: NpcContext,
        operation: NpcOperation,
    ) -> Result<NpcCompletion, NpcFailure> {
        self.log(format!("{operation:?}"));
        match operation {
            NpcOperation::Text { .. } => {}
            NpcOperation::Quest { name, mutation, .. } => {
                let entry = self.quests.entry(name);
                match mutation {
                    NpcQuestMutation::Stamp => {
                        *entry.or_insert(0) = 1;
                    }
                    NpcQuestMutation::Erase => {
                        entry.and_modify(|value| *value = 0);
                    }
                    NpcQuestMutation::Increment(amount) => {
                        let value = entry.or_insert(0);
                        *value = value.checked_add(amount).ok_or(NpcFailure::InvalidInput)?;
                    }
                    NpcQuestMutation::Decrement(amount) => {
                        let value = entry.or_insert(0);
                        *value = value.checked_sub(amount).ok_or(NpcFailure::InvalidInput)?;
                    }
                    NpcQuestMutation::Completions(count) => {
                        *entry.or_insert(0) = count;
                    }
                    NpcQuestMutation::Bits { mask, on } => {
                        let value = entry.or_insert(0);
                        if on {
                            *value |= mask;
                        } else {
                            *value &= !mask;
                        }
                    }
                    NpcQuestMutation::Update => {
                        self.log("Quest Update requires live solve policy");
                        return Err(NpcFailure::Unsupported);
                    }
                }
            }
            NpcOperation::Give {
                template, count, ..
            } => {
                let value = self.inventory.entry(template).or_default();
                *value = value.checked_add(count).ok_or(NpcFailure::Capacity)?;
            }
            NpcOperation::Take { template, count } => {
                let value = self.inventory.entry(template).or_default();
                let quantity = count.unwrap_or(*value);
                if quantity > *value {
                    return Err(NpcFailure::MissingContent);
                }
                *value -= quantity;
            }
            NpcOperation::Reward {
                kind: NpcRewardKind::Experience,
                amount,
                ..
            } => {
                self.xp = self.xp.checked_add(amount).ok_or(NpcFailure::Capacity)?;
            }
            unsupported => {
                self.log(format!("Unsupported effect: {unsupported:?}"));
                return Err(NpcFailure::Unsupported);
            }
        }
        Ok(NpcCompletion::Applied { post_delay: 0.0 })
    }
}

pub(crate) fn run(weenie: &WeenieTemplate, category: u32) -> Result<Trace, String> {
    let program = Arc::new(
        NativeProgram::prepare(
            weenie.properties.emotes.clone(),
            NativeLimits {
                instructions: 4096,
                ..NativeLimits::default()
            },
        )
        .map_err(|e| format!("Emote program: {e:?}"))?,
    );
    let mut manager = NativeEmoteManager::new(program);
    let mut host = Host {
        now: 0.0,
        lines: Vec::new(),
        inventory: BTreeMap::new(),
        quests: BTreeMap::new(),
        xp: 0,
    };
    let selected = manager
        .trigger(
            &NativeTrigger {
                category,
                random: false,
                ..Default::default()
            },
            NpcContext {
                source: EntityId(1),
                target: Some(EntityId(2)),
                operation: 1,
            },
            0.0,
            &mut host,
        )
        .map_err(|e| format!("Emote trigger: {e:?}"))?;
    if !selected {
        host.log("No matching emote set");
    }
    for _ in 0..4096 {
        if !manager.busy() {
            break;
        }
        let next = manager
            .checkpoint()
            .work
            .iter()
            .map(|work| work.due)
            .min_by(f64::total_cmp)
            .ok_or("Emote awaits a live completion")?;
        host.now = next;
        let result = manager.step(next, &mut host);
        match result {
            Ok(NativeStep::Executed { action }) => host.log(format!("Action {action} completed")),
            Ok(NativeStep::SourceNoop { action }) => {
                host.log(format!("Action {action} is a source no-op"))
            }
            Ok(NativeStep::Waiting) => {}
            Ok(NativeStep::Idle | NativeStep::Completed) => break,
            Ok(NativeStep::Pending { .. }) => {
                host.log("Effect awaits a live completion");
                break;
            }
            Err(error) => {
                host.log(format!("Stopped: {error:?}"));
                break;
            }
        }
    }
    if manager.busy() {
        host.log("Trace stopped before completion");
    }
    Ok(Trace {
        lines: host.lines,
        inventory: host.inventory,
        quests: host.quests,
        xp: host.xp,
    })
}
