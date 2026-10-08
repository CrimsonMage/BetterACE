//! Exact durable equipment metadata handoff. Existing reliable publications pin
//! their old immutable blueprints; later accepted views use the new attachment graph.
use super::*;
use crate::inventory_equipment_output::EquipmentVisibilityUpdate;
#[derive(Clone, Copy)]
pub(super) struct EquipmentReceipt {
    pub(super) operation: u64,
    before: u64,
    after: u64,
}
impl VisibilityService {
    /// Borrowed input remains with its inventory owner on every rejection. A
    /// repeated exact receipt is idempotent. Appearance never supplies a pose,
    /// velocity, accepted motion or teleport epoch.
    pub fn replace_equipment_blueprint(
        &mut self,
        update: &EquipmentVisibilityUpdate,
    ) -> Result<(), VisibilityServiceError> {
        if update.children.len() > self.limits.codec.max_children
            || update.children.len() != update.descriptions.len()
        {
            return Err(VisibilityServiceError::InvalidDescription);
        }
        let old = self
            .objects
            .get(&update.actor)
            .ok_or(VisibilityServiceError::Stale)?;
        let mut children = update.children.clone();
        let mut descriptions = update.descriptions.clone();
        if let Some(ammo) = old
            .ammunition_receipt
            .as_ref()
            .filter(|r| r.operation > update.operation)
        {
            super::ammunition::patch_child(&mut children, &mut descriptions, &ammo.change)?;
        }
        if update.operation == 0
            || update.after_revision <= update.before_revision
            || old.blueprint.incarnation != update.incarnation
            || old.blueprint.description.physics.sequences.instance != update.instance_sequence
        {
            return Err(VisibilityServiceError::Stale);
        }
        if let Some(receipt) = old.equipment_receipt {
            if receipt.operation == update.operation {
                return if receipt.before == update.before_revision
                    && receipt.after == update.after_revision
                    && old.blueprint.description.model == update.model
                    && old.blueprint.description.physics.options.children == children
                    && old.blueprint.children == descriptions
                {
                    Ok(())
                } else {
                    Err(VisibilityServiceError::Stale)
                };
            }
            if update.before_revision < receipt.after {
                return Err(VisibilityServiceError::Stale);
            }
        } else if update.before_revision < old.initial_character_revision {
            return Err(VisibilityServiceError::Stale);
        }
        if update
            .children
            .iter()
            .any(|child| child.object_id == update.actor.0 || child.object_id == 0)
            || update.descriptions.iter().any(|child| {
                child.physics.options.parent.is_none_or(|parent| {
                    parent.object_id != update.actor.0
                        || !update.children.iter().any(|entry| {
                            entry.object_id == child.object_id && entry.location == parent.location
                        })
                })
            })
        {
            return Err(VisibilityServiceError::InvalidDescription);
        }
        let mut description = (*old.blueprint.description).clone();
        description.model = update.model.clone();
        description.physics.options.children = children;
        let revision = old
            .blueprint
            .revision
            .checked_add(1)
            .ok_or(VisibilityServiceError::Capacity)?;
        let tick = old.blueprint.admitted_tick;
        // Existing bounded byte accounting includes old blueprints pinned by
        // in-flight reliable batches. Validation precedes the replacement.
        self.register_object(
            update.incarnation,
            revision,
            tick,
            Arc::new(description),
            descriptions,
        )?;
        self.objects
            .get_mut(&update.actor)
            .expect("validated retained object")
            .equipment_receipt = Some(EquipmentReceipt {
            operation: update.operation,
            before: update.before_revision,
            after: update.after_revision,
        });
        Ok(())
    }
}
