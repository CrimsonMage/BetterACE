//! Source recall private notices and local announcement; canonical counters are
//! advanced only after the entire bounded response can be retained.
use super::*;
use bace_interactions::RecallError;
use bace_replication::{BatchLimits, InventoryProjection as P};
use bace_wire::SimpleGameEvent;
impl GameRuntime {
    pub(super) fn project_recall_output(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            if self.recalls.event.is_none() {
                self.recalls.event = self.simulation.recall_events().try_recv().ok();
            }
            let Some(event) = self.recalls.event.clone() else {
                break;
            };
            if self.recalls.bindings.matches_event(&event) {
                self.project_binding_event(event)?;
                self.recalls.event = None;
                continue;
            }
            let context = match &event {
                RecallEvent::Retry { context }
                | RecallEvent::Started { context, .. }
                | RecallEvent::Rejected { context, .. }
                | RecallEvent::Staged { context, .. }
                | RecallEvent::Cancelled { context } => *context,
                _ => return Err("binding recall event needs binding output owner; retained".into()),
            };
            let key = self
                .recalls
                .pending
                .iter()
                .find_map(|(key, p)| (p.context == context).then_some(*key))
                .ok_or("unrelated recall event retained")?;
            let p = &self.recalls.pending[&key];
            if matches!(
                event,
                RecallEvent::Rejected {
                    error: RecallError::Stale,
                    ..
                }
            ) {
                // Definitive authorization rejection has no accepted mutation
                // and no ACE success packet. It must not hold the shared lane.
                self.recalls.rejected.insert(key, RecallError::Stale);
                self.recalls.pending.remove(&key);
                self.recalls.event = None;
                continue;
            }
            if matches!(event, RecallEvent::Retry { .. }) {
                if !matches!(p.phase, Phase::Submitted) {
                    return Err("recall retry phase mismatch".into());
                }
                let p = self.recalls.pending.get_mut(&key).expect("matched recall");
                p.phase = Phase::Capture;
                p.prepared = None;
                self.recalls.event = None;
                continue;
            }
            if let RecallEvent::Staged { kind, .. } = &event
                && (*kind != p.kind || !matches!(p.phase, Phase::Running))
            {
                return Err("recall stage correlation mismatch".into());
            }
            if matches!(
                event,
                RecallEvent::Staged { .. } | RecallEvent::Cancelled { .. }
            ) {
                // Neither terminal has an ACE packet. A disconnect may have
                // removed the private recipient, so output pressure cannot
                // hold its owner or allocate a repeated announcement.
                self.recalls.pending.remove(&key);
                self.recalls.event = None;
                continue;
            }
            if self.visibility.service.pending()
                || self.network_output.len() >= self.limits.messages
                || !self.observer_room(1, 8192)
            {
                break;
            }
            let announcement = match event {
                RecallEvent::Started { kind, .. } => Some(announcement(&p.name, kind)),
                _ => None,
            };
            let terminal = match &event {
                RecallEvent::Started { kind, .. } => {
                    if !matches!(p.phase, Phase::Submitted) || *kind != p.kind {
                        return Err("recall start correlation mismatch".into());
                    }
                    false
                }
                RecallEvent::Rejected { .. } => true,
                _ => unreachable!(),
            };
            let steps = notice_steps(&event, announcement.as_deref())?;
            let limits = BatchLimits {
                max_messages: 8,
                max_bytes: self.limits.message_bytes,
                max_message_bytes: self.limits.message_bytes,
                max_string_bytes: 4096,
            };
            let objects = bace_wire::ObjectCodecLimits {
                max_message_bytes: self.limits.message_bytes,
                max_model_entries: 255,
                max_children: 128,
                max_restrictions: 1024,
                max_motion_commands: 32,
                max_string_bytes: 4096,
            };
            let r = self
                .players
                .replication(context.actor)
                .ok_or("recall canonical recipient missing")?;
            let mut batch = r
                .events
                .project_inventory_with_actor(
                    p.binding,
                    &steps,
                    &mut r.item_properties,
                    Some(&mut r.properties),
                    objects,
                    limits,
                )
                .map_err(|e| format!("recall output: {e:?}"))?;
            let mut observers: Vec<_> = if announcement.is_some() {
                batch.messages.last().cloned().into_iter().collect()
            } else {
                Vec::new()
            };
            if let RecallEvent::Started { motion, .. } = event {
                let packet = bace_replication::project_server_motion(
                    context.actor.0,
                    &recall_motion_view(motion),
                    &mut r.properties,
                    limits,
                )
                .map_err(|error| format!("recall motion projection: {error:?}"))?;
                batch.messages.push(packet.clone());
                observers.push(packet);
            }
            if !batch.messages.is_empty() {
                self.network_output.push_back(
                    crate::game_messages::session_batch_command(key, batch)
                        .map_err(|e| e.to_string())?,
                );
            }
            if !observers.is_empty() {
                self.retain_observer_messages(vec![(context.actor, observers)])?;
            }
            if terminal {
                self.recalls.pending.remove(&key);
            } else {
                self.recalls
                    .pending
                    .get_mut(&key)
                    .expect("matched recall")
                    .phase = Phase::Running;
            }
            self.recalls.event = None;
        }
        Ok(())
    }
}
fn recall_motion_view(motion: u32) -> bace_wire::MovementDescription {
    bace_wire::MovementDescription {
        autonomous: false,
        motion_flags: 0,
        current_style: 0x3d,
        body: bace_wire::MotionBody::State {
            state: bace_wire::InterpretedMotion {
                current_style: Some(0x3d),
                forward_command: Some(3),
                commands: vec![bace_wire::MotionCommandItem {
                    raw_command: motion as u16,
                    sequence: 0,
                    autonomous: false,
                    speed: 1.,
                }],
                ..Default::default()
            },
            sticky_object: None,
        },
    }
}
fn announcement(name: &str, kind: RecallKind) -> String {
    let phrase = match kind {
        RecallKind::Lifestone => "is recalling to the lifestone.",
        RecallKind::House => "is recalling home.",
        RecallKind::Marketplace => "is recalling to the marketplace.",
        RecallKind::AllegianceHometown => "is going to the Allegiance hometown.",
        RecallKind::AllegianceHousing => "is recalling to the Allegiance housing.",
        RecallKind::PkArena => "is going to the PK Arena.",
        RecallKind::PklArena => "is going to the PKL Arena.",
    };
    format!("{name} {phrase}")
}
fn error_code(error: RecallError) -> Option<u32> {
    Some(match error {
        RecallError::Olthoi => 0x593,
        RecallError::PkOnly => 0x55f,
        RecallError::PklOnly => 0x560,
        RecallError::PkRecent => 0x4cc,
        RecallError::TrainingAcademy => 0x55d,
        RecallError::Busy => 0x1d,
        RecallError::NoHouse => 0x47f,
        RecallError::NoAllegiance => 0x414,
        RecallError::NoHometown => 0x54c,
        RecallError::NoMansion => 0x480,
        RecallError::WrongHouseType => 0x481,
        RecallError::MansionClosed => 0x482,
        RecallError::MovedTooFar => 0x498,
        _ => return None,
    })
}

