//! A committed stack/remove receipt patches only an already prepared public
//! child. No model, attachment or accepted pose is reconstructed from gameplay.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AmmunitionReceipt {
    pub(super) operation: u64,
    pub(super) change: bace_inventory::ItemChange,
}
impl VisibilityService {
    pub fn apply_ammunition_child(
        &mut self,
        binding: CharacterBinding,
        operation: u64,
        change: &bace_inventory::ItemChange,
    ) -> Result<(), VisibilityServiceError> {
        let before = change
            .before
            .as_ref()
            .ok_or(VisibilityServiceError::InvalidDescription)?;
        let after = &change.after;
        let removed = after.place == bace_inventory::ItemPlace::Removed;
        if operation == 0
            || before.id != after.id
            || before.revision.checked_add(1) != Some(after.revision)
            || !matches!(before.place,bace_inventory::ItemPlace::Contained{container,equipped,..} if container==binding.actor && equipped!=0)
            || (!removed
                && (after.place != before.place
                    || before.stack.checked_sub(1) != Some(after.stack)))
            || (removed && before.stack != 1)
        {
            return Err(VisibilityServiceError::InvalidDescription);
        }
        let Some(old) = self
            .objects
            .get(&binding.actor)
            .filter(|o| o.blueprint.incarnation == binding.session.0)
        else {
            return Ok(());
        };
        let receipt = AmmunitionReceipt {
            operation,
            change: change.clone(),
        };
        if let Some(previous) = &old.ammunition_receipt {
            if previous.operation == operation {
                return if **previous == receipt {
                    Ok(())
                } else {
                    Err(VisibilityServiceError::Stale)
                };
            }
            if previous.operation > operation {
                return Ok(());
            }
        }
        if old
            .equipment_receipt
            .is_some_and(|r| r.operation > operation)
        {
            return Ok(());
        }
        let mut description = (*old.blueprint.description).clone();
        let mut children = old.blueprint.children.clone();
        if patch_child(
            &mut description.physics.options.children,
            &mut children,
            change,
        )? {
            let revision = old
                .blueprint
                .revision
                .checked_add(1)
                .ok_or(VisibilityServiceError::Capacity)?;
            let tick = old.blueprint.admitted_tick;
            self.register_object(
                binding.session.0,
                revision,
                tick,
                Arc::new(description),
                children,
            )?;
        }
        self.objects
            .get_mut(&binding.actor)
            .expect("retained incarnation")
            .ammunition_receipt = Some(Box::new(receipt));
        Ok(())
    }
}

pub(super) fn patch_child(
    attachments: &mut Vec<bace_wire::PhysicsChild>,
    children: &mut Vec<Arc<ObjectDescription>>,
    change: &bace_inventory::ItemChange,
) -> Result<bool, VisibilityServiceError> {
    let after = &change.after;
    if let Some(index) = children
        .iter()
        .position(|child| child.object_id == after.id.0)
    {
        if after.place == bace_inventory::ItemPlace::Removed {
            children.remove(index);
            attachments.retain(|child| child.object_id != after.id.0);
        } else {
            let child = Arc::make_mut(&mut children[index]);
            child.game.options.stack_size = Some(
                u16::try_from(after.stack)
                    .map_err(|_| VisibilityServiceError::InvalidDescription)?,
            );
            if child.game.options.value.is_some() {
                child.game.options.value = Some(
                    i32::try_from(
                        after
                            .unit_value
                            .checked_mul(after.stack)
                            .ok_or(VisibilityServiceError::InvalidDescription)?,
                    )
                    .map_err(|_| VisibilityServiceError::InvalidDescription)?,
                );
            }
            if child.game.options.burden.is_some() {
                child.game.options.burden = Some(
                    u16::try_from(
                        after
                            .unit_burden
                            .checked_mul(after.stack)
                            .ok_or(VisibilityServiceError::InvalidDescription)?,
                    )
                    .map_err(|_| VisibilityServiceError::InvalidDescription)?,
                );
            }
        }
        return Ok(true);
    }
    Ok(false)
}
