use super::*;
use bace_inventory::ActivationMessage;
use bace_replication::{BatchLimits, InventoryProjection};
use bace_wire::{CraftingEvent, ObjectCodecLimits, SimpleGameEvent};
impl GameRuntime {
    pub(super) fn project_skill_device_output(&mut self) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let Some(p) = self.skill_devices.pending.as_ref() else {
            return Ok(());
        };
        let key = p.key;
        let b = binding(p.context);
        let notice = if let Phase::Rejected(error) = p.phase {
            device_failure(error, p.quote.as_ref().map(|q| q.quote.device))?
        } else {
            None
        };
        let activation_notice = if let Phase::Rejected(
            bace_simulation::SkillDeviceError::Activation(failure),
        ) = p.phase
        {
            Some(
                failure
                    .message(p.quote.as_ref().map_or("", |q| q.name.as_str()))
                    .ok_or("skill device activation lacks source rejection output")?,
            )
        } else {
            None
        };
        if matches!(p.phase, Phase::Cancelled) {
            self.skill_devices.pending = None;
            return Ok(());
        }
        let inactive_rejection = matches!(p.phase, Phase::Rejected(_)) && p.quote.is_none();
        let mut steps = match &p.phase {
            Phase::Inactive => Vec::new(),
            Phase::Prompt(q) => prompt_steps(q)?,

            Phase::Rejected(_) if inactive_rejection => vec![InventoryProjection::Simple(
                SimpleGameEvent::UseDone(0x058d),
            )],
            Phase::Rejected(_) => match (&activation_notice, &notice) {
                (Some(ActivationMessage::Error(code)), _) => vec![InventoryProjection::Simple(
                    SimpleGameEvent::WeenieError(*code),
                )],
                (Some(ActivationMessage::ErrorWithString { error, text }), _) => {
                    vec![InventoryProjection::Group(
                        bace_wire::GroupEvent::ErrorWithString { code: *error, text },
                    )]
                }
                (Some(ActivationMessage::Transient(text)), _) => vec![InventoryProjection::Social(
                    bace_wire::SocialEvent::Transient(text),
                )],
                (None, Some(notice)) => vec![match &notice.text {
                    Some(text) => {
                        InventoryProjection::Group(bace_wire::GroupEvent::ErrorWithString {
                            code: notice.code,
                            text,
                        })
                    }
                    None => InventoryProjection::Simple(SimpleGameEvent::WeenieError(notice.code)),
                }],
                (None, None) => return Err("skill device rejection has no source output".into()),
            },
            Phase::Finished(_) => return self.project_skill_device_commit(),
            _ => return Ok(()),
        };
        if p.use_action
            && matches!(
                p.phase,
                Phase::Rejected(
                    bace_simulation::SkillDeviceError::Domain(_)
                        | bace_simulation::SkillDeviceError::Skill(
                            bace_simulation::SkillActionError::Domain(_)
                        )
                        | bace_simulation::SkillDeviceError::Confirmation
                )
            )
            && let Some(text) = p.quote.as_ref().and_then(|q| q.activation_talk.as_deref())
        {
            // ActOnUse can reject a skill and return; base OnActivate still
            // invokes OnTalk. Generic activation failures return before both.
            steps.push(InventoryProjection::System { text, chat_type: 0 });
        }
        // Player.TryUseItem queues UseDone after OnActivate returns. A response
        // invokes ActOnUse directly; decline and confirmed failures add no UseDone.
        if p.use_action && !inactive_rejection {
            steps.push(InventoryProjection::Simple(SimpleGameEvent::UseDone(0)));
        }
        let replica = self
            .players
            .replication(b.actor)
            .ok_or("device canonical counters missing")?;
        if replica.key != key {
            return Err("device sequence binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory(
                b,
                &steps,
                &mut replica.item_properties,
                objects(max),
                limits(max),
            )
            .map_err(|e| format!("device projection: {e:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        let p = self.skill_devices.pending.take().expect("projected owner");
        if let Phase::Prompt(q) = p.phase {
            self.skill_devices.quotes.insert(key, q);
        }
        Ok(())
    }
    fn project_skill_device_commit(&mut self) -> Result<(), String> {
        use bace_replication::skill_devices::{
            SkillDeviceKind, SkillDeviceView, project_skill_device,
        };
        let p = self
            .skill_devices
            .pending
            .as_ref()
            .ok_or("device result owner missing")?;
        let Phase::Finished(completion) = &p.phase else {
            return Ok(());
        };
        if !completion.committed {
            // A durable rejection has already released exactly this owner. No
            // gameplay-success packets or consumption are fabricated.
            self.skill_devices
                .failures
                .insert(p.key, "skill device transaction rejected".into());
            self.skill_devices.pending = None;
            return Ok(());
        }
        let SkillSaveOwner::Device(ticket) = &completion.owner else {
            return Err("device result kind mismatch".into());
        };
        let quote = p.quote.as_ref().ok_or("device source quote missing")?;
        let kind = match quote.quote.device {
            PreparedSkillDevice::Specialize(_) => SkillDeviceKind::Specialize,
            PreparedSkillDevice::Lower(_) => SkillDeviceKind::Lower,
            PreparedSkillDevice::Augment { .. } => SkillDeviceKind::Augment,
        };
        if kind == SkillDeviceKind::Augment && !self.observer_room(2, 16384) {
            return Ok(());
        }
        let change = ticket.skill.change;
        let item_change = ticket
            .inventory
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == p.item)
            .ok_or("device consumption missing")?;
        let consumption = if item_change.after.place == bace_inventory::ItemPlace::Removed {
            vec![InventoryProjection::Remove(p.item)]
        } else if item_change
            .before
            .as_ref()
            .is_some_and(|before| before.stack == item_change.after.stack + 1)
        {
            vec![InventoryProjection::Stack {
                item: p.item,
                quantity: item_change.after.stack,
                value: item_change
                    .after
                    .unit_value
                    .checked_mul(item_change.after.stack)
                    .ok_or("device stack value overflow")?,
            }]
        } else {
            return Err("device consumed unexpected quantity".into());
        };
        let (saved, _, _) = self
            .online_saves
            .baseline(p.context.actor.0)
            .ok_or("device committed actor baseline missing")?;
        let actor_name = saved
            .player
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 1)
            .map(|p| p.value.as_str())
            .ok_or("device actor name missing")?;
        let key = p.key;
        let b = binding(p.context);
        let replica = self
            .players
            .replication(b.actor)
            .ok_or("device canonical counters missing")?;
        if replica.key != key {
            return Err("device committed binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let result = project_skill_device(
            &mut replica.events,
            b,
            SkillDeviceView {
                kind,
                change: bace_gameplay_api::SkillTrainingChange {
                    before: change.before,
                    after: change.after,
                    available_skill_credits: change.available_skill_credits,
                    revision: change.revision,
                },
                available_experience: change.available_experience,
                actor_name,
                device_name: &quote.name,
            },
            consumption,
            &mut replica.item_properties,
            &mut replica.properties,
            objects(max),
            limits(max),
        )
        .map_err(|e| format!("committed device projection: {e:?}"))?;
        if item_change.after.place == bace_inventory::ItemPlace::Removed {
            replica.item_properties.remove(&p.item);
        }
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: result
                    .owner
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        if !result.observers.is_empty() {
            self.retain_observer_messages(vec![(b.actor, result.observers)])?;
        }
        self.skill_devices.pending = None;
        Ok(())
    }
}

pub(super) fn prompt_steps(q: &Quote) -> Result<Vec<InventoryProjection<'_>>, String> {
    let mut steps = vec![InventoryProjection::Crafting(
        CraftingEvent::ConfirmationRequest {
            confirmation_type: if matches!(q.quote.device, PreparedSkillDevice::Augment { .. }) {
                6
            } else {
                2
            },
            context: u32::try_from(q.quote.token).map_err(|_| "device token overflow")?,
            text: &q.prompt,
        },
    )];
    // ACE OnActivate invokes ActOnUse before OnTalk; the outer player UseDone
    // follows both. Keep these in one bounded ordered projection.
    if let Some(text) = q.activation_talk.as_deref() {
        steps.push(InventoryProjection::System { text, chat_type: 0 });
    }
    Ok(steps)
}

