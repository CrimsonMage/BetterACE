//! Accepted crafting proposals already contain the full scalar quality delta.
//! Project only those scalars; no fake template/class/physics identity is needed.
use super::*;
impl Kernel {
    pub(super) fn prepare_crafting_wands(
        &self,
        ticket: &crate::CraftingTicket,
    ) -> Result<Vec<bace_magic::MagicWand>, bace_crafting::CraftError> {
        let crate::CraftingDecision::Tinker(proposal) = &ticket.decision else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        for item in [
            proposal.source_after.as_ref(),
            proposal.target_after.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if !self.magic.references_damage_wand(EntityId(item.id)) {
                continue;
            }
            let mut properties = bace_content::SparseProperties::default();
            for (key, value) in &item.properties {
                use bace_crafting::{PropertyKind as K, PropertyValue as V};
                match (key.kind, value) {
                    (K::Int, V::Int(value)) => properties.ints.push(bace_content::Property {
                        id: key.id,
                        value: *value,
                    }),
                    (K::Float, V::Float(value)) => properties.floats.push(bace_content::Property {
                        id: key.id,
                        value: *value,
                    }),
                    (K::Bool, V::Bool(value)) => properties.bools.push(bace_content::Property {
                        id: key.id,
                        value: *value,
                    }),
                    _ => {}
                }
            }
            result.push(
                bace_magic::prepare_magic_wand_properties(item.id, item.revision, &properties)
                    .map_err(|_| bace_crafting::CraftError::InvalidState)?,
            );
        }
        Ok(result)
    }
}
