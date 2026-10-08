//! Referenced portal closure from captured avatar ties and the accepted native
//! spell. Templates remain metadata until a real summon identity is allocated.
use super::*;
type PreparedPortalClosure = (
    Vec<(
        bace_interactions::PortalTemplate,
        Arc<bace_physics::CollisionShape>,
    )>,
    Vec<bace_types::CellId>,
);
impl StaffNativeMagicAssets {
    pub(super) fn prepare_portals(
        &mut self,
        source: &WeenieV1,
        effect: &bace_magic::SpellEffect,
        budget: &mut usize,
    ) -> Result<PreparedPortalClosure, String> {
        use bace_magic::{PortalEffect as P, SpellEffect as S};
        let effect = match effect {
            S::Portal(p) | S::FellowshipPortal(p) => p,
            _ => return Ok((Vec::new(), Vec::new())),
        };
        let mut templates = Vec::new();
        let mut cells = Vec::new();
        let tied = |did| {
            source
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == did && p.value != 0)
                .map(|p| p.value)
        };
        match effect {
            P::Recall { slot } if (3..=5).contains(slot) => {
                if let Some(id) = tied(match slot {
                    3 => 47,
                    4 => 31,
                    _ => 48,
                }) {
                    templates.push(id);
                }
            }
            P::Recall { slot } => {
                let position = match slot {
                    1 => 4,
                    2 => 15,
                    _ => return Err("portal recall index".into()),
                };
                if let Some(p) = source
                    .properties
                    .positions
                    .iter()
                    .find(|p| p.id == position)
                {
                    cells.push(bace_types::CellId(p.value.obj_cell_id));
                }
            }
            P::Summon { slot, template, .. } => {
                if let Some(id) = tied(if *slot <= 1 { 31 } else { 48 }) {
                    templates.push(id);
                }
                templates.push(*template);
            }
            P::Sending { cell, .. } => cells.push(bace_types::CellId(*cell)),
            P::Link { .. } => {}
        }
        templates.sort_unstable();
        templates.dedup();
        let mut prepared = Vec::with_capacity(templates.len());
        for id in templates {
            let PackLookup::Record(record) = self
                .generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err("referenced portal template missing".into());
            };
            *budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("portal source budget")?;
            let template: WeenieV1 =
                bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            if template.weenie_id != id {
                return Err("portal template identity".into());
            }
            let definition = crate::portal_preparation::prepare_portal_template(&template)
                .map_err(|e| format!("portal template: {e:?}"))?;
            let did = template
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 1)
                .ok_or("portal setup missing")?
                .value;
            let setup = CollisionSetup::decode(&read(&mut self.portal, did, budget)?)
                .map_err(|e| e.to_string())?;
            let shape = crate::world_admission::prepare_collision_shape(&setup, scale(&template)?)?;
            if let Some(destination) = definition.destination {
                cells.push(bace_types::CellId(destination.cell));
            }
            prepared.push((definition, shape));
        }
        cells.sort_unstable();
        cells.dedup();
        Ok((prepared, cells))
    }
}
