//! Trusted cold admission checks the live binding and captured aggregate revision.
use super::*;
impl Kernel {
    pub fn register_actor_magic_program(
        &mut self,
        program: crate::PreparedActorMagicProgram,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        use bace_gameplay_api::CastRejection as E;
        self.characters
            .can_take_complete(program.binding)
            .map_err(|_| E::InvalidState)?;
        let actor = program.binding.actor;
        if self
            .characters
            .get(actor)
            .is_none_or(|p| p.revision() != program.expected_character_revision)
        {
            return Err(E::InvalidState);
        }
        if self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
            || self.portals.reserved(actor)
        {
            return Err(E::Busy);
        }
        if program.portal_templates.len() > 2 || program.destination_cells.len() > 3 {
            return Err(E::Capacity);
        }
        let mut seen = std::collections::BTreeSet::new();
        for (definition, _) in &program.portal_templates {
            definition.validate().map_err(|_| E::InvalidState)?;
            if !seen.insert(definition.template) {
                return Err(E::InvalidState);
            }
        }
        self.preflight_portal_definitions(&seen)
            .map_err(|_| E::Capacity)?;
        self.magic
            .register_actor_program(actor, program.definition, program.projectile_shapes)?;
        for (definition, shape) in program.portal_templates {
            self.register_portal_definition(definition, shape)
                .expect("complete portal definition preflight");
        }
        Ok(())
    }
    pub(crate) fn apply_actor_magic_program(
        &mut self,
        correlation: u64,
        program: crate::PreparedActorMagicProgram,
    ) -> Result<(), Box<crate::PreparedActorMagicProgram>> {
        if !self.staff.room(1) {
            return Err(Box::new(program));
        }
        let actor = program.binding.actor;
        let result = if correlation == 0 {
            Err(bace_gameplay_api::staff::StaffError::Invalid)
        } else if self
            .characters
            .get(actor)
            .is_some_and(|p| p.revision() != program.expected_character_revision)
        {
            Err(bace_gameplay_api::staff::StaffError::Stale)
        } else {
            self.register_actor_magic_program(program)
                .map_err(|e| match e {
                    bace_gameplay_api::CastRejection::Busy => {
                        bace_gameplay_api::staff::StaffError::Busy
                    }
                    bace_gameplay_api::CastRejection::Capacity => {
                        bace_gameplay_api::staff::StaffError::Capacity
                    }
                    _ => bace_gameplay_api::staff::StaffError::Invalid,
                })
        };
        self.staff
            .push(bace_gameplay_api::staff::StaffEvent::Outcome {
                token: correlation,
                actor: Some(actor),
                result,
            });
        Ok(())
    }
}
