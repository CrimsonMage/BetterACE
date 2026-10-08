//! Retained physical contact and source-ordered spell side effects. Combat owns
//! the hit; Magic owns each proc and its descendants until an exact receipt.
use super::*;
use bace_combat::physical::{
    PhysicalContact, PhysicalDamageInput, SelectedPhysicalAttack, finish_physical_contact,
    physical_dirty_spells, prepare_physical_contact,
};
use bace_gameplay_api::{physical_procs::*, weapon_combat::*};
#[derive(Clone, Copy)]
struct FrozenInput {
    selected: SelectedPhysicalAttack,
    kind: PhysicalKind,
    hook_part: u32,
    height: u32,
    original_power: f32,
    power: f32,
    angle: f64,
    stamina: u32,
    in_combat: bool,
    rolls: PhysicalRolls,
}
impl FrozenInput {
    fn capture(i: PhysicalDamageInput<'_>) -> Self {
        Self {
            selected: i.selected,
            kind: i.kind,
            hook_part: i.hook_part,
            height: i.height,
            original_power: i.original_power,
            power: i.power,
            angle: i.target_angle_degrees,
            stamina: i.defender_stamina,
            in_combat: i.defender_in_combat,
            rolls: i.rolls,
        }
    }
    fn restore<'a>(self, mut i: PhysicalDamageInput<'a>) -> PhysicalDamageInput<'a> {
        i.selected = self.selected;
        i.kind = self.kind;
        i.hook_part = self.hook_part;
        i.height = self.height;
        i.original_power = self.original_power;
        i.power = self.power;
        i.target_angle_degrees = self.angle;
        i.defender_stamina = self.stamina;
        i.defender_in_combat = self.in_combat;
        i.rolls = self.rolls;
        i
    }
}
pub(super) struct PendingPhysicalHit {
    contact: PhysicalContact,
    input: FrozenInput,
    weapon: Option<EntityId>,
    cloak_roll: f64,
    request: Option<PhysicalProcRequest>,
    receipt: Option<PhysicalProcReceipt>,
    result: Option<PhysicalImpact>,
}
/// Returns an immutable final decision repeatedly until the owning hook commits.
/// No request is replayed after its matching receipt has advanced the phase.
pub(super) fn advance_hit(
    hits: &mut BTreeMap<PhysicalHitKey, PendingPhysicalHit>,
    capacity: usize,
    key: PhysicalHitKey,
    input: PhysicalDamageInput<'_>,
    weapon: Option<EntityId>,
    draws: [f64; 4],
) -> Result<Option<PhysicalImpact>, CombatRejection> {
    if !hits.contains_key(&key) {
        if hits.len() >= capacity {
            return Ok(None);
        }
        let contact =
            prepare_physical_contact(input).map_err(|_| CombatRejection::InvalidRequest)?;
        let request = if input.kind == PhysicalKind::Missile && contact.evaded() {
            None
        } else {
            Some(PhysicalProcRequest::Attack {
                key,
                weapon: weapon.zip(contact.weapon_proc()),
                sigil_target: (!contact.evaded()).then_some(key.target),
                sigil_rolls: [draws[0], draws[1], draws[2]],
            })
        };
        let result = if request.is_none() {
            Some(
                finish_physical_contact(input, contact)
                    .map_err(|_| CombatRejection::InvalidRequest)?,
            )
        } else {
            None
        };
        hits.insert(
            key,
            PendingPhysicalHit {
                contact,
                input: FrozenInput::capture(input),
                weapon,
                cloak_roll: draws[3],
                request,
                receipt: None,
                result,
            },
        );
    }
    let hit = hits.get_mut(&key).expect("inserted retained contact");
    if hit.request.is_none()
        && hit.receipt.is_none()
        && let Some(result) = hit.result
    {
        return Ok(Some(result));
    }
    let Some(receipt) = hit.receipt.take() else {
        return Ok(None);
    };
    let input = hit.input.restore(input);
    match receipt.phase {
        PhysicalProcPhase::Attack => {
            if hit.contact.evaded() {
                hit.result = Some(
                    finish_physical_contact(input, hit.contact)
                        .map_err(|_| CombatRejection::InvalidRequest)?,
                );
            } else {
                let (spells, count) = physical_dirty_spells(
                    input.attacker,
                    input.selected.skill_level,
                    input.height,
                    input.rolls.dirty,
                )
                .map_err(|_| CombatRejection::InvalidRequest)?;
                if count > 0 {
                    hit.request = Some(PhysicalProcRequest::Dirty {
                        key,
                        weapon: if input.kind == PhysicalKind::Melee && input.height == 2 {
                            None
                        } else {
                            hit.weapon
                        },
                        spells,
                        count,
                    });
                } else {
                    prepare_cloak(hit, key, input)?;
                }
            }
        }
        PhysicalProcPhase::Dirty => prepare_cloak(hit, key, input)?,
        PhysicalProcPhase::Cloak => {
            let damage = receipt.damage.ok_or(CombatRejection::InvalidRequest)?;
            let mut result = hit.result.take().ok_or(CombatRejection::InvalidRequest)?;
            result.damage = damage;
            hit.result = Some(result);
        }
    }
    // A prepared cloak has a frozen impact, but cannot publish it until its
    // request has an acknowledged completion.
    if hit.request.is_some() {
        Ok(None)
    } else {
        Ok(hit.result)
    }
}
fn prepare_cloak(
    hit: &mut PendingPhysicalHit,
    key: PhysicalHitKey,
    input: PhysicalDamageInput<'_>,
) -> Result<(), CombatRejection> {
    let impact =
        finish_physical_contact(input, hit.contact).map_err(|_| CombatRejection::InvalidRequest)?;
    hit.result = Some(impact);
    hit.request = Some(PhysicalProcRequest::Cloak {
        key,
        damage: impact.damage,
        roll: hit.cloak_roll,
    });
    Ok(())
}
impl Combat {
    pub(crate) fn resume_physical_procs(
        &mut self,
        world: &mut World,
        inventory: &crate::inventory::Inventory,
    ) {
        let now = self.simulation_tick as f64 / 30.0;
        self.step_physical(world, now, Some(inventory), true);
        // A completed target may expose the next member of the same accepted
        // cleave. This only prepares retained hook work, never a new hook.
        self.step_physical(world, now, Some(inventory), true);
        self.step_missiles(world, now, Some(inventory), true);
    }
    pub(crate) fn enable_physical_procs(&mut self) {
        self.physical_procs_enabled = true;
    }
    pub(crate) fn pending_physical_proc(&self) -> Option<PhysicalProcRequest> {
        self.physical_hits.values().find_map(|h| h.request.clone())
    }
    pub(crate) fn accept_physical_proc(
        &mut self,
        receipt: PhysicalProcReceipt,
    ) -> Result<(), CombatRejection> {
        let hit = self
            .physical_hits
            .get_mut(&receipt.key)
            .ok_or(CombatRejection::InvalidRequest)?;
        let request = hit
            .request
            .as_ref()
            .ok_or(CombatRejection::InvalidRequest)?;
        if request.key() != receipt.key || request.phase() != receipt.phase {
            return Err(CombatRejection::InvalidRequest);
        }
        hit.request = None;
        hit.receipt = Some(receipt);
        Ok(())
    }
    pub(crate) fn physical_proc_pending(&self, actor: EntityId) -> bool {
        self.physical_attacks.iter().any(|(source, attack)| {
            attack.hook_cost.is_some()
                && (*source == actor
                    || attack
                        .pending_targets
                        .as_ref()
                        .is_some_and(|targets| targets.0[..targets.1].contains(&Some(actor))))
        }) || self
            .physical_hits
            .keys()
            .any(|key| key.attacker == actor || key.target == actor)
    }
}
pub(super) fn proc_draws(
    stream: &bace_random::RandomStream,
    ordinal: usize,
    target: EntityId,
) -> Result<[f64; 4], CombatRejection> {
    let mut stream = stream
        .fork(
            b"physical-procs",
            ((ordinal as u64) << 32) | u64::from(target.0),
        )
        .map_err(|_| CombatRejection::InvalidRequest)?;
    let mut out = [0.0; 4];
    for value in &mut out {
        *value = (stream
            .next_u64()
            .map_err(|_| CombatRejection::InvalidRequest)?
            >> 11) as f64
            / 9_007_199_254_740_992.0;
    }
    Ok(out)
}

pub(super) fn contact_evaded(
    hits: &BTreeMap<PhysicalHitKey, PendingPhysicalHit>,
    key: PhysicalHitKey,
) -> Option<bool> {
    hits.get(&key).map(|h| h.contact.evaded())
}

pub(super) fn has_pending_request(
    hits: &BTreeMap<PhysicalHitKey, PendingPhysicalHit>,
    actor: EntityId,
    operation: u64,
) -> bool {
    hits.iter().any(|(key, hit)| {
        key.attacker == actor && key.operation == operation && hit.request.is_some()
    })
}
