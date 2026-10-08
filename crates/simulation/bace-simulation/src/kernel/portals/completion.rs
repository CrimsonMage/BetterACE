use super::*;
impl Kernel {
    pub fn confirm_portal_committed(
        &mut self,
        receipt: &PortalServiceReceipt,
    ) -> Result<(), PortalError> {
        if self
            .portals
            .pending
            .get(&receipt.operation)
            .is_some_and(|p| matches!(p.ticket.origin, PortalServiceOrigin::Emote { .. }))
        {
            return Err(PortalError::Conflict);
        }
        self.confirm_portal_committed_inner(receipt)
    }
    pub(in crate::kernel) fn confirm_portal_committed_inner(
        &mut self,
        receipt: &PortalServiceReceipt,
    ) -> Result<(), PortalError> {
        let pending = self
            .portals
            .pending
            .get_mut(&receipt.operation)
            .ok_or(PortalError::Invalid)?;
        let ticket = &pending.ticket;
        if ticket.actor != receipt.actor
            || ticket.after_revision != receipt.after_revision
            || receipt.revisions.len() != ticket.participants.len()
            || ticket.participants.iter().any(|(actor, _, after)| {
                receipt
                    .revisions
                    .iter()
                    .filter(|(id, revision)| id == actor && revision == after)
                    .count()
                    != 1
            })
        {
            return Err(PortalError::Conflict);
        }
        pending.committed = true;
        Ok(())
    }
    pub fn reject_portal_proposal(&mut self, operation: u64) -> Result<(), PortalError> {
        if self
            .portals
            .pending
            .get(&operation)
            .is_some_and(|p| matches!(p.ticket.origin, PortalServiceOrigin::Emote { .. }))
        {
            return Err(PortalError::Conflict);
        }
        self.reject_portal_proposal_inner(operation)
    }
    pub(in crate::kernel) fn reject_portal_proposal_inner(
        &mut self,
        operation: u64,
    ) -> Result<(), PortalError> {
        let pending = self
            .portals
            .pending
            .get(&operation)
            .ok_or(PortalError::Invalid)?;
        if pending.committed {
            return Err(PortalError::Conflict);
        }
        if pending.ticket.origin == PortalServiceOrigin::Spell
            && !self
                .magic
                .portal_finish_ready(pending.ticket.cast_actor, pending.ticket.cast)
        {
            return Err(PortalError::Capacity);
        }
        // Magic release can refuse while its bounded completion/recovery lane
        // is unavailable. Keep the exact portal and registry holds until it
        // succeeds, just as in the committed completion path.
        if pending.ticket.origin == PortalServiceOrigin::Spell {
            self.magic
                .finish_portal_service(
                    pending.ticket.cast_actor,
                    pending.ticket.cast,
                    Err(bace_gameplay_api::CastRejection::InvalidState),
                    &mut self.world,
                    self.tick as f64 / 30.0,
                )
                .map_err(|_| PortalError::Conflict)?;
        }
        let pending = self
            .portals
            .pending
            .remove(&operation)
            .expect("checked portal ticket");
        self.portals.proposals.retain(|op| *op != operation);
        self.release_portal_reservations(&pending);
        Ok(())
    }

