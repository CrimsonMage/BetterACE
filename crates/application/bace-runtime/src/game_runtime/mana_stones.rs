//! Pinned ACE ManaStone UseWithTarget, retained through exact item durability.
mod output;
use super::*;
use crate::mana_stone_service::{ManaStoneCompletion, ManaStoneService};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_inventory::{ManaCharge, ManaStoneDecision, ManaStoneItem, ManaStoneProfile};
use bace_simulation::{
    Command, ManaStoneCommand, ManaStoneError, ManaStoneOutcome, ManaStoneResult, ManaStoneTicket,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;

enum Phase {
    Ready(Option<Command>),
    Submitted,
    Saving,
    Finished {
        decision: ManaStoneDecision,
        committed: bool,
    },
    Rejected(ManaStoneError),
}
struct Pending {
    key: SessionKey,
    context: ActionContext,
    correlation: u64,
    stone: EntityId,
    target_name: String,
    phase: Phase,
    ticket: Option<ManaStoneTicket>,
    completion: Option<ManaStoneCompletion>,
}
pub(super) struct ManaStoneRuntime {
    service: ManaStoneService,
    pending: Option<Pending>,
    unexpected: Option<Box<ManaStoneOutcome>>,
    failures: BTreeMap<SessionKey, String>,
}
impl ManaStoneRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: ManaStoneService::new(),
            pending: None,
            unexpected: None,
            failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some() || self.service.has_pending() || self.unexpected.is_some()
    }
    pub(super) fn owns_capture(&self, outcome: &bace_simulation::PlayerSnapshotOutcome) -> bool {
        self.service.owns_capture(outcome)
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: bace_simulation::PlayerSnapshotOutcome,
    ) -> Result<(), bace_simulation::PlayerSnapshotOutcome> {
        self.service.accept_capture(outcome)
    }
}
impl GameRuntime {
    pub fn mana_stone_failure(&self, key: SessionKey) -> Option<&str> {
        self.mana_stones
            .failures
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.mana_stone_ingress_blocked(key)
                    .then(|| self.mana_stones.service.blocked())
                    .flatten()
            })
    }
    pub(super) fn mana_stone_ingress_blocked(&self, key: SessionKey) -> bool {
        self.mana_stones
            .pending
            .as_ref()
            .is_some_and(|p| p.key == key)
    }
    pub(super) fn retry_mana_stone(&mut self, key: SessionKey) -> bool {
        if self.mana_stone_ingress_blocked(key) && self.mana_stones.service.blocked().is_some() {
            self.mana_stones.service.retry();
            self.mana_stones.failures.remove(&key);
            true
        } else {
            false
        }
    }
    pub(super) fn forget_mana_stone_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.mana_stone_ingress_blocked(key) {
            return Err("mana-stone durable work remains pending".into());
        }
        self.mana_stones.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_mana_stone_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(session) = self.sessions.get(&key) else {
            return Ok(I::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(I::Unsupported);
        };
        let binding = loading.loaded.binding;
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let decoded = match bace_session::decode_crafting(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(bace_session::DispatchError::UnsupportedAction(_)) => return Ok(I::Unsupported),
            Err(error) => return Err(format!("mana-stone input: {error:?}")),
        };
        let bace_wire::CraftingAction::UseWithTarget {
            source_id,
            target_id,
        } = decoded.request
        else {
            return Ok(I::Unsupported);
        };
        let context = decoded.context;
        let Some(source) = self
            .online_saves
            .inventory_baseline(binding.actor.0, source_id)
        else {
            return Ok(I::Unsupported);
        };
        if source.entity.state.weenie_type != 37 {
            return Ok(I::Unsupported);
        }
        if self.mana_stones.has_pending()
            || self.inventory.has_pending()
            || self.healing_kits.has_pending()
            || self.crafting.has_pending()
        {
            return Ok(I::Blocked);
        }
        if !self.players.entered(binding.actor) || session.terminated || session.disconnected {
            return Ok(I::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("mana-stone authenticated binding mismatch".into());
        }
        let target = self
            .online_saves
            .inventory_baseline(binding.actor.0, target_id);
        let target_type = source
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 94)
            .map_or(0, |p| p.value);
        let compatible = target.is_some_and(|row| {
            let item_type = row
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|p| p.id == 1)
                .map_or(0, |p| p.value);
            target_type & item_type != 0
        });
        let target_name = target
            .and_then(|row| {
                row.entity
                    .state
                    .properties
                    .strings
                    .iter()
                    .find(|p| p.id == 1)
                    .map(|p| p.value.clone())
            })
            .unwrap_or_else(|| "item".into());
        let profile = profile(
            &source.entity.state,
            &loading.loaded.player.player.entity.state,
        )?;
        let stone = ManaStoneItem {
            id: EntityId(source_id),
            mana: mana(&source.entity.state),
            retained: false,
        };
        let stone_revision = source.entity.mutation_revision;
        let target = target.map(|row| {
            (
                ManaStoneItem {
                    id: EntityId(target_id),
                    mana: mana(&row.entity.state),
                    retained: row
                        .entity
                        .state
                        .properties
                        .bools
                        .iter()
                        .any(|p| p.id == 91 && p.value),
                },
                row.entity.mutation_revision,
            )
        });
        let correlation = self.token()?;
        let phase = if !compatible && target.is_some() || target_id == binding.actor.0 {
            Phase::Rejected(ManaStoneError::InvalidTarget)
        } else if let Some((target, target_revision)) = target {
            Phase::Ready(Some(Command::ManaStone(ManaStoneCommand::Use {
                correlation,
                context,
                stone,
                stone_revision,
                target,
                target_revision,
                profile,
            })))
        } else {
            Phase::Rejected(ManaStoneError::Ownership)
        };
        self.mana_stones.pending = Some(Pending {
            key,
            context,
            correlation,
            stone: EntityId(source_id),
            target_name,
            phase,
            ticket: None,
            completion: None,
        });
        self.mana_stones.failures.remove(&key);
        self.diagnostic(
            "info",
            "game.mana_stone_use",
            &format!(
                "session {} generation {} stone {source_id:#010x} target {target_id:#010x}",
                key.id, key.generation
            ),
        );
        Ok(I::Accepted)
    }
    pub(super) fn poll_mana_stones(&mut self) -> Result<(), String> {
        if self.mana_stones.unexpected.is_some() {
            return Err("uncorrelated mana-stone owner output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.mana_stone_outcomes().try_recv() else {
                break;
            };
            let Some(p) = self.mana_stones.pending.as_mut() else {
                self.mana_stones.unexpected = Some(Box::new(outcome));
                return Err("mana-stone output without request".into());
            };
            if matches!(p.phase, Phase::Saving) {
                self.mana_stones
                    .service
                    .accept_owner(outcome)
                    .map_err(|outcome| {
                        self.mana_stones.unexpected = Some(outcome);
                        "mana-stone receipt mismatch".to_owned()
                    })?;
                continue;
            }
            if !matches!(p.phase, Phase::Submitted)
                || outcome.correlation != p.correlation
                || outcome.context != Some(p.context)
            {
                self.mana_stones.unexpected = Some(Box::new(outcome));
                return Err("mana-stone proposal correlation mismatch".into());
            }
            match outcome.result {
                Ok(ManaStoneResult::Immediate(decision)) => {
                    p.phase = Phase::Finished {
                        decision,
                        committed: true,
                    }
                }
                Ok(ManaStoneResult::Proposed(ticket)) if ticket.context == p.context => {
                    let mut identity = [0; 16];
                    OsRng
                        .try_fill_bytes(&mut identity)
                        .map_err(|e| e.to_string())?;
                    let binding = CharacterBinding {
                        actor: p.context.actor,
                        account: p.context.account,
                        session: p.context.session,
                    };
                    self.mana_stones
                        .service
                        .stage(identity, binding, ticket.clone())
                        .map_err(|_| "mana-stone save service busy".to_owned())?;
                    p.ticket = Some(ticket);
                    p.phase = Phase::Saving;
                }
                Err(error) => p.phase = Phase::Rejected(error),
                _ => return Err("unexpected mana-stone proposal result".into()),
            }
        }
        if let Some(p) = self.mana_stones.pending.as_mut()
            && let Phase::Ready(command) = &mut p.phase
            && let Some(value) = command.take()
        {
            match self.simulation.input().try_submit(value) {
                Ok(()) => p.phase = Phase::Submitted,
                Err(TrySendError::Full(value)) => *command = Some(value),
                Err(TrySendError::Disconnected(value)) => {
                    *command = Some(value);
                    return Err("mana-stone simulation input closed".into());
                }
            }
        }
        let correlation = self.token()?;
        if let Err(error) = self.mana_stones.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        ) && let Some(key) = self.mana_stones.pending.as_ref().map(|p| p.key)
        {
            self.diagnostic(
                "error",
                "game.mana_stone_durability",
                &format!("session {} generation {}: {error}", key.id, key.generation),
            );
            self.mana_stones.failures.insert(key, error);
        }
        if let Some(completion) = self.mana_stones.service.take_completion() {
            let p = self
                .mana_stones
                .pending
                .as_mut()
                .ok_or("mana-stone completion without request")?;
            if p.ticket.as_ref().is_none_or(|ticket| {
                ticket.inventory.operation != completion.ticket.inventory.operation
            }) {
                return Err("mana-stone operation mismatch".into());
            }
            p.phase = Phase::Finished {
                decision: completion.ticket.decision.clone(),
                committed: completion.committed,
            };
            p.completion = Some(completion);
        }
        self.project_mana_stone_output()?;
        Ok(())
    }
}

