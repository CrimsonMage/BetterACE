use super::*;
use bace_character::AttributeTransferError;
use bace_inventory::ActivationMessage;
use bace_replication::{BatchLimits, InventoryProjection};
use bace_wire::{CraftingEvent, ObjectCodecLimits, SimpleGameEvent};

impl GameRuntime {
    pub(super) fn project_attribute_transfer_output(&mut self) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let Some(pending) = self.attribute_transfers.pending.as_ref() else {
            return Ok(());
        };
        if matches!(pending.phase, Phase::Cancelled) {
            self.attribute_transfers.pending = None;
            return Ok(());
        }
        if matches!(pending.phase, Phase::Finished(_)) {
            return self.project_committed_attribute_transfer();
        }
        let inactive_rejection =
            matches!(pending.phase, Phase::Rejected(_)) && pending.quote.is_none();
        let failure = if let Phase::Rejected(error) = pending.phase {
            Some(failure(
                error,
                pending
                    .quote
                    .as_ref()
                    .map(|quote| quote.confirmation.device),
            )?)
        } else {
            None
        };
        let activation = if let Phase::Rejected(
            bace_simulation::AttributeTransferDeviceError::Activation(error),
        ) = pending.phase
        {
            Some(
                error
                    .message(
                        pending
                            .quote
                            .as_ref()
                            .map_or("", |quote| quote.name.as_str()),
                    )
                    .ok_or("attribute transfer activation has no source output")?,
            )
        } else {
            None
        };
        let mut steps = match &pending.phase {
            Phase::Inactive => Vec::new(),
            Phase::Rejected(_) if inactive_rejection => vec![InventoryProjection::Simple(
                SimpleGameEvent::UseDone(0x058d),
            )],
            Phase::Prompt(quote) => vec![InventoryProjection::Crafting(
                CraftingEvent::ConfirmationRequest {
                    confirmation_type: 3,
                    context: u32::try_from(quote.confirmation.token)
                        .map_err(|_| "attribute transfer token overflow")?,
                    text: &quote.prompt,
                },
            )],
            Phase::Rejected(_) => match (activation.as_ref(), failure.as_ref()) {
                (Some(ActivationMessage::Error(code)), _) => {
                    vec![InventoryProjection::Simple(SimpleGameEvent::WeenieError(
                        *code,
                    ))]
                }
                (Some(ActivationMessage::ErrorWithString { error, text }), _) => {
                    vec![InventoryProjection::Group(
                        bace_wire::GroupEvent::ErrorWithString { code: *error, text },
                    )]
                }
                (Some(ActivationMessage::Transient(text)), _) => {
                    vec![InventoryProjection::Social(
                        bace_wire::SocialEvent::Transient(text),
                    )]
                }
                (None, Some((code, text))) => vec![match text {
                    Some(text) => {
                        InventoryProjection::Group(bace_wire::GroupEvent::ErrorWithString {
                            code: *code,
                            text,
                        })
                    }
                    None => InventoryProjection::Simple(SimpleGameEvent::WeenieError(*code)),
                }],
                (None, None) => return Err("attribute transfer rejection has no output".into()),
            },
            _ => return Ok(()),
        };
        if pending.use_action && !inactive_rejection {
            steps.push(InventoryProjection::Simple(SimpleGameEvent::UseDone(0)));
        }
        let binding = binding(pending.context);
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("attribute transfer canonical counters missing")?;
        if replica.key != pending.key {
            return Err("attribute transfer sequence binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory(
                binding,
                &steps,
                &mut replica.item_properties,
                objects(max),
                limits(max),
            )
            .map_err(|error| format!("attribute transfer projection: {error:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key: pending.key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|message| (message.queue, message.bytes))
                    .collect(),
            });
        let pending = self
            .attribute_transfers
            .pending
            .take()
            .expect("projected owner");
        if let Phase::Prompt(quote) = pending.phase {
            self.attribute_transfers.quotes.insert(pending.key, quote);
        }
        Ok(())
    }

