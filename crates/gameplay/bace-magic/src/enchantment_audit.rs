//! Pinned ACE Player_Spells.AuditItemSpells. Set membership is prepared from
//! authoritative equipped items and spell-set assets, never inferred from buffs.
use crate::{EnchantmentRegistry, RegistryError};

#[derive(Clone, Copy)]
pub struct EquippedSpellSet<'a> {
    pub id: u32,
    pub possible: &'a [u32],
    pub active: &'a [u32],
}
#[derive(Clone, Copy)]
pub struct EquippedSpellOwner<'a> {
    pub id: u32,
    pub equipped: bool,
    pub set: Option<EquippedSpellSet<'a>>,
}
/// Atomically remove dangling equipped and inactive set effects before login.
/// Timed effects and vitae are unchanged. Returns whether a dirty revision arose.
pub fn audit_equipped_spells(
    registry: &mut EnchantmentRegistry,
    possessions: &[EquippedSpellOwner<'_>],
) -> Result<bool, RegistryError> {
    if possessions.len() > 1024 {
        return Err(RegistryError::Capacity);
    }
    let mut owners = std::collections::BTreeMap::new();
    for owner in possessions {
        if owner.id == 0 || owners.insert(owner.id, owner).is_some() {
            return Err(RegistryError::InvalidEntry);
        }
        if let Some(set) = owner.set {
            if set.id == 0 || set.possible.len() > 4096 || set.active.len() > 4096 {
                return Err(RegistryError::InvalidEntry);
            }
            if set.active.iter().any(|id| !set.possible.contains(id)) {
                return Err(RegistryError::InvalidEntry);
            }
        }
    }
    let permanent = |e: &crate::EnchantmentEntry| e.spec.duration == -1.0 && e.spell != 666;
    let mut inactive = std::collections::BTreeSet::new();
    for entry in registry.entries().iter().filter(|e| permanent(e)) {
        let Some(owner) = owners.get(&entry.caster) else {
            continue;
        };
        if !entry.metadata.has_spell_set_id && !owner.equipped {
            continue;
        }
        if let Some(set) = owner.set {
            for spell in set.possible.iter().filter(|id| !set.active.contains(id)) {
                if inactive.len() >= 4096 && !inactive.contains(&(set.id, *spell)) {
                    return Err(RegistryError::Capacity);
                }
                inactive.insert((set.id, *spell));
            }
        }
    }
    let keep = |entry: &&crate::EnchantmentEntry| {
        if !permanent(entry) {
            return true;
        }
        let Some(owner) = owners.get(&entry.caster) else {
            return false;
        };
        (entry.metadata.has_spell_set_id || owner.equipped)
            && !inactive.contains(&(entry.metadata.spell_set_id as u32, entry.spell))
    };
    let entries: Vec<_> = registry.entries().iter().filter(keep).cloned().collect();
    if entries.len() == registry.entries().len() {
        return Ok(false);
    }
    let revision = registry
        .revision()
        .checked_add(1)
        .ok_or(RegistryError::RevisionExhausted)?;
    let next =
        EnchantmentRegistry::restore(registry.capacity(), revision, entries).map_err(|e| e.0)?;
    *registry = next;
    Ok(true)
}
