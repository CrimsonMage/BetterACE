//! Item XP is proposed against accepted equipment, reserved with inventory, and
//! adopted only by the same durable reward composite as player XP.
mod completion;
mod preparation;
use super::*;
use crate::item_experience::*;
use bace_gameplay_api::InventoryRejection as E;
impl Kernel {
    pub fn validate_item_experience_events_batch<'a>(
        &self,
        rewards: impl IntoIterator<Item = &'a ItemExperienceReward>,
    ) -> Result<(), E> {
        let count = rewards
            .into_iter()
            .try_fold(0usize, |n, r| n.checked_add(r.events.len()))
            .ok_or(E::Capacity)?;
        if count > self.item_experience.capacity - self.item_experience.events.len() {
            return Err(E::Capacity);
        }
        Ok(())
    }

    pub fn register_item_experience(
        &mut self,
        prepared: PreparedItemExperience,
    ) -> Result<(), (E, Box<PreparedItemExperience>)> {
        let result = (|| {
            let item = self.inventory.item(prepared.item).ok_or(E::MissingItem)?;
            if self.inventory.reserved(prepared.item) || self.inventory.reserved(prepared.actor) {
                return Err(E::DurabilityPending);
            }
            if !self.inventory.owned(prepared.actor, prepared.item) {
                return Err(E::InvalidState);
            }
            prepared.validate(item)?;
            if !self.item_experience.items.contains_key(&prepared.item)
                && self.item_experience.items.len() >= self.item_experience.capacity
            {
                return Err(E::Capacity);
            }
            Ok(())
        })();
        if let Err(error) = result {
            return Err((error, Box::new(prepared)));
        }
        self.item_experience.items.insert(prepared.item, prepared);
        Ok(())
    }
    pub fn item_experience(&self, item: EntityId) -> Option<&PreparedItemExperience> {
        self.item_experience.items.get(&item)
    }
    pub fn peek_item_experience_event(&self) -> Option<&ItemExperienceEvent> {
        self.item_experience.events.front()
    }
    pub fn take_item_experience_event(&mut self) -> Option<ItemExperienceEvent> {
        self.item_experience.events.pop_front()
    }
    pub fn has_item_experience_state(&self) -> bool {
        !self.item_experience.items.is_empty() || !self.item_experience.events.is_empty()
    }
    pub fn take_item_experience(
        &mut self,
        item: EntityId,
    ) -> Result<Option<PreparedItemExperience>, E> {
        if self.inventory.reserved(item)
            || self.item_experience.events.iter().any(|e| e.item == item)
        {
            return Err(E::DurabilityPending);
        }
        Ok(self.item_experience.items.remove(&item))
    }
}
