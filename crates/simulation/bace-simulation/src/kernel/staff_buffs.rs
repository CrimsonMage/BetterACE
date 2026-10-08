//! Sentinel buffs use prepared source spell order and accepted fellowship/equipment.
use super::*;
use bace_gameplay_api::staff::{StaffBaneItem, StaffBuffPlan, StaffError as E, StaffEvent};
impl Kernel {
    pub fn register_staff_buff_plans(&mut self, plans: Vec<StaffBuffPlan>) -> Result<(), E> {
        if plans.len() > 8 || !self.staff.buffs.is_empty() {
            return Err(E::Capacity);
        }
        let mut map = std::collections::BTreeMap::new();
        for plan in plans {
            if !(1..=8).contains(&plan.level)
                || plan.self_spells.len() > 128
                || plan.other_spells.len() > 128
                || plan.banes.len() > 16
                || plan.effects.len() > 256
                || plan.missing.len() > 256
                || plan.missing.iter().any(|s| s.len() > 128)
            {
                return Err(E::Invalid);
            }
            if map.insert(plan.level, plan).is_some() {
                return Err(E::Invalid);
            }
        }
        self.staff.buffs = map;
        Ok(())
    }
    pub fn staff_buff(
        &mut self,
        context: ActionContext,
        target: EntityId,
        fellowship: bool,
        maximum_level: u8,
        equipment: Vec<StaffBaneItem>,
        sudo: bool,
    ) -> Result<(), E> {
        if equipment.len() > 4096 || !self.staff.room(1) {
            return Err(E::Capacity);
        }
        let (targets, is_self) = if fellowship {
            self.fellowships.membership(target).map_or(
                (vec![target], target == context.actor),
                |g| {
                    (
                        g.members().to_vec(),
                        g.members().len() == 1 && g.leader() == context.actor,
                    )
                },
            )
        } else {
            (vec![target], target == context.actor)
        };
        if targets.len() > 9 {
            return Err(E::Capacity);
        }
        let plan = self
            .staff
            .buffs
            .get(&maximum_level.clamp(1, 8))
            .ok_or(E::Invalid)?
            .clone();
        self.authorize_staff(context, 2, sudo)?;
        let mut metadata = std::collections::BTreeMap::new();
        for item in equipment {
            if metadata.insert(item.item, item).is_some() {
                return Err(E::Invalid);
            }
        }
        let mut requests = Vec::new();
        let mut scripts = Vec::new();
        for actor in targets {
            if self.characters.get(actor).is_none() {
                return Err(E::MissingTarget);
            }
            if self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.npcs.reserved(actor)
                || self.housing.reserved(actor)
            {
                return Err(E::Busy);
            }
            if self
                .characters
                .get(actor)
                .is_none_or(|p| p.revision() == u64::MAX)
            {
                return Err(E::Overflow);
            }
            let spells = if is_self {
                &plan.self_spells
            } else {
                &plan.other_spells
            };
            let mut effects = std::collections::BTreeSet::new();
            for &spell in spells {
                requests.push((actor, spell));
                if let Some(&(_, effect)) = plan.effects.iter().find(|&&(id, _)| id == spell)
                    && effects.insert(effect)
                {
                    scripts.push((actor, effect));
                }
            }
            for item in self.inventory.items().filter(|item|matches!(item.place,bace_inventory::ItemPlace::Contained{container,equipped,..} if container==actor&&equipped!=0)){
    let prepared=metadata.remove(&item.id).ok_or(E::Invalid)?;
    if item.revision!=prepared.revision||item.template!=prepared.template{return Err(E::Stale);}
    if prepared.eligible{
     if item.revision==u64::MAX{return Err(E::Overflow);}
     if self.inventory.reserved(item.id){return Err(E::Busy);}
     for &spell in &plan.banes{requests.push((item.id,spell));}
    }
   }
        }
        if !metadata.is_empty() {
            return Err(E::Stale);
        }
        self.magic
            .staff_buffs(
                context.actor,
                &requests,
                self.tick as f64 / 30.,
                &self.staff.effects,
            )
            .map_err(|e| match e {
                bace_gameplay_api::CastRejection::Capacity => E::Capacity,
                bace_gameplay_api::CastRejection::Busy => E::Busy,
                _ => E::Invalid,
            })?;
        self.sync_registry_revisions().map_err(|_| E::Overflow)?;
        self.staff.push(StaffEvent::Scripts {
            context,
            targets: scripts,
        });
        Ok(())
    }
}
