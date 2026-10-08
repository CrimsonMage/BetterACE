use super::*;
impl Kernel {
    fn portal_plan(
        &self,
        request: &crate::magic::portals::PendingPortalCast,
    ) -> Result<(PortalServiceEffect, Option<bace_entity::Actor>), PortalError> {
        let source = request.actor;
        let target = request.target;
        match &request.effect {
            PortalEffect::Link { slot } => {
                let access = self.live_portal_access(source)?;
                let links = self.portals.links.get(&source).ok_or(PortalError::NoLink)?;
                let anchor = self
                    .portals
                    .anchors
                    .get(&target)
                    .ok_or(PortalError::WrongKind)?;
                Ok((
                    PortalServiceEffect::Link(links.propose_link(*slot, anchor, access)?),
                    None,
                ))
            }
            PortalEffect::Recall { slot } => {
                if self
                    .world
                    .combatant(source)
                    .is_some_and(|c| c.profile().player)
                    && self.live_portal_access(source)?.pk_recent
                {
                    return Err(PortalError::PkRecent);
                }
                let links = self.portals.links.get(&target).ok_or(PortalError::NoLink)?;
                let destination = if (3..=5).contains(slot) {
                    let did = match slot {
                        3 => 47,
                        4 => 31,
                        _ => 48,
                    };
                    let template = links
                        .template(did)
                        .filter(|id| *id != 0)
                        .ok_or(PortalError::NoLink)?;
                    let portal = self
                        .portals
                        .templates
                        .get(&template)
                        .ok_or(PortalError::MissingAssets)?;
                    if portal.restrictions & 0x20 != 0 {
                        return Err(PortalError::NoRecall);
                    }
                    portal.check(self.live_portal_access(target)?)?;
                    portal.destination.ok_or(PortalError::NoLink)?
                } else {
                    links.recall(*slot)?
                };
                Ok((
                    PortalServiceEffect::Teleport(vec![self.portal_teleport(target, destination)?]),
                    None,
                ))
            }
            PortalEffect::Sending {
                cell,
                position,
                heading,
            } => {
                let (sin, cos) = (*heading * 0.5).sin_cos();
                let destination = PortalPosition {
                    cell: *cell,
                    origin: [position.x, position.y, position.z],
                    rotation: [cos, 0., 0., sin],
                };
                destination.validate()?;
                let recipients = if request.fellowship {
                    self.fellowships
                        .roster(target)
                        .ok_or(PortalError::NoLink)?
                        .to_vec()
                } else {
                    vec![target]
                };
                if recipients.len() > 9 || recipients.is_empty() {
                    return Err(PortalError::Capacity);
                }
                let target_block = self
                    .world
                    .actor_state(target)
                    .map_err(|_| PortalError::Invalid)?
                    .0
                    .0
                    >> 16;
                let mut moves = Vec::new();
                for actor in recipients {
                    if self
                        .world
                        .actor_state(actor)
                        .is_ok_and(|(cell, _)| cell.0 >> 16 == target_block)
                    {
                        moves.push(self.portal_teleport(actor, destination)?);
                    }
                }
                if moves.is_empty() {
                    return Err(PortalError::Invalid);
                }
                Ok((PortalServiceEffect::Teleport(moves), None))
            }
            PortalEffect::Summon {
                slot,
                template,
                lifetime,
            } => {
                let access = self.live_portal_access(source)?;
                if access.pk_recent {
                    return Err(PortalError::PkRecent);
                }
                if access.olthoi {
                    return Err(PortalError::OlthoiRestricted);
                }
                if !lifetime.is_finite() || !(0.0..=86400.0).contains(lifetime) || *lifetime == 0.0
                {
                    return Err(PortalError::Invalid);
                }
                let links = self.portals.links.get(&source).ok_or(PortalError::NoLink)?;
                let did = if *slot <= 1 { 31 } else { 48 };
                if links.tied_summoned(did) {
                    return Err(PortalError::NoSummon);
                }
                let original_template = links
                    .template(did)
                    .filter(|id| *id != 0)
                    .ok_or(PortalError::NoLink)?;
                let portal = self
                    .portals
                    .templates
                    .get(&original_template)
                    .ok_or(PortalError::MissingAssets)?;
                if portal.restrictions & 0x10 != 0 {
                    return Err(PortalError::NoSummon);
                }
                portal.check(access)?;
                let destination = portal.destination.ok_or(PortalError::NoLink)?;
                let entity = *self.portals.ids.front().ok_or(PortalError::Capacity)?;
                let shape = self
                    .portals
                    .shapes
                    .get(template)
                    .ok_or(PortalError::MissingAssets)?
                    .clone();
                let (cell, state) = self
                    .world
                    .actor_state(source)
                    .map_err(|_| PortalError::Invalid)?;
                let (sin, cos) = state.heading_radians().sin_cos();
                let at = state.position() + bace_geometry::Vec3::new(-3.0 * sin, 3.0 * cos, 0.0);
                let (sin, cos) = (state.heading_radians() * 0.5).sin_cos();
                let origin = PortalPosition {
                    cell: cell.0,
                    origin: [at.x, at.y, at.z],
                    rotation: [cos, 0., 0., sin],
                };
                let body = self
                    .world
                    .prepare_geometry_body(bace_physics::GeometrySpawn {
                        cell: cell.0,
                        position: at,
                        shape,
                        capabilities: bace_motion::Capabilities {
                            speed: 0.0,
                            jump_impulse: 0.0,
                        },
                        heading: state.heading_radians(),
                        maximum_turn_rate: 0.0,
                    })
                    .map_err(|_| PortalError::MissingAssets)?;
                Ok((
                    PortalServiceEffect::Summon {
                        entity,
                        template: *template,
                        original_template,
                        origin,
                        destination,
                        lifetime: *lifetime,
                    },
                    Some(bace_entity::Actor {
                        id: entity,
                        cell,
                        body,
                    }),
                ))
            }
        }
    }
    pub(in crate::kernel) fn portal_teleport(
        &self,
        actor: EntityId,
        position: PortalPosition,
    ) -> Result<bace_world::WorldTeleport, PortalError> {
        if self.world.combatant(actor).is_none_or(|c| c.health() == 0) {
            return Err(PortalError::Invalid);
        }
        self.portal_death_teleport(actor, position)
    }
    pub(in crate::kernel) fn portal_death_teleport(
        &self,
        actor: EntityId,
        position: PortalPosition,
    ) -> Result<bace_world::WorldTeleport, PortalError> {
        position.validate()?;
        let (_, state) = self
            .world
            .actor_state(actor)
            .map_err(|_| PortalError::Invalid)?;
        let scale = match self
            .world
            .properties(actor)
            .and_then(|p| p.get(bace_entity::PropertyFamily::Float, 39))
        {
            Some(bace_entity::PropertyValue::Float(value)) => *value as f32,
            _ => 1.0,
        };
        if !scale.is_finite() || scale <= 0.0 {
            return Err(PortalError::Invalid);
        }
        let adjust = if self
            .world
            .combatant(actor)
            .is_some_and(|c| c.profile().player)
        {
            0.005 * scale
        } else {
            0.0
        };
        Ok(bace_world::WorldTeleport {
            actor,
            expected_epoch: state.epoch(),
            destination: CellId(position.cell),
            position: bace_geometry::Vec3::new(
                position.origin[0],
                position.origin[1],
                position.origin[2] + adjust,
            ),
            heading: position.heading(),
        })
    }
    pub(in crate::kernel) fn stage_portal_request(
        &mut self,
        request: crate::magic::portals::PendingPortalCast,
    ) -> Result<(), PortalError> {
        if matches!(request.effect, PortalEffect::Recall { .. })
            && !self.recall_policy.spell_recalls
        {
            return Err(PortalError::NoRecall);
        }
        let (effect, prepared_anchor) = self.portal_plan(&request)?;
        self.stage_owned_portal_request(
            request.actor,
            request.epoch,
            request.cast,
            PortalServiceOrigin::Spell,
            request.mana_cost,
            effect,
            prepared_anchor,
        )
    }
    #[expect(clippy::too_many_arguments)]
    pub(in crate::kernel) fn stage_owned_portal_request(
        &mut self,
        actor: EntityId,
        epoch: u16,
        cast: u64,
        origin: PortalServiceOrigin,
        mana_cost: u32,
        effect: PortalServiceEffect,
        prepared_anchor: Option<bace_entity::Actor>,
    ) -> Result<(), PortalError> {
        if self.world.is_in_portal_transit(actor) {
            return Err(PortalError::Teleporting);
        }
        if !self
            .world
            .actor_state(actor)
            .is_ok_and(|(_, state)| state.epoch() == epoch)
        {
            return Err(PortalError::Invalid);
        }
        if self.portals.pending.len() >= self.portals.capacity {
            return Err(PortalError::Capacity);
        }
        let npc_ticket = match origin {
            PortalServiceOrigin::Emote { ticket } => Some(ticket),
            PortalServiceOrigin::Spell => match self.magic.portal_origin(actor, cast) {
                Some(bace_gameplay_api::CastOrigin::Emote { event, .. }) => Some(event),
                _ => None,
            },
            _ => None,
        };
        if self.portals.reserved(actor)
            || self.characters.reserved(actor)
            || npc_ticket.map_or_else(
                || self.npcs.reserved(actor),
                |ticket| self.npcs.reserved_except(actor, ticket),
            )
            || self.inventory.reserved(actor)
            || self.housing.reserved(actor)
        {
            return Err(PortalError::Conflict);
        }
        self.prepare_inventory_time()
            .map_err(|_| PortalError::Conflict)?;
        let operation = self
            .portals
            .next
            .checked_add(1)
            .ok_or(PortalError::Capacity)?;
        let mut ids = vec![actor];
        if let PortalServiceEffect::Teleport(moves) = &effect {
            ids.extend(moves.iter().map(|m| m.actor));
        }
        ids.sort_unstable();
        ids.dedup();
        if ids.iter().any(|actor| {
            self.portals.reserved(*actor)
                || self.characters.reserved(*actor)
                || npc_ticket.map_or_else(
                    || self.npcs.reserved(*actor),
                    |ticket| self.npcs.reserved_except(*actor, ticket),
                )
                || self.inventory.reserved(*actor)
                || self.housing.reserved(*actor)
        }) {
            return Err(PortalError::Conflict);
        }
        let mut participants = Vec::new();
        for &actor in &ids {
            if let Some(character) = self.characters.get(actor) {
                let before = character.revision();
                participants.push((
                    actor,
                    before,
                    before.checked_add(1).ok_or(PortalError::Capacity)?,
                ));
            }
        }
        let &(player_actor, before_revision, after_revision) = participants
            .iter()
            .find(|p| p.0 == actor)
            .or_else(|| participants.first())
            .ok_or(PortalError::Invalid)?;
        self.characters
            .can_reserve_portal(operation, &participants)
            .map_err(|_| PortalError::Conflict)?;
        let mana = if mana_cost == 0
            && self.characters.get(actor).is_none()
            && origin == PortalServiceOrigin::Spell
        {
            None
        } else {
            let mana = self
                .world
                .vital(actor, EntityVital::Mana)
                .map_err(|_| PortalError::Invalid)?;
            Some(VitalMutation {
                actor,
                vital: EntityVital::Mana,
                before: mana.current,
                after: mana
                    .current
                    .checked_sub(mana_cost)
                    .ok_or(PortalError::Invalid)?,
            })
        };
        if let Some(mana) = mana {
            self.world
                .validate_vital_batch(&[mana], None)
                .map_err(|_| PortalError::Invalid)?;
        }
        if let PortalServiceEffect::Sanctuary { stamina, .. } = &effect {
            self.world
                .validate_vital_batch(&[*stamina], None)
                .map_err(|_| PortalError::Conflict)?;
        }
        let now = self.tick as f64 / 30.0;
        let mut reserved = Vec::new();
        for id in ids {
            if self.magic.registry(id).is_some() {
                if self.magic.reserve_registry(id, true, now).is_err() {
                    for old in reserved {
                        let _ = self.magic.reserve_registry(old, false, now);
                    }
                    return Err(PortalError::Conflict);
                }
                reserved.push(id);
            }
        }
        let mana_token = bace_world::VitalReservationToken {
            domain: bace_world::VitalReservationDomain::Portal,
            operation,
        };
        let mut resources: Vec<_> = mana.iter().map(|m| (m.actor, m.vital)).collect();
        if let PortalServiceEffect::Sanctuary { .. } = &effect {
            resources.push((actor, EntityVital::Stamina));
        }
        if !resources.is_empty() && self.world.reserve_vitals(&resources, mana_token).is_err() {
            for old in reserved {
                let _ = self.magic.reserve_registry(old, false, now);
            }
            return Err(PortalError::Conflict);
        }
        if let PortalServiceEffect::Teleport(moves) = &effect {
            let actors: Vec<_> = moves.iter().map(|m| (m.actor, m.expected_epoch)).collect();
            if self.world.begin_portal_transit(&actors, operation).is_err()
                || self.world.validate_teleport_batch(moves).is_err()
            {
                for &(id, _) in &actors {
                    let _ = self.world.finish_portal_transit(id, operation);
                }
                for old in reserved {
                    let _ = self.magic.reserve_registry(old, false, now);
                }
                self.world.release_vitals(mana_token);
                return Err(PortalError::MissingAssets);
            }
        }
        if origin == PortalServiceOrigin::Spell
            && self.magic.bind_portal(actor, cast, operation).is_err()
        {
            if let PortalServiceEffect::Teleport(moves) = &effect {
                for m in moves {
                    let _ = self.world.finish_portal_transit(m.actor, operation);
                }
            }
            for old in reserved {
                let _ = self.magic.reserve_registry(old, false, now);
            }
            self.world.release_vitals(mana_token);
            return Err(PortalError::Conflict);
        }
        if matches!(effect, PortalServiceEffect::Summon { .. }) {
            self.portals.ids.pop_front();
        }
        let ticket = PortalServiceTicket {
            origin,
            operation,
            cast,
            actor: player_actor,
            cast_actor: actor,
            before_revision,
            after_revision,
            participants,
            mana,
            effect,
        };
        self.characters
            .reserve_portal(operation, &ticket.participants);
        self.portals.next = operation;
        self.portals.pending.insert(
            operation,
            PendingPortalService {
                ticket,
                committed: false,
                due: None,
                prepared_anchor,
                blocked: false,
                reserved_registries: reserved,
                mana_applied: false,
                completed: None,
            },
        );
        self.portals.proposals.push_back(operation);
        Ok(())
    }
}
