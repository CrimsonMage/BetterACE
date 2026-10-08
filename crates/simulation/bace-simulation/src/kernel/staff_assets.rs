//! Staff-only direct-effect assets never enter the player's casting catalog.
use super::Kernel;
use bace_gameplay_api::staff::{StaffBuffPlan, StaffError as E, StaffSpellDefinition};
use bace_magic::{EnchantmentEntry, EnchantmentRegistry};
use std::collections::BTreeMap;
impl Kernel {
    pub fn register_staff_magic_assets(
        &mut self,
        definitions: Vec<StaffSpellDefinition>,
        plans: Vec<StaffBuffPlan>,
        effects: Vec<EnchantmentEntry>,
    ) -> Result<(), E> {
        if !self.staff.spells.is_empty()
            || !self.staff.buffs.is_empty()
            || !self.staff.effects.is_empty()
            || definitions.len() > 8192
            || plans.len() > 8
            || effects.len() > 4096
        {
            return Err(E::Capacity);
        }
        let mut spells = BTreeMap::new();
        for d in definitions {
            if d.spell == 0
                || d.spell > 65535
                || d.name.len() > 1024
                || d.enum_name.len() > 128
                || spells.insert(d.spell, d).is_some()
            {
                return Err(E::Invalid);
            }
        }
        let mut entries = BTreeMap::new();
        for entry in effects {
            let id = entry.spell;
            if !spells.contains_key(&id) || entries.contains_key(&id) || entry.caster != 0 {
                return Err(E::Invalid);
            }
            EnchantmentRegistry::restore(1, 0, vec![entry.clone()]).map_err(|_| E::Invalid)?;
            entries.insert(id, entry);
        }
        let mut buffs = BTreeMap::new();
        for p in plans {
            if !(1..=8).contains(&p.level)
                || p.self_spells.len() > 128
                || p.other_spells.len() > 128
                || p.banes.len() > 16
                || p.effects.len() > 256
                || p.missing.len() > 256
                || p.missing.iter().any(|s| s.len() > 128)
                || p.self_spells
                    .iter()
                    .chain(&p.other_spells)
                    .chain(&p.banes)
                    .any(|id| !entries.contains_key(id))
                || buffs.insert(p.level, p).is_some()
            {
                return Err(E::Invalid);
            }
        }
        self.staff.spells = spells;
        self.staff.buffs = buffs;
        self.staff.effects = entries;
        Ok(())
    }
}