    fn release_portal_reservations(&mut self, pending: &PendingPortalService) {
        self.characters
            .release_portal(pending.ticket.operation, &pending.ticket.participants);
        self.world
            .release_vitals(bace_world::VitalReservationToken {
                domain: bace_world::VitalReservationDomain::Portal,
                operation: pending.ticket.operation,
            });
        if let PortalServiceEffect::Teleport(moves) = &pending.ticket.effect {
            for m in moves {
                let _ = self
                    .world
                    .finish_portal_transit(m.actor, pending.ticket.operation);
            }
        }
        for &actor in &pending.reserved_registries {
            let _ = self
                .magic
                .reserve_registry(actor, false, self.tick as f64 / 30.0);
        }
    }
    pub(in crate::kernel) fn service_portals(&mut self) {
        let now = self.tick as f64 / 30.0;
        let expired: Vec<_> = self
            .portals
            .spawned
            .iter()
            .filter(|(_, until)| **until <= now)
            .map(|(&id, _)| id)
            .take(32)
            .collect();
        for entity in expired {
            if self.portals.events.len() >= self.portals.capacity {
                break;
            }
            self.world.remove_anchor(entity);
            self.portals.spawned.remove(&entity);
            self.portals.anchors.remove(&entity);
            self.portals
                .events
                .push_back(PortalServiceEvent::Removed { entity });
        }
        self.service_portal_completions();
        for request in self.magic.pending_portal_casts() {
            if let Err(error) = self.stage_portal_request(request.clone())
                && !matches!(
                    error,
                    PortalError::Capacity | PortalError::Conflict | PortalError::MissingAssets
                )
                && self.magic.portal_finish_ready(request.actor, request.cast)
            {
                let _ = self.magic.finish_portal_service(
                    request.actor,
                    request.cast,
                    Err(bace_gameplay_api::CastRejection::InvalidTarget),
                    &mut self.world,
                    now,
                );
            }
        }
        let ready: Vec<_> = self
            .portals
            .pending
            .iter()
            .filter(|(_, p)| p.committed)
            .map(|(&id, _)| id)
            .take(32)
            .collect();
        for operation in ready {
            if self.portals.events.len() + 1 > self.portals.capacity {
                break;
            }
            let pending = &self.portals.pending[&operation];
            if pending.ticket.origin == PortalServiceOrigin::Spell
                && !self
                    .magic
                    .portal_finish_ready(pending.ticket.cast_actor, pending.ticket.cast)
            {
                continue;
            }
            if pending.completed.is_none()
                && pending
                    .ticket
                    .participants
                    .iter()
                    .any(|(actor, before, _)| {
                        self.characters
                            .get(*actor)
                            .is_none_or(|c| c.revision() != *before)
                    })
            {
                continue;
            }
            let mut pending = self
                .portals
                .pending
                .remove(&operation)
                .expect("selected portal ticket");
            if let Some(aborted) = pending.completed {
                self.finish_applied_portal(pending, aborted, now);
                continue;
            }
            if !pending.mana_applied {
                if pending.ticket.mana.is_some_and(|mana| {
                    self.magic
                        .apply_portal_mana(&mut self.world, mana, operation)
                        .is_err()
                }) {
                    self.portals.pending.insert(operation, pending);
                    continue;
                }
                pending.mana_applied = true;
                if !matches!(pending.ticket.effect, PortalServiceEffect::Sanctuary { .. }) {
                    self.world
                        .release_vitals(bace_world::VitalReservationToken {
                            domain: bace_world::VitalReservationDomain::Portal,
                            operation,
                        });
                }
                if let PortalServiceEffect::Teleport(moves) = &pending.ticket.effect {
                    pending.due = Some(
                        now + if pending.ticket.origin == PortalServiceOrigin::Spell {
                            2.0
                        } else {
                            0.0
                        },
                    );
                    self.portals.events.push_back(PortalServiceEvent::Hidden {
                        operation,
                        actors: moves.iter().map(|m| m.actor).collect(),
                        views: moves
                            .iter()
                            .map(|m| self.accepted_portal_view(m.actor))
                            .collect(),
                    });
                    self.portals.pending.insert(operation, pending);
                    continue;
                }
            }
            if pending.due.is_some_and(|due| now < due) {
                self.portals.pending.insert(operation, pending);
                continue;
            }
            let mut aborted = false;
            let applied = match &pending.ticket.effect {
                PortalServiceEffect::Sanctuary {
                    link,
                    character,
                    stamina,
                } => {
                    let token = bace_world::VitalReservationToken {
                        domain: bace_world::VitalReservationDomain::Portal,
                        operation,
                    };
                    let candidate = self
                        .portals
                        .links
                        .get(&pending.ticket.actor)
                        .cloned()
                        .and_then(|mut links| links.adopt(link).ok().map(|()| links));
                    if let Some(candidate) = candidate
                        && self.characters.native_services(pending.ticket.actor)
                            == Some(&character.before)
                        && self
                            .world
                            .validate_vital_batch_reserved(&[*stamina], None, token)
                            .is_ok()
                        && self
                            .characters
                            .adopt_native_services(pending.ticket.actor, (**character).clone())
                            .is_ok()
                    {
                        self.world
                            .apply_vital_batch_reserved(&[*stamina], None, token)
                            .expect("prevalidated sanctuary stamina");
                        self.portals.links.insert(pending.ticket.actor, candidate);
                        true
                    } else {
                        false
                    }
                }
                PortalServiceEffect::Link(change) => self
                    .portals
                    .links
                    .get_mut(&pending.ticket.cast_actor)
                    .is_some_and(|links| links.adopt(change).is_ok()),
                PortalServiceEffect::Teleport(moves) => {
                    if pending.ticket.origin != PortalServiceOrigin::Death
                        && moves.iter().any(|m| {
                            self.world
                                .combatant(m.actor)
                                .is_none_or(|c| c.health() == 0)
                        })
                    {
                        aborted = true;
                        true
                    } else {
                        self.world.teleport_batch(moves).is_ok()
                    }
                }
                PortalServiceEffect::Summon {
                    entity,
                    template,
                    original_template,
                    origin,
                    destination,
                    lifetime,
                } => {
                    if let Some(actor) = pending.prepared_anchor.take() {
                        match self.world.insert_anchor(actor) {
                            Ok(()) => {
                                let mut anchor = self.portals.templates[original_template]
                                    .instantiate(entity.0, *origin)
                                    .expect("preflighted portal definition and placement");
                                anchor.template = *template;
                                anchor.original_template = Some(*original_template);
                                anchor.position = *origin;
                                anchor.destination = Some(*destination);
                                anchor.restrictions |= 0x10;
                                self.portals.anchors.insert(*entity, Arc::new(anchor));
                                self.portals.spawned.insert(*entity, now + *lifetime);
                                true
                            }
                            Err((_, actor)) => {
                                pending.prepared_anchor = Some(*actor);
                                false
                            }
                        }
                    } else {
                        false
                    }
                }
            };
            if !applied {
                if !pending.blocked {
                    self.portals.events.push_back(PortalServiceEvent::Blocked {
                        operation,
                        actor: pending.ticket.actor,
                    });
                    pending.blocked = true;
                }
                self.portals.pending.insert(operation, pending);
                continue;
            }
            self.finish_applied_portal(pending, aborted, now);
        }
    }

