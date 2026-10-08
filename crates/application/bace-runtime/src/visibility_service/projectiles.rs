//! Delayed cold preparation cannot erase short-flight Create evidence. This is
//! an output continuation only; accepted pose/PVS remain simulation-owned.
use super::*;
struct Recipient {
    key: SessionKey,
    observer: ProjectileLaunchObserver,
    done: bool,
}
pub(super) struct RetainedLaunch {
    snapshot: Arc<AcceptedProjectileLaunch>,
    blueprint: Arc<Blueprint>,
    creates: Vec<Vec<u8>>,
    recipients: Vec<Recipient>,
    bytes: usize,
}
impl VisibilityService {
    pub fn publish_object_birth(
        &mut self,
        birth: Arc<AcceptedObjectBirth>,
        players: &mut PlayerService,
    ) -> Result<bool, VisibilityServiceError> {
        self.publish_projectile_launch_inner(birth, Some(players))
    }

    /// Retain the producer's Created event until every original recipient either
    /// admits its exact Create batch or leaves the original session/teleport.
    pub fn publish_projectile_launch(
        &mut self,
        snapshot: Arc<AcceptedProjectileLaunch>,
        players: &mut PlayerService,
    ) -> Result<bool, VisibilityServiceError> {
        self.publish_projectile_launch_inner(snapshot, Some(players))
    }
    pub(super) fn publish_projectile_launch_inner(
        &mut self,
        snapshot: Arc<AcceptedProjectileLaunch>,
        mut players: Option<&mut PlayerService>,
    ) -> Result<bool, VisibilityServiceError> {
        if let Some(players) = players.as_deref_mut() {
            let bindings: Vec<_> = players.entered_bindings().collect();
            for (key, binding) in bindings {
                self.bind(key, binding)?;
            }
        }
        let id = snapshot.view.entity;
        if snapshot.observers.len() > 4096
            || snapshot
                .observers
                .windows(2)
                .any(|p| p[0].binding.actor >= p[1].binding.actor)
        {
            return Err(VisibilityServiceError::Projection);
        }
        if let Some(old) = self.launches.get(&id) {
            if *old.snapshot != *snapshot {
                return Err(VisibilityServiceError::Stale);
            }
        } else {
            if self.launches.len() >= 4096 {
                return Err(VisibilityServiceError::Capacity);
            }
            let object = self
                .objects
                .get_mut(&id)
                .ok_or(VisibilityServiceError::InvalidDescription)?;
            let projection = ObjectProjection::prepare(
                &object.blueprint.description,
                snapshot.view.clone(),
                None,
                &mut object.sequences,
                self.limits.codec,
            )
            .map_err(|_| VisibilityServiceError::Projection)?;
            let recipients: Vec<_> = snapshot
                .observers
                .iter()
                .filter_map(|observer| {
                    self.observers
                        .iter()
                        .find(|(_, o)| o.binding == observer.binding)
                        .map(|(&key, _)| Recipient {
                            key,
                            observer: *observer,
                            done: false,
                        })
                })
                .collect();
            let mut creates = vec![projection.create];
            for child in &object.blueprint.children {
                creates.push(
                    child
                        .encode_create(self.limits.codec)
                        .map_err(|_| VisibilityServiceError::Projection)?,
                );
            }
            let bytes = creates.iter().map(Vec::len).sum::<usize>()
                + snapshot.observers.len() * std::mem::size_of::<ProjectileLaunchObserver>()
                + recipients.len() * std::mem::size_of::<Recipient>();
            if bytes
                > self
                    .limits
                    .retained_bytes
                    .saturating_sub(self.retained_bytes)
            {
                return Err(VisibilityServiceError::Capacity);
            }
            for recipient in &recipients {
                self.observers
                    .get_mut(&recipient.key)
                    .expect("matched")
                    .launches
                    .insert(id);
            }
            self.retained_bytes += bytes;
            self.launches.insert(
                id,
                RetainedLaunch {
                    snapshot: snapshot.clone(),
                    blueprint: object.blueprint.clone(),
                    creates,
                    recipients,
                    bytes,
                },
            );
        }
        // Temporarily detach this output-only record so publishing an observer
        // batch can borrow the service. Every path below restores or completes it.
        let mut launch = self.launches.remove(&id).expect("inserted");
        let result = (|| {
            for recipient in &mut launch.recipients {
                if recipient.done {
                    continue;
                }
                let Some(o) = self.observers.get_mut(&recipient.key) else {
                    recipient.done = true;
                    continue;
                };
                let require_binding = players.is_some();
                let epoch = players
                    .as_mut()
                    .and_then(|p| p.replication(o.binding.actor))
                    .map(|r| {
                        r.properties
                            .current(bace_replication::SequenceKind::ObjectTeleport, 0)
                    });
                if o.binding.actor == id
                    || o.failed.is_some()
                    || require_binding && epoch.is_none()
                    || o.binding != recipient.observer.binding
                    || epoch.is_some_and(|e| e != recipient.observer.epoch)
                {
                    o.launches.remove(&id);
                    recipient.done = true;
                    continue;
                }
                if o.knowledge.knows(id)
                    && o.sent
                        .get(&id)
                        .is_some_and(|s| s.blueprint.incarnation == launch.blueprint.incarnation)
                {
                    o.launches.remove(&id);
                    recipient.done = true;
                    continue;
                }
                if o.query.is_some()
                    || o.view_query.is_some()
                    || o.view_request.is_some()
                    || o.publication.is_some()
                    || o.reset_requested.is_some()
                {
                    continue;
                }
                match o.knowledge.stage_projectile(
                    id,
                    recipient.observer.epoch,
                    snapshot.tick,
                    recipient.observer.distance_squared,
                ) {
                    Ok(Some(_)) => (),
                    Ok(None) | Err(bace_replication::SpatialVisibilityError::StaleSnapshot) => {
                        o.launches.remove(&id);
                        recipient.done = true;
                        continue;
                    }
                    Err(bace_replication::SpatialVisibilityError::Busy) => continue,
                    Err(_) => return Err(VisibilityServiceError::Capacity),
                };
                if o.sent
                    .get(&id)
                    .is_some_and(|old| old.blueprint.incarnation != launch.blueprint.incarnation)
                {
                    o.knowledge
                        .recreate_pending(id)
                        .map_err(|_| VisibilityServiceError::Projection)?;
                }
                let delta = o.knowledge.pending().expect("staged").clone();
                let mut messages = Vec::new();
                for removed in &delta.removes {
                    if let Some(old) = o.sent.get(removed) {
                        delivery::append_deletes(&mut messages, &old.blueprint);
                    }
                }
                if !delta.creates.is_empty() {
                    messages.extend(
                        launch
                            .creates
                            .iter()
                            .cloned()
                            .map(|bytes| ReplicationMessage { queue: 10, bytes }),
                    );
                }
                let sent = BTreeMap::from([(
                    id,
                    Sent {
                        blueprint: launch.blueprint.clone(),
                        version: 0,
                    },
                )]);
                match self.publish(recipient.key, delta.ticket, messages, sent, delta.removes) {
                    Ok(()) => {}
                    Err(VisibilityServiceError::Capacity) => {
                        self.observers
                            .get_mut(&recipient.key)
                            .expect("present")
                            .knowledge
                            .discard_unpublished(delta.ticket)
                            .map_err(|_| VisibilityServiceError::Projection)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(launch.recipients.iter().all(|r| r.done))
        })();
        if result == Ok(true) {
            self.retained_bytes -= launch.bytes;
        } else {
            self.launches.insert(id, launch);
        }
        result
    }
}
