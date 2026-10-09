use super::tests::fixture;
use crate::{OwnedUiState, PlayerDeathEvent};
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_types::{AccountId, EntityId};

const ACTOR: EntityId = EntityId(0x5000_0001);

#[test]
fn protection_notice_freezes_entered_binding_at_source_transition() {
    let mut kernel = fixture();
    let binding = CharacterBinding {
        actor: ACTOR,
        account: AccountId(1),
        session: SessionId(1),
    };
    kernel
        .register_character_ui(
            binding,
            OwnedUiState {
                state: Default::default(),
                known_spells: Vec::new(),
                component_templates: Vec::new(),
                entered: true,
            },
        )
        .unwrap();
    kernel
        .player_deaths
        .states
        .get_mut(&ACTOR)
        .unwrap()
        .protection_elapsed = Some(1.);
    kernel.dispel_lifestone_protection(ACTOR).unwrap();
    assert_eq!(
        kernel.take_player_death_event(),
        Some(PlayerDeathEvent::ProtectionDispelled {
            actor: ACTOR,
            recipient: Some(binding),
        })
    );
    kernel
        .player_deaths
        .states
        .get_mut(&ACTOR)
        .unwrap()
        .protection_elapsed = Some(55.);
    kernel.step_player_death_timers(150).unwrap();
    assert_eq!(
        kernel.take_player_death_event(),
        Some(PlayerDeathEvent::ProtectionExpired {
            actor: ACTOR,
            recipient: Some(binding),
        })
    );
    kernel.death_policy.pk_server = true;
    kernel.death_policy.pk_respite_seconds = 10;
    kernel
        .player_deaths
        .states
        .get_mut(&ACTOR)
        .unwrap()
        .pk_respite_elapsed = Some(5.);
    kernel.step_player_death_timers(300).unwrap();
    assert_eq!(
        kernel.take_player_death_event(),
        Some(PlayerDeathEvent::PkStatus {
            actor: ACTOR,
            status: 4,
            recipient: Some(binding),
        })
    );
}