fn mana(item: &bace_content::WeenieV1) -> ManaCharge {
    let int = |id| {
        item.properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    ManaCharge {
        current: int(107),
        maximum: int(108),
    }
}
fn profile(
    stone: &bace_content::WeenieV1,
    player: &bace_content::WeenieV1,
) -> Result<ManaStoneProfile, String> {
    let float = |id| {
        stone
            .properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    let rank = player
        .properties
        .ints
        .iter()
        .find(|p| p.id == 340)
        .map_or(0, |p| p.value);
    let profile = ManaStoneProfile {
        efficiency: float(87),
        destroy_chance: float(137),
        unlimited_use: stone.properties.bools.iter().any(|p| p.id == 63 && p.value),
        mana_gain_rank: rank,
    };
    if profile.efficiency.is_some_and(|v| !v.is_finite() || v < 0.)
        || profile
            .destroy_chance
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        || !(0..=100).contains(&rank)
    {
        return Err("invalid mana-stone source properties".into());
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Property, WeenieV1};

    fn source(weenie_type: u32) -> WeenieV1 {
        WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "mana_stone_fixture".into(),
            weenie_type,
            last_modified: None,
            properties: Default::default(),
        }
    }

    #[test]
    fn pinned_mana_stone_property_ids_prepare_distinct_charge_and_policy() {
        // ACE ManaStone.cs reads ItemEfficiency 87, DestroyChance 137,
        // ItemCurMana 107, ItemMaxMana 108 and UnlimitedUse 63. The player
        // augmentation is Int340 and has no dependency on the stone's WCID.
        let mut stone = source(37);
        stone.properties.ints.extend([
            Property { id: 107, value: 80 },
            Property {
                id: 108,
                value: 100,
            },
        ]);
        stone.properties.floats.extend([
            Property {
                id: 87,
                value: 0.75,
            },
            Property {
                id: 137,
                value: 0.25,
            },
        ]);
        stone.properties.bools.push(Property {
            id: 63,
            value: true,
        });
        let mut player = source(10);
        player.properties.ints.push(Property { id: 340, value: 2 });
        assert_eq!(
            mana(&stone),
            ManaCharge {
                current: Some(80),
                maximum: Some(100)
            }
        );
        assert_eq!(
            profile(&stone, &player).unwrap(),
            ManaStoneProfile {
                efficiency: Some(0.75),
                destroy_chance: Some(0.25),
                unlimited_use: true,
                mana_gain_rank: 2
            }
        );
    }
}