    fn project_committed_attribute_transfer(&mut self) -> Result<(), String> {
        let pending = self
            .attribute_transfers
            .pending
            .as_ref()
            .ok_or("attribute transfer result owner missing")?;
        let Phase::Finished(completion) = &pending.phase else {
            return Ok(());
        };
        if !completion.committed {
            self.attribute_transfers.failures.insert(
                pending.key,
                "attribute transfer transaction rejected".into(),
            );
            self.attribute_transfers.pending = None;
            return Ok(());
        }
        let SkillSaveOwner::AttributeTransfer(ticket) = &completion.owner else {
            return Err("attribute transfer result kind mismatch".into());
        };
        let item_change = ticket
            .inventory
            .proposal
            .changes
            .iter()
            .find(|change| change.after.id == pending.item)
            .ok_or("attribute transfer consumption missing")?;
        let consumption = if item_change.after.place == bace_inventory::ItemPlace::Removed {
            vec![InventoryProjection::Remove(pending.item)]
        } else if item_change
            .before
            .as_ref()
            .is_some_and(|before| before.stack == item_change.after.stack + 1)
        {
            vec![InventoryProjection::Stack {
                item: pending.item,
                quantity: item_change.after.stack,
                value: item_change
                    .after
                    .unit_value
                    .checked_mul(item_change.after.stack)
                    .ok_or("attribute transfer stack value overflow")?,
            }]
        } else {
            return Err("attribute transfer consumed unexpected quantity".into());
        };
        let binding = binding(pending.context);
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("attribute transfer canonical counters missing")?;
        if replica.key != pending.key {
            return Err("attribute transfer committed binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let batch = bace_replication::attribute_transfer::project_attribute_transfer(
            &mut replica.events,
            binding,
            ticket.character.proposal.from_after,
            ticket.character.proposal.to_after,
            consumption,
            &mut replica.item_properties,
            &mut replica.properties,
            objects(max),
            limits(max),
        )
        .map_err(|error| format!("committed attribute transfer projection: {error:?}"))?;
        if item_change.after.place == bace_inventory::ItemPlace::Removed {
            replica.item_properties.remove(&pending.item);
        }
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key: pending.key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|message| (message.queue, message.bytes))
                    .collect(),
            });
        self.attribute_transfers.pending = None;
        Ok(())
    }
}

fn objects(max: usize) -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_string_bytes: 4096,
        max_model_entries: 4096,
        max_children: 1024,
        max_restrictions: 1024,
        max_motion_commands: 1024,
        max_message_bytes: max,
    }
}
fn limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4096,
        max_message_bytes: max,
        max_bytes: 64 * 1024 * 1024,
        max_string_bytes: 4096,
    }
}

/// Error text follows pinned AttributeTransferDevice.VerifyRequirements,
/// including its source's TransferFromAttribute wording for the to-high case.
fn failure(
    error: bace_simulation::AttributeTransferDeviceError,
    device: Option<PreparedAttributeTransfer>,
) -> Result<(u32, Option<String>), String> {
    use bace_simulation::AttributeTransferDeviceError as E;
    match error {
        E::Domain(AttributeTransferError::WieldRequirement) => Ok((0x04e0, None)),
        E::Domain(AttributeTransferError::SourceAtMinimum) => {
            let source = preparation::name(device.ok_or("attribute source quote missing")?.from);
            Ok((
                0x04de,
                Some(format!(
                    "Your innate level of {source} is already as low as it can be. You may not reduce it any further."
                )),
            ))
        }
        E::Domain(AttributeTransferError::TargetAtMaximum) => {
            let source = preparation::name(device.ok_or("attribute source quote missing")?.from);
            Ok((
                0x04df,
                Some(format!(
                    "Your innate level of {source} is already as high as it can be. You may not increase it any further."
                )),
            ))
        }
        E::Content | E::Ownership | E::MissingWieldProfile | E::Stale => Ok((0x04dd, None)),
        E::Confirmation => Ok((0x048f, None)),
        E::Activation(_) => Ok((0x04dd, None)),
        _ => Err(format!(
            "attribute transfer owner obligation retained: {error:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_attribute_transfer_prompt_and_error_text() {
        let device = PreparedAttributeTransfer::source(1, 2).unwrap();
        assert_eq!(
            preparation::prompt(device),
            "This action will transfer 10 points from your Strength to your Endurance."
        );
        assert_eq!(
            failure(
                bace_simulation::AttributeTransferDeviceError::Domain(
                    AttributeTransferError::WieldRequirement,
                ),
                Some(device),
            )
            .unwrap(),
            (0x04e0, None)
        );
        assert_eq!(
            failure(
                bace_simulation::AttributeTransferDeviceError::Domain(
                    AttributeTransferError::SourceAtMinimum,
                ),
                Some(device),
            )
            .unwrap(),
            (
                0x04de,
                Some("Your innate level of Strength is already as low as it can be. You may not reduce it any further.".into()),
            )
        );
        // The pinned source accidentally names TransferFromAttribute here.
        assert_eq!(
            failure(
                bace_simulation::AttributeTransferDeviceError::Domain(
                    AttributeTransferError::TargetAtMaximum,
                ),
                Some(device),
            )
            .unwrap(),
            (
                0x04df,
                Some("Your innate level of Strength is already as high as it can be. You may not increase it any further.".into()),
            )
        );
    }
}
