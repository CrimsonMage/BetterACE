use super::*;

fn profile() -> CorpseAccessProfile {
    CorpseAccessProfile {
        victim: Some(EntityId(0x5000_0001)),
        killer: Some(EntityId(0x5000_0002)),
        is_monster: false,
        generated_rare: false,
        pk_death: false,
        looted: false,
        permittees: Vec::new(),
    }
}

#[test]
fn source_victim_killer_permit_and_fellowship_order() {
    let p = profile();
    let stranger = EntityId(0x5000_0003);
    assert_eq!(
        decide(&p, None, p.victim.unwrap(), false, false, None),
        D::Open {
            consume_permit: false
        }
    );
    assert_eq!(
        decide(&p, None, p.killer.unwrap(), false, false, None),
        D::Open {
            consume_permit: false
        }
    );
    assert_eq!(
        decide(&p, None, stranger, false, false, None),
        D::Denied(Denial::Locked)
    );
    assert_eq!(
        decide(&p, None, stranger, true, false, None),
        D::Open {
            consume_permit: true
        }
    );
    assert_eq!(
        decide(&p, None, stranger, false, true, None),
        D::Open {
            consume_permit: false
        }
    );
    let mut reopened = p.clone();
    reopened.permittees.push(stranger);
    assert_eq!(
        decide(&reopened, None, stranger, false, false, None),
        D::Open {
            consume_permit: false
        }
    );
    let mut looted = p;
    looted.looted = true;
    assert_eq!(
        decide(&looted, None, stranger, false, false, None),
        D::Open {
            consume_permit: false
        }
    );
}

#[test]
fn source_rare_pk_and_monster_public_boundary() {
    let stranger = EntityId(0x5000_0003);
    let mut p = profile();
    p.is_monster = true;
    assert_eq!(
        decide(&p, None, stranger, false, false, Some(180 * 30)),
        D::Denied(Denial::Locked)
    );
    assert_eq!(
        decide(&p, None, stranger, false, false, Some(180 * 30 - 1)),
        D::Open {
            consume_permit: false
        }
    );
    p.generated_rare = true;
    assert_eq!(
        decide(&p, None, stranger, false, true, Some(0)),
        D::Denied(Denial::Rare)
    );
    assert_eq!(
        decide(&p, None, stranger, true, false, Some(0)),
        D::Open {
            consume_permit: true
        }
    );
    p.generated_rare = false;
    p.pk_death = true;
    p.is_monster = false;
    assert_eq!(
        decide(&p, None, stranger, true, true, None),
        D::Denied(Denial::PlayerKiller)
    );
}

#[test]
fn one_viewer_blocks_other_users_and_close_marks_looted_once() {
    let p = profile();
    let viewer = p.victim.unwrap();
    let other = p.killer.unwrap();
    assert_eq!(
        decide(&p, Some(viewer), viewer, false, false, None),
        D::Close { mark_looted: true }
    );
    assert_eq!(
        decide(&p, Some(viewer), other, false, false, None),
        D::Denied(Denial::InUse)
    );
    let mut looted = p;
    looted.looted = true;
    assert_eq!(
        decide(&looted, Some(viewer), viewer, false, false, None),
        D::Close { mark_looted: false }
    );
}

#[test]
fn malformed_durable_rights_are_rejected() {
    let mut p = profile();
    p.permittees = vec![EntityId(8), EntityId(7)];
    assert_eq!(p.validate(), Err(E::Invalid));
    p.permittees = vec![EntityId(7), EntityId(7)];
    assert_eq!(p.validate(), Err(E::Invalid));
    p.permittees = (1..=1025).map(EntityId).collect();
    assert_eq!(p.validate(), Err(E::Invalid));
}
