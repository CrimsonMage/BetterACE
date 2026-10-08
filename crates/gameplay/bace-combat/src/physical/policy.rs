use bace_gameplay_api::weapon_combat::{PhysicalCombatProfile, PkStatus};
/// CWeenieObject::IsValidPkAction; immunity/attackability is a separate gate.
pub fn pk_action_allowed(helpful: bool, a: PkStatus, b: PkStatus) -> bool {
    use PkStatus::*;
    if !helpful && a == Protected {
        return false;
    }
    !matches!(
        (a, b),
        (Npk, Pk) | (Pk, Npk) | (Pk, PkLite) | (PkLite, Pk) | (Npk, PkLite)
    ) && (helpful || !matches!((a, b), (PkLite, Npk)))
}
pub fn hostile_pk_allowed(a: PkStatus, b: PkStatus) -> bool {
    let mask = |p| match p {
        PkStatus::Pk => 4,
        PkStatus::PkLite => 64,
        _ => 0,
    };
    let am = mask(a);
    let bm = mask(b);
    if am != bm {
        return false;
    }
    if am == 0
        && !matches!(a, PkStatus::Free | PkStatus::Baelzharon)
        && !matches!(b, PkStatus::Free | PkStatus::Baelzharon)
    {
        return false;
    }
    pk_action_allowed(false, a, b)
}
pub fn physical_permission(a: &PhysicalCombatProfile, b: &PhysicalCombatProfile) -> bool {
    physical_permission_with_status(a, b, a.pk, b.pk)
}
pub fn physical_permission_with_status(
    a: &PhysicalCombatProfile,
    b: &PhysicalCombatProfile,
    ap: PkStatus,
    bp: PkStatus,
) -> bool {
    b.attackable
        && !b.immune
        && !b.lifestone_protected
        && (!a.player || !b.player || hostile_pk_allowed(ap, bp))
}
