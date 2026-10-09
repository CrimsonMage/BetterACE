use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_entity::{Combatant, CombatantProfile};
use bace_gameplay_api::{ActionContext, CharacterBinding, CharacterUi, SessionId};
use bace_social::{SocialPreferences, SocialPresence};
use std::sync::Arc;

fn binding(id: u32) -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(id),
        account: bace_types::AccountId(id as u64),
        session: SessionId(id as u64),
    }
}

fn presence(id: u32, name: &str) -> SocialPresence {
    SocialPresence {
        identity: bace_gameplay_api::social::SocialIdentity {
            character: EntityId(id),
            account: bace_types::AccountId(id as u64),
            name: name.into(),
        },
        access: 0,
        online: true,
        appear_offline: false,
        afk: false,
        gagged: false,
        olthoi: false,
        no_olthoi_talk: false,
        ignore_fellowship_requests: false,
        auto_accept_fellowship: false,
        share_fellowship_loot: false,
        society: 0,
        listen_allegiance: true,
        listen_general: true,
        listen_trade: true,
        listen_lfg: true,
        listen_roleplay: true,
        listen_society: true,
    }
}

fn fixture() -> Kernel {
    let mut kernel = crate::synthetic_scenario(2, 0).unwrap();
    let rank = RankTable::new(&[0, 10]).unwrap();
    let tables = Arc::new(ProgressionTables {
        attributes: rank.clone(),
        vitals: rank.clone(),
        trained_skills: rank.clone(),
        specialized_skills: rank,
    });
    for (id, name) in [(1, "Granter"), (2, "Recipient")] {
        let binding = binding(id);
        kernel
            .register_character(
                binding,
                CharacterProgression::new(&[], tables.clone(), 0, 0).unwrap(),
            )
            .unwrap();
        kernel
            .register_character_ui(
                binding,
                crate::OwnedUiState {
                    state: CharacterUi {
                        options1: if id == 2 { 0x0008_0000 } else { 0 },
                        ..Default::default()
                    },
                    known_spells: vec![],
                    component_templates: vec![],
                    entered: true,
                },
            )
            .map_err(|e| e.0)
            .unwrap();
        kernel
            .register_social_presence(presence(id, name), SocialPreferences::default())
            .unwrap();
    }
    kernel
}

fn command(actor: u32, sequence: u32, request: R, unix_seconds: u64) -> CorpseConsentCommand {
    let binding = binding(actor);
    CorpseConsentCommand {
        correlation: u64::from(sequence),
        context: ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence,
        },
        request,
        unix_seconds,
    }
}

fn apply(kernel: &mut Kernel, command: CorpseConsentCommand) -> Vec<(EntityId, String)> {
    kernel.apply_corpse_consent_command(command);
    kernel
        .take_corpse_consent_outcome()
        .unwrap()
        .result
        .unwrap()
}

#[test]
fn recipient_grant_survives_granter_logout_and_expires_at_source_boundary() {
    let mut kernel = fixture();
    let granted = apply(&mut kernel, command(1, 1, R::Add("Recipient".into()), 100));
    assert_eq!(granted.len(), 2);
    assert!(granted[0].1.contains("Granter has given you permission"));
    let grant = &kernel.player_deaths.consent_grants[&EntityId(2)][&EntityId(1)];
    assert_eq!(grant.expires_at, 3700);
    assert_eq!(grant.granter_name, "Granter");
    kernel.take_social_presence(EntityId(1)).unwrap();
    let listed = apply(&mut kernel, command(2, 1, R::Display, 3700));
    assert!(listed[0].1.ends_with("Granter"));
    let removed = apply(
        &mut kernel,
        command(2, 2, R::RemoveFrom("Granter".into()), 3700),
    );
    assert!(removed[0].1.contains("removed your permissions"));
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
}

#[test]
fn expired_grant_is_pruned_and_replayed_sequence_cannot_restore_it() {
    let mut kernel = fixture();
    apply(&mut kernel, command(1, 1, R::Add("Recipient".into()), 100));
    let listed = apply(&mut kernel, command(2, 1, R::Display, 3701));
    assert_eq!(
        listed[0].1,
        "You do not have permission to loot anyone's corpse."
    );
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
    kernel.apply_corpse_consent_command(command(1, 1, R::Add("Recipient".into()), 3701));
    assert_eq!(
        kernel.take_corpse_consent_outcome().unwrap().result,
        Err(CorpseConsentError::Ownership)
    );
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
}

