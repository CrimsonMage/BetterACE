//! Nonspell recalls use the same durable portal owner and receipt format.
use super::*;
impl Kernel {
    pub(in crate::kernel) fn stage_emote_teleport(
        &mut self,
        actor: EntityId,
        destination: PortalPosition,
        npc_ticket: u64,
    ) -> Result<PortalServiceTicket, PortalError> {
        let epoch = self
            .world
            .actor_state(actor)
            .map_err(|_| PortalError::Invalid)?
            .1
            .epoch();
        let movement = self.portal_teleport(actor, destination)?;
        self.stage_owned_portal_request(
            actor,
            epoch,
            0,
            PortalServiceOrigin::Emote { ticket: npc_ticket },
            0,
            PortalServiceEffect::Teleport(vec![movement]),
            None,
        )?;
        let operation = self.portals.next;
        self.portals.proposals.retain(|op| *op != operation);
        Ok(self
            .portals
            .pending
            .get(&operation)
            .ok_or(PortalError::Invalid)?
            .ticket
            .clone())
    }
    pub(in crate::kernel) fn stage_lifestone_binding(
        &mut self,
        actor: EntityId,
        destination: PortalPosition,
        stamina_before: u32,
        stamina_after: u32,
    ) -> Result<u64, bace_interactions::RecallError> {
        use bace_interactions::RecallError as E;
        self.prepare_inventory_time().map_err(|_| E::Busy)?;
        let links = self.portal_links(actor).ok_or(E::MissingAssets)?;
        let link = PortalLinkMutation {
            before_revision: links.revision(),
            after_revision: links.revision().checked_add(1).ok_or(E::Capacity)?,
            position_slot: 4,
            before: links.position(4),
            after: destination,
            data_id: None,
            tied_summoned: false,
        };
        let revision = self.characters.get(actor).ok_or(E::Invalid)?.revision();
        let character = self
            .characters
            .native_services(actor)
            .ok_or(E::MissingAssets)?
            .propose_sanctuary(
                revision,
                bace_gameplay_api::NpcDestination {
                    cell: Some(CellId(destination.cell)),
                    position: Vec3::new(
                        destination.origin[0],
                        destination.origin[1],
                        destination.origin[2],
                    ),
                    rotation: destination.rotation,
                    relative: false,
                },
            )
            .map_err(|_| E::Invalid)?;
        let stamina = VitalMutation {
            actor,
            vital: EntityVital::Stamina,
            before: stamina_before,
            after: stamina_after,
        };
        let epoch = self
            .world
            .actor_state(actor)
            .map_err(|_| E::Invalid)?
            .1
            .epoch();
        self.stage_owned_portal_request(
            actor,
            epoch,
            0,
            PortalServiceOrigin::Binding,
            0,
            PortalServiceEffect::Sanctuary {
                link,
                character: Box::new(character),
                stamina,
            },
            None,
        )
        .map_err(|error| match error {
            PortalError::Capacity => E::Capacity,
            PortalError::Conflict => E::Busy,
            _ => E::Invalid,
        })?;
        Ok(self.portals.next)
    }
    pub(in crate::kernel) fn stage_command_recall(
        &mut self,
        actor: EntityId,
        kind: bace_interactions::RecallKind,
        destination: PortalPosition,
    ) -> Result<u64, PortalError> {
        let epoch = self
            .world
            .actor_state(actor)
            .map_err(|_| PortalError::Invalid)?
            .1
            .epoch();
        let movement = self.portal_teleport(actor, destination)?;
        self.stage_owned_portal_request(
            actor,
            epoch,
            0,
            PortalServiceOrigin::Recall(kind),
            0,
            PortalServiceEffect::Teleport(vec![movement]),
            None,
        )?;
        Ok(self.portals.next)
    }
}
impl Kernel {
    pub(in crate::kernel) fn can_admit_death_portal(&self, actor: EntityId) -> bool {
        self.portals.awaiting.len() < 4096
            && !self.portals.awaiting.contains_key(&actor)
            && self.portals.events.len() + 2 <= self.portals.capacity
    }
    pub(in crate::kernel) fn admit_death_portal(&mut self, actor: EntityId, operation: u64) {
        let epoch = self
            .world
            .actor_state(actor)
            .expect("death teleport adopted")
            .1
            .epoch();
        self.portals.awaiting.insert(
            actor,
            PortalCompletion {
                operation,
                epoch,
                client_ready: false,
                destination_ready: false,
            },
        );
        self.portals.events.push_back(PortalServiceEvent::Hidden {
            operation,
            actors: vec![actor],
            views: vec![self.accepted_portal_view(actor)],
        });
        self.portals
            .events
            .push_back(PortalServiceEvent::Teleported {
                operation,
                actors: vec![actor],
                views: vec![self.accepted_portal_view(actor)],
            });
    }
}