    fn finish_applied_portal(
        &mut self,
        mut pending: PendingPortalService,
        aborted: bool,
        now: f64,
    ) {
        let operation = pending.ticket.operation;
        // Keep registries held until magic accepts the exact terminal result.
        // Unreserving a clock can expose timer debt and exhaust magic capacity.
        // A delayed acknowledgment must never reapply the teleport/link/effect.
        if pending.ticket.origin == PortalServiceOrigin::Spell {
            let outcome = if aborted {
                Err(bace_gameplay_api::CastRejection::Dead)
            } else {
                Ok(())
            };
            if self
                .magic
                .finish_portal_service(
                    pending.ticket.cast_actor,
                    pending.ticket.cast,
                    outcome,
                    &mut self.world,
                    now,
                )
                .is_err()
            {
                pending.completed = Some(aborted);
                self.portals.pending.insert(operation, pending);
                return;
            }
        }
        self.characters
            .release_portal(operation, &pending.ticket.participants);
        let success_teleport =
            !aborted && matches!(pending.ticket.effect, PortalServiceEffect::Teleport(_));
        if success_teleport {
            if let PortalServiceEffect::Teleport(moves) = &pending.ticket.effect {
                for movement in moves {
                    self.clear_death_protection_for_teleport(movement.actor);
                    if let Ok((_, state)) = self.world.actor_state(movement.actor) {
                        self.portals.awaiting.insert(
                            movement.actor,
                            PortalCompletion {
                                operation,
                                epoch: state.epoch(),
                                client_ready: self
                                    .world
                                    .combatant(movement.actor)
                                    .is_some_and(|c| !c.profile().player),
                                destination_ready: self
                                    .world
                                    .combatant(movement.actor)
                                    .is_some_and(|c| !c.profile().player),
                            },
                        );
                    }
                }
            }
            for &actor in &pending.reserved_registries {
                let _ = self.magic.reserve_registry(actor, false, now);
            }
        } else {
            self.release_portal_reservations(&pending);
        }
        for &(actor, _, _) in &pending.ticket.participants {
            // All expected aggregate revisions were checked on this owner.
            if !matches!(&pending.ticket.effect,PortalServiceEffect::Sanctuary{character,..} if character.before!=character.after)
            {
                let _ = self.characters.touch_auxiliary(actor);
            }
            if aborted {
                let _ = self.characters.touch_auxiliary(actor);
            }
        }
        let event = if aborted {
            PortalServiceEvent::AbortedAfterCommit {
                operation,
                actor: pending.ticket.actor,
            }
        } else {
            match &pending.ticket.effect {
                PortalServiceEffect::Link(_) | PortalServiceEffect::Sanctuary { .. } => {
                    PortalServiceEvent::Linked {
                        operation,
                        actor: pending.ticket.actor,
                    }
                }
                PortalServiceEffect::Teleport(moves) => PortalServiceEvent::Teleported {
                    operation,
                    actors: moves.iter().map(|m| m.actor).collect(),
                    views: moves
                        .iter()
                        .map(|m| self.accepted_portal_view(m.actor))
                        .collect(),
                },
                PortalServiceEffect::Summon {
                    entity, template, ..
                } => PortalServiceEvent::Summoned {
                    operation,
                    entity: *entity,
                    template: *template,
                },
            }
        };
        self.portals.events.push_back(event);
    }
    pub fn acknowledge_portal_ready(
        &mut self,
        context: bace_gameplay_api::ActionContext,
        operation: u64,
        epoch: u16,
    ) -> Result<(), PortalError> {
        if self.portals.events.len() >= self.portals.capacity {
            return Err(PortalError::Capacity);
        }
        if operation == 0 {
            if self.portals.awaiting.contains_key(&context.actor) {
                return Err(PortalError::Conflict);
            }
            self.characters
                .authorize(context, self.world.body(context.actor).is_ok())
                .map_err(|_| PortalError::Conflict)?;
            self.portals
                .events
                .push_back(PortalServiceEvent::Materialized {
                    operation,
                    actor: context.actor,
                    view: self.accepted_portal_view(context.actor),
                });
            return Ok(());
        }
        if self
            .portals
            .awaiting
            .get(&context.actor)
            .is_none_or(|p| p.operation != operation || p.epoch != epoch)
        {
            return Err(PortalError::Conflict);
        }
        self.acknowledge_portal_login(context)
    }
    pub fn acknowledge_portal_login(
        &mut self,
        context: bace_gameplay_api::ActionContext,
    ) -> Result<(), PortalError> {
        if !self.portals.awaiting.contains_key(&context.actor) {
            return Err(PortalError::Invalid);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| PortalError::Conflict)?;
        self.portals
            .awaiting
            .get_mut(&context.actor)
            .expect("checked transition")
            .client_ready = true;
        self.service_portal_completions();
        Ok(())
    }
    pub fn mark_portal_destination_ready(
        &mut self,
        actor: EntityId,
        operation: u64,
        epoch: u16,
    ) -> Result<(), PortalError> {
        let pending = self
            .portals
            .awaiting
            .get_mut(&actor)
            .ok_or(PortalError::Invalid)?;
        if pending.operation != operation
            || pending.epoch != epoch
            || !self
                .world
                .actor_state(actor)
                .is_ok_and(|(_, state)| state.epoch() == epoch)
        {
            return Err(PortalError::Conflict);
        }
        pending.destination_ready = true;
        self.service_portal_completions();
        Ok(())
    }
    fn service_portal_completions(&mut self) {
        let ready: Vec<_> = self
            .portals
            .awaiting
            .iter()
            .filter(|(actor, p)| {
                p.client_ready && p.destination_ready && !self.player_deaths.reserved(**actor)
            })
            .map(|(&actor, p)| (actor, p.operation, p.epoch))
            .take(32)
            .collect();
        for (actor, operation, epoch) in ready {
            if self.portals.events.len() >= self.portals.capacity {
                break;
            }
            if !self
                .world
                .actor_state(actor)
                .is_ok_and(|(_, state)| state.epoch() == epoch)
            {
                continue;
            }
            if self.world.finish_portal_transit(actor, operation).is_ok() {
                self.portals.awaiting.remove(&actor);
                self.portals
                    .events
                    .push_back(PortalServiceEvent::Materialized {
                        operation,
                        actor,
                        view: self.accepted_portal_view(actor),
                    });
            }
        }
    }
    /// Lifecycle ownership transfer, never a client portal-completion shortcut.
    pub fn take_portal_links(
        &mut self,
        actor: EntityId,
    ) -> Result<Option<PortalLinks>, PortalError> {
        if self.portals.reserved(actor) {
            return Err(PortalError::Conflict);
        }
        if let Some(completion) = self.portals.awaiting.remove(&actor) {
            let _ = self
                .world
                .finish_portal_transit(actor, completion.operation);
        }
        self.portals.access.remove(&actor);
        Ok(self.portals.links.remove(&actor))
    }
}
