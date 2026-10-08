//! Whole-pool equipment changes join the same explicit inventory reservation.
use super::*;
use bace_entity::{EntityVital, VitalMutation};
impl World {
    pub fn validate_equipment_vitals(
        &self,
        actor: EntityId,
        token: VitalReservationToken,
        before: [u32; 3],
        after: [u32; 3],
        maxima: [u32; 3],
    ) -> Result<(), WorldError> {
        if token.domain != VitalReservationDomain::Equipment || token.operation == 0 {
            return Err(WorldError::VitalReserved);
        }
        self.validate_health_observations((before[0] != after[0]).then_some(actor))?;
        let body = self
            .combatants
            .get(&actor)
            .ok_or(WorldError::MissingActor)?;
        body.validate_vital_maxima(maxima)
            .map_err(|_| WorldError::InvalidVital)?;
        if body.revision().checked_add(4).is_none() {
            return Err(WorldError::InvalidVital);
        }
        for (index, vital) in [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
        {
            if self.vital_reservations.get(&(actor, vital)) != Some(&token)
                || body.vital(vital).is_none_or(|v| v.current != before[index])
                || after[index] > before[index]
                || after[index] > maxima[index]
            {
                return Err(WorldError::InvalidVital);
            }
        }
        Ok(())
    }
    pub fn adopt_equipment_vitals(
        &mut self,
        actor: EntityId,
        token: VitalReservationToken,
        before: [u32; 3],
        after: [u32; 3],
        maxima: [u32; 3],
    ) {
        self.validate_equipment_vitals(actor, token, before, after, maxima)
            .expect("preflighted equipment pools");
        let changes: Vec<_> = [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
            .map(|(i, vital)| VitalMutation {
                actor,
                vital,
                before: before[i],
                after: after[i],
            })
            .collect();
        self.apply_validated_vital_batch(&changes, None);
        self.combatants
            .get_mut(&actor)
            .expect("equipment vital owner")
            .replace_vital_maxima(maxima)
            .expect("preflighted equipment maxima");
    }
}