#[test]
fn revoke_and_clear_change_only_the_recipient_owned_grants() {
    let mut kernel = fixture();
    apply(&mut kernel, command(1, 1, R::Add("Recipient".into()), 100));
    let revoked = apply(
        &mut kernel,
        command(1, 2, R::Remove("Recipient".into()), 101),
    );
    assert_eq!(revoked.len(), 2);
    assert!(revoked[0].1.contains("revoked permission"));
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
    apply(&mut kernel, command(1, 3, R::Add("Recipient".into()), 102));
    let cleared = apply(&mut kernel, command(2, 1, R::Clear, 103));
    assert!(cleared[0].1.contains("cleared your consent list"));
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
}

#[test]
fn committed_corpse_open_consumes_transient_grant_and_keeps_corpse_right() {
    let mut kernel = fixture();
    apply(&mut kernel, command(1, 1, R::Add("Recipient".into()), 100));
    kernel
        .world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
                maximum_health: 10,
                melee_damage: 1,
                melee_range: 2.,
                attack_duration: 1.,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    kernel
        .world
        .combatant_mut(EntityId(1))
        .unwrap()
        .damage(10)
        .unwrap();
    let corpse = EntityId(3);
    kernel
        .world
        .create_corpse(
            EntityId(1),
            corpse,
            bace_world::CorpseState {
                operation: 44,
                source: EntityId(1),
                template: 1,
                owner: Some(EntityId(2)),
                items: vec![],
            },
        )
        .unwrap();
    kernel
        .register_corpse_access(
            corpse,
            44,
            crate::CorpseAccessProfile {
                victim: Some(EntityId(1)),
                killer: None,
                is_monster: false,
                generated_rare: false,
                pk_death: false,
                looted: false,
                permittees: vec![],
            },
        )
        .unwrap();
    let decision = kernel
        .inspect_corpse_access(corpse, EntityId(2), true, false)
        .unwrap();
    assert_eq!(
        decision,
        crate::CorpseAccessDecision::Open {
            consume_permit: true
        }
    );
    kernel
        .adopt_corpse_access(corpse, EntityId(2), true, false, decision)
        .unwrap();
    assert!(
        !kernel
            .player_deaths
            .consent_grants
            .contains_key(&EntityId(2))
    );
    let access = &kernel.player_deaths.corpse_access[&corpse];
    assert_eq!(access.profile.permittees, vec![EntityId(2)]);
    assert_eq!(access.viewer, Some(EntityId(2)));

    // Container.ActOnUse may close the prior viewer's corpse from elsewhere,
    // but an Adopt receipt still belongs to that viewer's exact session.
    let close = crate::CorpseAccessDecision::Close { mark_looted: true };
    let stale = crate::CorpseAccessCommand::Adopt {
        correlation: 81,
        context: ActionContext {
            actor: EntityId(2),
            account: binding(2).account,
            session: SessionId(99),
            sequence: 0,
        },
        corpse,
        has_loot_permit: false,
        decision: close,
    };
    kernel.apply_corpse_access_command(stale);
    assert!(matches!(
        kernel.take_corpse_access_outcome(),
        Some(crate::CorpseAccessOutcome::Adopted {
            correlation: 81,
            result: Err(crate::CorpseAccessError::Ownership),
            ..
        })
    ));
    assert_eq!(
        kernel.player_deaths.corpse_access[&corpse].viewer,
        Some(EntityId(2))
    );
    assert!(!kernel.player_deaths.corpse_access[&corpse].profile.looted);

    kernel.apply_corpse_access_command(crate::CorpseAccessCommand::Adopt {
        correlation: 82,
        context: ActionContext {
            actor: EntityId(2),
            account: binding(2).account,
            session: binding(2).session,
            sequence: 0,
        },
        corpse,
        has_loot_permit: false,
        decision: close,
    });
    assert!(matches!(
        kernel.take_corpse_access_outcome(),
        Some(crate::CorpseAccessOutcome::Adopted {
            correlation: 82,
            result: Ok(crate::CorpseAccessDecision::Close { mark_looted: true }),
            ..
        })
    ));
    assert_eq!(kernel.player_deaths.corpse_access[&corpse].viewer, None);
    assert!(kernel.player_deaths.corpse_access[&corpse].profile.looted);
}
