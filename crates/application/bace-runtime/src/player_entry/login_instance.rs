//! PlayerEnterWorld initializes ObjectInstance from the committed TotalLogins.
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{OnlineLoginReceipt, OwnershipState};
pub fn instance_from_login(
    receipt: OnlineLoginReceipt,
    binding: CharacterBinding,
) -> Result<u16, String> {
    if receipt.lease.state != OwnershipState::Online
        || receipt.lease.epoch <= 0
        || receipt.lease.character_id != binding.actor.0
        || binding.actor.0 == 0
        || receipt.total_logins == 0
        || receipt.total_logins > i32::MAX as u32
    {
        return Err("entry login receipt identity/state mismatch".into());
    }
    // Unchecked C# ushort cast deliberately wraps; ownership epochs do not.
    Ok(receipt.total_logins as u16)
}
#[cfg(test)]
mod tests {
    use super::*;
    use bace_gameplay_api::SessionId;
    use bace_persistence::CharacterLease;
    use bace_types::{AccountId, EntityId};
    fn binding() -> CharacterBinding {
        CharacterBinding {
            actor: EntityId(1),
            session: SessionId(1),
            account: AccountId(1),
        }
    }
    fn receipt(count: u32) -> OnlineLoginReceipt {
        OnlineLoginReceipt {
            lease: CharacterLease {
                character_id: 1,
                epoch: 7,
                state: OwnershipState::Online,
            },
            total_logins: count,
        }
    }
    #[test]
    fn source_int32_to_ushort_login_sequence_boundaries() {
        let rows = include_str!("../../tests/fixtures/login_instance.csv");
        let mut count = 0;
        for line in rows.lines().filter(|l| !l.starts_with('#')) {
            let fields: Vec<u32> = line.split(',').map(|s| s.parse().unwrap()).collect();
            assert_eq!(
                instance_from_login(receipt(fields[1]), binding()).unwrap(),
                fields[2] as u16
            );
            count += 1;
        }
        assert_eq!(count, 7);
    }
    #[test]
    fn missing_stale_or_exhausted_counter_cannot_fabricate_entry_instance() {
        for count in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(instance_from_login(receipt(count), binding()).is_err());
        }
        let mut wrong = receipt(1);
        wrong.lease.character_id = 2;
        assert!(instance_from_login(wrong, binding()).is_err());
        wrong = receipt(1);
        wrong.lease.state = OwnershipState::Loading;
        assert!(instance_from_login(wrong, binding()).is_err());
    }
}
