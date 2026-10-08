//! Pinned ACE Player.SetEphemeralValues, Player_Properties and PropertyManager
//! startup defaults. Authenticated account access is never taken from a packet.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct PlayerLoginPolicy {
    pub override_character_permissions: bool,
    pub require_spell_components: bool,
    pub safe_spell_components: bool,
    pub enforce_portal_maximum_level: bool,
}
impl Default for PlayerLoginPolicy {
    fn default() -> Self {
        Self {
            override_character_permissions: true,
            require_spell_components: true,
            safe_spell_components: false,
            enforce_portal_maximum_level: true,
        }
    }
}
impl PlayerLoginPolicy {
    pub fn prepare(
        self,
        loaded: &LoadedPlayer,
        account: &bace_auth::AccountRecord,
        created: Option<i64>,
        unix: u64,
    ) -> Result<crate::player_assets::PlayerColdPolicy, String> {
        if account.disabled || account.id != loaded.binding.account {
            return Err("cold player account authority mismatch".into());
        }
        let p = &loaded.player.player.entity.state.properties;
        let flag = |id, default| {
            p.bools
                .iter()
                .find(|v| v.id == id)
                .map_or(default, |v| v.value)
        };
        let integer = |id| p.ints.iter().find(|v| v.id == id).map(|v| v.value);
        let access = account.access_level as u8;
        let mut privileges = bace_gameplay_api::staff::StaffPrivileges {
            account_access: access,
            advocate: flag(47, false),
            sentinel: flag(46, false),
            envoy: flag(9005, false),
            developer: flag(45, false),
            admin: flag(44, false),
            psr: flag(97, false),
        };
        if self.override_character_permissions {
            match access {
                5 => privileges.admin = true,
                4 => privileges.developer = true,
                3 => {
                    privileges.envoy = true;
                    privileges.sentinel = true;
                }
                2 => privileges.sentinel = true,
                1 => privileges.advocate = true,
                _ => {}
            }
            // AdvocateQuest is an independent persisted entitlement. It is not
            // inferred from a account name or elevated password credential.
            if access != 1 && !flag(43, false) {
                privileges.advocate = false;
            }
        }
        let level = u32::try_from(integer(25).ok_or("source player level missing")?)
            .map_err(|_| "negative player level")?;
        let pk_status =
            u32::try_from(integer(134).unwrap_or(2)).map_err(|_| "negative PK status")?;
        let age = created.is_some_and(|created| {
            created >= 0
                && (unix / 1000)
                    .checked_sub(created as u64)
                    .is_some_and(|seconds| seconds >= 15 * 86400)
        });
        Ok(crate::player_assets::PlayerColdPolicy {
            account_created_unix: created,
            staff: bace_gameplay_api::staff::StaffRegistration {
                binding: loaded.binding,
                privileges,
            },
            components_required: flag(68, true) && self.require_spell_components,
            safe_components: flag(20, false) || self.safe_spell_components,
            portal_access: bace_interactions::PortalAccess {
                level,
                pk_status,
                pk_recent: false,
                olthoi: matches!(integer(188), Some(13 | 14)),
                vitae: loaded.player.enchantments.iter().any(|e| e.spell_id == 666),
                account_15_days: flag(127, false) || age,
                // Legacy account entitlement portal test is disabled at the pin;
                // quest predicates are checked against each prepared destination.
                entitlement: 0,
                quest_allowed: true,
                teleporting: false,
                recently_teleported: false,
                ignore_restrictions: flag(80, false),
                enforce_maximum_level: self.enforce_portal_maximum_level,
            },
        })
    }
}
