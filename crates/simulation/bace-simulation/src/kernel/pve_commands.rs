use super::*;
impl Kernel {
    fn confirm_prepared_pve_committed(
        &mut self,
        operation: u64,
        corpse: Option<EntityId>,
        items: &[EntityId],
        credits: &[(EntityId, bace_character::ExperienceCredit)],
        social: Option<&crate::AllegianceTicket>,
    ) -> Result<(), PveError> {
        if let Some(ticket) = social {
            if self.allegiances.pve_operation != Some(operation)
                || self.population.shared_death_ticket(operation) != Some(ticket)
            {
                return Err(PveError::InvalidReceipt);
            }
            self.validate_allegiance_commit(ticket)
                .map_err(|_| PveError::InvalidReceipt)?;
        } else if self.population.shared_death_ticket(operation).is_some() {
            return Err(PveError::InvalidReceipt);
        }
        let staged = self
            .population
            .staged_death(operation)
            .ok_or(PveError::InvalidReceipt)?;
        let proposal = self
            .population
            .submitted_death(operation)
            .ok_or(PveError::UnknownOperation)?;
        let block = (proposal
            .position
            .as_ref()
            .ok_or(PveError::InvalidReceipt)?
            .obj_cell_id
            >> 16) as u16;
        self.validate_world_region_items(block, &staged.forest)
            .map_err(|_| PveError::InvalidReceipt)?;
        let graph = self
            .inventory
            .prepare_region_admission(
                &staged.forest.items,
                &staged.forest.containers,
                &Default::default(),
            )
            .map_err(|_| PveError::InvalidReceipt)?;
        let mut forest = self.population.committed_prepared(
            operation,
            corpse,
            items,
            credits,
            &mut self.world,
            self.tick,
        )?;
        self.inventory.adopt_region_admission(graph);
        self.adopt_world_region_items(&mut forest);
        if let Some(ticket) = social {
            self.adopt_allegiance_commit(ticket)
                .expect("prevalidated shared death commit");
            self.allegiances.pve_operation = None;
        } else {
            self.characters
                .commit_rewards(operation)
                .expect("reserved native death reward commit");
        }
        Ok(())
    }
    fn stage_pve_death(
        &mut self,
        operation: u64,
        forest: PreparedWorldRegionItems,
    ) -> Result<(), PveError> {
        let proposal = self
            .population
            .submitted_death(operation)
            .ok_or(PveError::UnknownOperation)?;
        let position = proposal.position.as_ref().ok_or(PveError::InvalidReceipt)?;
        if self.population.staged_death(operation).is_some()
            || !forest.constructed.is_empty()
            || forest.items.len() != proposal.drops.len() + usize::from(!proposal.no_corpse)
            || forest.roots.len() > proposal.drops.len() + usize::from(!proposal.no_corpse)
            || forest.roots.len() > 257
            || forest.roots.iter().any(|root| {
                root.location.cell != position.obj_cell_id
                    || root.location.origin
                        != [
                            position.position_x,
                            position.position_y,
                            position.position_z,
                        ]
                    || root.location.rotation
                        != [
                            position.rotation_x,
                            position.rotation_y,
                            position.rotation_z,
                            position.rotation_w,
                        ]
            })
        {
            return Err(PveError::InvalidReceipt);
        }
        if proposal.no_corpse {
            if forest.roots.iter().any(|root| root.corpse.is_some()) {
                return Err(PveError::InvalidReceipt);
            }
        } else if forest.roots.len() != 1
            || forest.roots[0].corpse.as_ref().is_none_or(|corpse| {
                corpse.state.operation != operation
                    || corpse.state.source != proposal.victim
                    || corpse.state.template != proposal.corpse_template
                    || corpse.state.owner != proposal.owner
            })
        {
            return Err(PveError::InvalidReceipt);
        }
        self.validate_world_region_items((position.obj_cell_id >> 16) as u16, &forest)
            .map_err(|_| PveError::InvalidReceipt)?;
        self.inventory
            .prepare_region_admission(&forest.items, &forest.containers, &Default::default())
            .map_err(|_| PveError::InvalidReceipt)?;
        let geometry = self.world.geometry().ok_or(PveError::MissingGeometry)?;
        let mut roots = Vec::with_capacity(forest.roots.len());
        for root in &forest.roots {
            let q = root.location.rotation;
            let norm = q.iter().map(|v| v * v).sum::<f32>();
            if !norm.is_finite()
                || !(0.999..=1.001).contains(&norm)
                || q[0].abs() > 0.0002
                || q[1].abs() > 0.0002
            {
                return Err(PveError::InvalidReceipt);
            }
            let body = bace_physics::Body::spawn_geometry(
                geometry,
                bace_physics::GeometrySpawn {
                    cell: root.location.cell,
                    position: bace_geometry::Vec3::new(
                        root.location.origin[0],
                        root.location.origin[1],
                        root.location.origin[2],
                    ),
                    shape: root.shape.clone(),
                    capabilities: bace_motion::Capabilities {
                        speed: 0.0,
                        jump_impulse: 0.0,
                    },
                    heading: 2.0 * q[2].atan2(q[3]),
                    maximum_turn_rate: 0.0,
                },
            )
            .map_err(|_| PveError::MissingGeometry)?;
            roots.push((
                bace_entity::Actor {
                    id: root.entity,
                    cell: bace_types::CellId(root.location.cell),
                    body,
                },
                root.corpse.as_ref().map(|corpse| corpse.state.clone()),
            ));
        }
        self.world
            .preflight_pve_death_roots(proposal.victim, &roots)
            .map_err(|_| PveError::InvalidReceipt)?;
        self.population
            .stage_death(operation, crate::pve::PreparedPveDeath { forest, roots })
    }
    pub fn peek_pve_service_outcome(&self) -> Option<&crate::PveServiceOutcome> {
        self.pve_service_outcomes.front()
    }
    pub fn take_pve_service_outcome(&mut self) -> Option<crate::PveServiceOutcome> {
        self.pve_service_outcomes.pop_front()
    }
    pub(super) fn handle_pve_service(&mut self, command: crate::PveServiceCommand) {
        use crate::PveServiceAction as A;
        let result = if !command.valid_bounds() {
            Err(PveError::InvalidReceipt)
        } else {
            match command.action {
                A::Stage { operation, forest } => self.stage_pve_death(operation, *forest),
                A::Retry { operation } => self.retry_death(operation),
                A::Committed {
                    operation,
                    corpse,
                    items,
                    credits,
                    social: Some(social),
                } => self.confirm_prepared_pve_committed(
                    operation,
                    corpse,
                    &items,
                    &credits,
                    Some(&social),
                ),
                A::Committed {
                    operation,
                    corpse,
                    items,
                    credits,
                    social: None,
                } => self.confirm_prepared_pve_committed(operation, corpse, &items, &credits, None),
            }
        };
        self.pve_service_outcomes
            .push_back(crate::PveServiceOutcome {
                correlation: command.correlation,
                result,
            });
    }
}
