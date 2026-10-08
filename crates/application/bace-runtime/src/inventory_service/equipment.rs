//! Retained bounded cold preparation between capture and any database submission.
use super::*;
impl InventoryService {
    pub fn equipment_capture(&self) -> Option<(&InventoryWork, &Arc<PlayerReadSnapshot>, u64)> {
        let pending = self.pending.as_ref()?;
        let Phase::Captured(snapshot, unix) = &pending.phase else {
            return None;
        };
        (self.blocked.is_none()
            && pending.work.operation.equipment.is_some()
            && pending.work.operation.equipment_vitals.is_none())
        .then_some((&pending.work, snapshot, *unix))
    }
    pub fn supply_equipment_physical(
        &mut self,
        prepared: bace_simulation::PreparedEquipmentPhysical,
        correlation: u64,
    ) -> Result<(), Box<bace_simulation::PreparedEquipmentPhysical>> {
        let Some(p) = self.pending.as_mut() else {
            return Err(Box::new(prepared));
        };
        let Phase::Captured(snapshot, unix) = &p.phase else {
            return Err(Box::new(prepared));
        };
        if self.blocked.is_some()
            || correlation <= self.last_correlation
            || p.work.operation.equipment.is_none()
            || p.work.operation.equipment_vitals.is_some()
            || prepared.actor != p.work.binding.actor
            || prepared.before_revision != p.work.operation.actor_revision
        {
            return Err(Box::new(prepared));
        }
        p.phase = Phase::PreparingEquipment {
            snapshot: snapshot.clone(),
            unix: *unix,
            correlation,
            command: Some(Box::new(Command::Inventory(Box::new(InventoryCommand {
                correlation,
                kind: InventoryCommandKind::PrepareEquipmentPhysical {
                    operation: p.work.operation.ticket.operation,
                    prepared: Box::new(prepared),
                },
            })))),
        };
        self.last_correlation = correlation;
        Ok(())
    }
}
