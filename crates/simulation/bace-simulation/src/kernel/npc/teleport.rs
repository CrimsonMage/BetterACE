//! TeleportTarget uses the portal owner and an atomic final-destination plus
//! source-checkpoint save. Only World applies the admitted destination.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, PortalServiceReceipt, npc::NpcPortalTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
impl Kernel {
    pub fn prepare_npc_teleport(&mut self, expected: &NpcProposal) -> Result<NpcPortalTicket, E> {
        self.npcs.validate_service(expected)?;
        if self
            .npcs
            .portal_services
            .values()
            .any(|p| p.npc.ticket == expected.ticket)
        {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::TeleportTarget(destination)) = expected.effect else {
            return Err(E::Unsupported);
        };
        if destination.relative {
            return Err(E::InvalidInput);
        }
        let actor = expected.context.target.ok_or(E::MissingActor)?;
        let portal = self
            .stage_emote_teleport(
                actor,
                bace_interactions::PortalPosition {
                    cell: destination.cell.ok_or(E::InvalidInput)?.0,
                    origin: [
                        destination.position.x,
                        destination.position.y,
                        destination.position.z,
                    ],
                    rotation: destination.rotation,
                },
                expected.ticket,
            )
            .map_err(|e| match e {
                bace_interactions::PortalError::MissingAssets => E::MissingContent,
                bace_interactions::PortalError::Capacity => E::Capacity,
                _ => E::Conflict,
            })?;
        let ticket = NpcPortalTicket {
            npc: expected.clone(),
            portal,
        };
        self.npcs
            .portal_services
            .insert(ticket.portal.operation, ticket.clone());
        Ok(ticket)
    }
    /// Source Teleport() starts its independent portal lifecycle here. This is
    /// durable command adoption; PortalServiceEvent owns actual visibility and
    /// completed physical movement notification.
    pub fn confirm_npc_teleport_committed(
        &mut self,
        ticket: &NpcPortalTicket,
        receipt: &PortalServiceReceipt,
    ) -> Result<(), E> {
        if self.npcs.portal_services.get(&receipt.operation) != Some(ticket) {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(&ticket.npc)?;
        self.confirm_portal_committed_inner(receipt)
            .map_err(|_| E::Conflict)?;
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.portal_services.remove(&receipt.operation);
        self.confirm_npc_committed(&ticket.npc)
    }
    pub fn reject_npc_teleport(&mut self, ticket: &NpcPortalTicket) -> Result<(), E> {
        if self.npcs.portal_services.get(&ticket.portal.operation) != Some(ticket) {
            return Err(E::Conflict);
        }
        self.reject_portal_proposal_inner(ticket.portal.operation)
            .map_err(|_| E::Conflict)?;
        self.npcs.portal_services.remove(&ticket.portal.operation);
        Ok(())
    }
}
