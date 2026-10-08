//! Trusted staff ingress still rechecks the live binding and source access role.
use super::*;
use bace_gameplay_api::staff::{StaffError as E, StaffEvent, StaffRunMode};
impl Kernel {
    pub fn staff_cast_spell(
        &mut self,
        context: ActionContext,
        target: Option<EntityId>,
        spell: u32,
        event: u64,
        sudo: bool,
    ) -> Result<(), E> {
        let definition = self.staff.spells.get(&spell).ok_or(E::Invalid)?;
        let target = if definition.targeted {
            Some(target.ok_or(E::MissingTarget)?)
        } else {
            None
        };
        self.authorize_staff(context, 4, sudo)?;
        if let Some(target) = target {
            if self.characters.reserved(target)
                || self.inventory.reserved(target)
                || self.housing.reserved(target)
                || self.npcs.reserved(target)
            {
                return Err(E::Busy);
            }
            // Last-appraised resolution is local to the accepted landblock in ACE.
            let a = self
                .world
                .actor_state(context.actor)
                .map_err(|_| E::MissingTarget)?
                .0;
            let t = self
                .world
                .actor_state(target)
                .map_err(|_| E::MissingTarget)?
                .0;
            if a.0 >> 16 != t.0 >> 16 {
                return Err(E::MissingTarget);
            }
        }
        let request = target.map_or(
            bace_gameplay_api::CastRequest::Untargeted { spell },
            |target| bace_gameplay_api::CastRequest::Targeted { target, spell },
        );
        self.cast_from_server(
            bace_gameplay_api::CastOrigin::Staff {
                actor: context.actor,
                event,
            },
            request,
        )
        .map_err(|error| match error {
            bace_gameplay_api::CastRejection::Capacity => E::Capacity,
            bace_gameplay_api::CastRejection::Busy => E::Busy,
            _ => E::Invalid,
        })?;
        Ok(())
    }
    pub fn staff_run(
        &mut self,
        context: ActionContext,
        mode: StaffRunMode,
        sudo: bool,
    ) -> Result<(), E> {
        self.authorize_staff(context, 2, sudo)?;
        if self
            .characters
            .get(context.actor)
            .is_none_or(|p| p.revision() == u64::MAX)
        {
            return Err(E::Overflow);
        }
        let text = self
            .magic
            .staff_run(
                context.actor,
                mode,
                self.tick as f64 / 30.,
                self.staff.effects.get(&1644),
            )
            .map_err(|error| match error {
                bace_gameplay_api::CastRejection::Capacity => E::Capacity,
                bace_gameplay_api::CastRejection::Busy => E::Busy,
                _ => E::Invalid,
            })?;
        self.sync_registry_revisions().map_err(|_| E::Overflow)?;
        self.staff.push(StaffEvent::Inspection {
            context,
            target: context.actor,
            lines: if text.is_empty() { vec![] } else { vec![text] },
        });
        Ok(())
    }
}