fn notice_steps<'a>(
    event: &RecallEvent,
    announcement: Option<&'a str>,
) -> Result<Vec<P<'a>>, String> {
    let mut steps = Vec::with_capacity(3);
    match event {
        RecallEvent::Started {
            mana_after,
            combat_mode_changed,
            ..
        } => {
            if let Some(current) = mana_after {
                steps.push(P::Vital {
                    vital: 6,
                    current: *current,
                });
            }
            if *combat_mode_changed {
                steps.push(P::PrivateProperty {
                    property: 40,
                    value: bace_wire::PropertyValue::Int(1),
                });
            }
            steps.push(P::System {
                text: announcement.ok_or("recall announcement missing")?,
                chat_type: 0x17,
            });
        }
        RecallEvent::Rejected { error, .. } => match error {
            RecallError::NoSanctuary => steps.push(P::System {
                text: "Your spirit has not been attuned to a sanctuary location.",
                chat_type: 0,
            }),
            RecallError::Disabled => steps.push(P::System {
                text: "This recall is disabled on this server.",
                chat_type: 0,
            }),
            error => steps.push(P::Simple(SimpleGameEvent::WeenieError(
                error_code(*error).ok_or_else(|| {
                    format!("recall owner infrastructure rejection retained: {error:?}")
                })?,
            ))),
        },
        _ => {}
    }
    Ok(steps)
}
#[cfg(test)]
mod tests;
