use super::*;
impl VisibilityService {
    /// Allocate one canonical nonplayer motion sequence. The caller retains
    /// these exact bytes while reliable observer publication catches up.
    pub fn project_object_motion(
        &mut self,
        object: EntityId,
        view: &bace_wire::MovementDescription,
    ) -> Result<ReplicationMessage, VisibilityServiceError> {
        let source = self
            .objects
            .get_mut(&object)
            .ok_or(VisibilityServiceError::Stale)?;
        bace_replication::project_server_motion(
            object.0,
            view,
            &mut source.sequences,
            bace_replication::BatchLimits {
                max_messages: 1,
                max_bytes: self.limits.codec.max_message_bytes,
                max_message_bytes: self.limits.codec.max_message_bytes,
                max_string_bytes: self.limits.codec.max_string_bytes,
            },
        )
        .map_err(|_| VisibilityServiceError::Projection)
    }
    pub fn accept_views(
        &mut self,
        outcome: &ObjectViewOutcome,
        players: &mut PlayerService,
    ) -> Result<(), VisibilityServiceError> {
        self.accept_views_inner(outcome, Some(players))
    }
    pub(super) fn accept_views_inner(
        &mut self,
        outcome: &ObjectViewOutcome,
        mut players: Option<&mut PlayerService>,
    ) -> Result<(), VisibilityServiceError> {
        let Some((&key, _)) = self
            .observers
            .iter()
            .find(|(_, o)| o.view_query == Some(outcome.correlation))
        else {
            return Ok(());
        };
        self.observers.get_mut(&key).expect("found").view_query = None;
        let delta = self.observers[&key]
            .knowledge
            .pending()
            .ok_or(VisibilityServiceError::ProjectionDetail {
                object: 0,
                reason: "object view arrived without staged knowledge".into(),
            })?
            .clone();
        let snapshot = match &outcome.result {
            Ok(s)
                if s.binding == delta.binding
                    && s.observer_epoch == delta.observer_epoch
                    && s.tick >= delta.tick =>
            {
                s
            }
            _ => return self.discard(key, delta.ticket),
        };
        if snapshot
            .views
            .iter()
            .any(|(id, view)| view.is_err() || !self.objects.contains_key(id))
        {
            return self.discard(key, delta.ticket);
        }
        // A player teleport has a retained F751/F748/state obligation. Generic
        // visibility must not consume its epoch first, or publish a future pose
        // before that canonical sequence. Older in-flight views are also stale.
        for (id, view) in &snapshot.views {
            if let Some(r) = players.as_mut().and_then(|p| p.replication(*id))
                && view.as_ref().is_ok_and(|view| {
                    view.epoch
                        != r.properties
                            .current(bace_replication::SequenceKind::ObjectTeleport, 0)
                })
            {
                return self.discard(key, delta.ticket);
            }
        }
        // Prepare the whole accepted snapshot before advancing any shared
        // object/player sequence owner. A transient incomplete movement view
        // must release this unpublished knowledge ticket, not leave the
        // observer permanently Busy after a partial projection.
        let mut prepared = Vec::with_capacity(snapshot.views.len());
        for (id, view) in &snapshot.views {
            let view = view.as_ref().expect("checked");
            let object = self.objects.get(id).expect("checked");
            if snapshot.tick < object.tick {
                prepared.push((*id, None));
                continue;
            }
            let source = &object.blueprint.description;
            let public_state = players
                .as_mut()
                .and_then(|p| p.replication(*id))
                .and_then(|r| r.public_physics_state);
            let force = *id == delta.binding.actor && self.observers[&key].force_self;
            if !force
                && object.projection.as_ref().is_some_and(|p| {
                    object.projection_revision == object.blueprint.revision
                        && p.view == *view
                        && p.description.model == source.model
                        && p.description.game == source.game
                        && public_state.is_none_or(|state| {
                            p.description.physics.state
                                == (state | if view.held { 0x01000000 } else { 0 })
                        })
                })
            {
                prepared.push((*id, None));
                continue;
            }
            let (mut sequences, public_state) =
                if let Some(replication) = players.as_mut().and_then(|p| p.replication(*id)) {
                    (
                        replication.properties.proposal_copy(),
                        replication.public_physics_state,
                    )
                } else {
                    (object.sequences.proposal_copy(), None)
                };
            let projection = match ObjectProjection::prepare_with_public_state(
                source,
                view.clone(),
                object.projection.as_ref(),
                &mut sequences,
                self.limits.codec,
                force,
                public_state,
            ) {
                Ok(projection) => projection,
                Err(bace_replication::SessionProjectionError::InvalidProjection) => {
                    // The owner may be between a server-movement transition
                    // and its next complete view. A new query can retry it;
                    // no wire bytes or sequence changes have been committed.
                    return self.discard(key, delta.ticket);
                }
                Err(error) => {
                    self.discard(key, delta.ticket)?;
                    return Err(VisibilityServiceError::ProjectionDetail {
                        object: id.0,
                        reason: format!("accepted object view: {error:?}"),
                    });
                }
            };
            prepared.push((*id, Some((projection, sequences, force))));
        }
        for (id, result) in prepared {
            let object = self.objects.get_mut(&id).expect("checked");
            let Some((projection, sequences, force)) = result else {
                if snapshot.tick >= object.tick {
                    object.tick = snapshot.tick;
                }
                continue;
            };
            if let Some(replication) = players.as_mut().and_then(|p| p.replication(id)) {
                replication.properties = sequences;
            } else {
                object.sequences = sequences;
            }
            object.version = object
                .version
                .checked_add(1)
                .ok_or(VisibilityServiceError::Capacity)?;
            object.tick = snapshot.tick;
            object.projection = Some(projection);
            object.projection_revision = object.blueprint.revision;
            if force {
                self.observers.get_mut(&key).expect("observer").force_self = false;
            }
        }
        let o = &self.observers[&key];
        let mut messages = Vec::new();
        let mut sent = BTreeMap::new();
        let mut self_sent = None;
        for id in &delta.removes {
            if let Some(old) = o.sent.get(id) {
                delivery::append_deletes(&mut messages, &old.blueprint);
            }
        }
        for (id, _) in &snapshot.views {
            let object = &self.objects[id];
            let p = object
                .projection
                .as_ref()
                .ok_or(VisibilityServiceError::ProjectionDetail {
                    object: id.0,
                    reason: "accepted view has no prepared projection".into(),
                })?;
            if *id == o.binding.actor {
                if o.self_sent.as_ref().is_none_or(|old| {
                    old.version != object.version
                        || old.blueprint.incarnation != object.blueprint.incarnation
                }) {
                    messages.extend(p.refresh_messages(self.limits.codec).map_err(|error| {
                        VisibilityServiceError::ProjectionDetail {
                            object: id.0,
                            reason: format!("self refresh: {error:?}"),
                        }
                    })?);
                    self_sent = Some(Sent {
                        blueprint: object.blueprint.clone(),
                        version: object.version,
                    });
                }
                continue;
            }
            let create = delta.creates.binary_search(id).is_ok();
            if create {
                messages.push(ReplicationMessage {
                    queue: 10,
                    bytes: p.create.clone(),
                });
                for child in &object.blueprint.children {
                    messages.push(ReplicationMessage {
                        queue: 10,
                        bytes: child.encode_create(self.limits.codec).map_err(|error| {
                            VisibilityServiceError::ProjectionDetail {
                                object: child.object_id,
                                reason: format!("new attachment create: {error:?}"),
                            }
                        })?,
                    });
                }
            } else if let Some(previous) = o.sent.get(id) {
                if previous.blueprint.revision != object.blueprint.revision {
                    for old in &previous.blueprint.children {
                        if !object.blueprint.children.iter().any(|child| {
                            child.object_id == old.object_id
                                && child.physics.sequences.instance
                                    == old.physics.sequences.instance
                        }) {
                            messages.push(ReplicationMessage {
                                queue: 10,
                                bytes: ObjectControl::Delete {
                                    object_id: old.object_id,
                                    instance_sequence: old.physics.sequences.instance,
                                }
                                .encode(),
                            });
                        }
                    }
                    for child in &object.blueprint.children {
                        match previous.blueprint.children.iter().find(|old| {
                            old.object_id == child.object_id
                                && old.physics.sequences.instance
                                    == child.physics.sequences.instance
                        }) {
                            None => messages.push(ReplicationMessage {
                                queue: 10,
                                bytes: child.encode_create(self.limits.codec).map_err(|error| {
                                    VisibilityServiceError::ProjectionDetail {
                                        object: child.object_id,
                                        reason: format!("changed attachment create: {error:?}"),
                                    }
                                })?,
                            }),
                            Some(old) if **old != **child => messages.push(ReplicationMessage {
                                queue: 10,
                                bytes: child.encode_update(self.limits.codec).map_err(|error| {
                                    VisibilityServiceError::ProjectionDetail {
                                        object: child.object_id,
                                        reason: format!("changed attachment update: {error:?}"),
                                    }
                                })?,
                            }),
                            _ => {}
                        }
                    }
                    messages.push(ReplicationMessage {
                        queue: 10,
                        bytes: p
                            .description
                            .encode_update(self.limits.codec)
                            .map_err(|error| VisibilityServiceError::ProjectionDetail {
                                object: id.0,
                                reason: format!("changed root update: {error:?}"),
                            })?,
                    });
                } else if previous.version != object.version {
                    messages.extend(p.refresh_messages(self.limits.codec).map_err(|error| {
                        VisibilityServiceError::ProjectionDetail {
                            object: id.0,
                            reason: format!("observer refresh: {error:?}"),
                        }
                    })?);
                }
            }
            if create
                || o.sent.get(id).is_none_or(|old| {
                    old.version != object.version
                        || old.blueprint.revision != object.blueprint.revision
                })
            {
                sent.insert(
                    *id,
                    Sent {
                        blueprint: object.blueprint.clone(),
                        version: object.version,
                    },
                );
            }
        }
        match self.publish_with_self(key, delta.ticket, messages, sent, delta.removes, self_sent) {
            Err(VisibilityServiceError::Capacity) => {
                // Nothing was submitted. Keep the exact staged knowledge plan;
                // reacquire accepted views when retained reliable capacity frees.
                let correlation = self.next()?;
                let observer = self.observers.get_mut(&key).expect("found");
                observer.view_request = Some(ObjectViewRequest {
                    correlation,
                    binding: observer.binding,
                    entities: snapshot.views.iter().map(|(id, _)| *id).collect(),
                });
                Ok(())
            }
            other => other,
        }
    }
    fn discard(&mut self, key: SessionKey, ticket: u64) -> Result<(), VisibilityServiceError> {
        let o = self.observers.get_mut(&key).expect("found");
        if let Some(snapshot) = o.knowledge.discard_unpublished(ticket).map_err(|error| {
            VisibilityServiceError::ProjectionDetail {
                object: o.binding.actor.0,
                reason: format!("unpublished view discard: {error:?}"),
            }
        })? {
            o.buffer = snapshot.candidates;
        }
        Ok(())
    }
}