pub(super) fn objects(max: usize) -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_string_bytes: 4096,
        max_model_entries: 4096,
        max_children: 1024,
        max_restrictions: 1024,
        max_motion_commands: 1024,
        max_message_bytes: max,
    }
}
pub(super) fn limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4096,
        max_message_bytes: max,
        max_bytes: 64 * 1024 * 1024,
        max_string_bytes: 4096,
    }
}

fn device_failure(
    error: bace_simulation::SkillDeviceError,
    device: Option<PreparedSkillDevice>,
) -> Result<Option<bace_replication::skill_devices::SkillDeviceNotice>, String> {
    use bace_character::SkillTransitionError as D;
    use bace_gameplay_api::SkillTrainingRejection as T;
    use bace_replication::skill_devices::SkillDeviceFailure as F;
    use bace_simulation::SkillDeviceError as E;
    if error == E::Olthoi {
        return Ok(Some(bace_replication::skill_devices::SkillDeviceNotice {
            code: 0x587,
            text: None,
        }));
    }
    let Some(device) = device else {
        return Ok(None);
    };
    let skill = match device {
        PreparedSkillDevice::Specialize(s)
        | PreparedSkillDevice::Lower(s)
        | PreparedSkillDevice::Augment { skill: s, .. } => s,
    };
    let error = match error {
        E::Skill(bace_simulation::SkillActionError::Domain(e)) => E::Domain(e),
        other => other,
    };
    let failure = match error {
        E::Confirmation => F::ConfirmationInProgress,
        E::Domain(D::WieldRequirement) => F::WieldRequirement,
        E::Domain(D::AlreadyAugmented) => F::AlreadyAugmented,
        E::Domain(D::InsufficientExperience) => F::AugmentationNotEnoughExperience,
        E::Domain(D::Training(T::UnknownSkill)) => F::UnknownSkill,
        E::Domain(D::Training(T::InsufficientCredits)) => F::NotEnoughCredits,
        E::Domain(D::Training(T::SpecializationCap)) => F::SpecializedCreditLimit,
        E::Domain(D::Training(T::NotTrained | T::AlreadyTrained)) => match device {
            PreparedSkillDevice::Specialize(_) => F::MustBeTrained,
            PreparedSkillDevice::Lower(_) => F::AlreadyUntrained,
            PreparedSkillDevice::Augment { .. } => F::AugmentationNotTrained,
        },
        _ => return Ok(None),
    };
    bace_replication::skill_devices::skill_device_failure(skill, failure)
        .map(Some)
        .map_err(|e| format!("device error projection: {e:?}"))
}
