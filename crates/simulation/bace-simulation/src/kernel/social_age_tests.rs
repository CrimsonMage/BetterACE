use super::*;
use crate::kernel::magic_components_fixture;
#[test]
fn held_valuable_save_keeps_age_revision_stable_then_accrues_all_elapsed_time() {
    let mut k = magic_components_fixture::component_kernel(16);
    let actor = EntityId(1);
    k.social.chat_eligibility.insert(
        actor,
        (
            bace_social::ChatEligibility {
                player_age_seconds: 40,
                ..Default::default()
            },
            0,
        ),
    );
    k.tick = 30;
    k.sync_social_age().unwrap();
    assert_eq!(k.social_player_age(actor), Some(41));
    let revision = k.character(actor).unwrap().revision();
    k.characters.reserve_death(actor, 99).unwrap();
    k.tick = 330;
    k.sync_social_age().unwrap();
    assert_eq!(k.social_player_age(actor), Some(41));
    assert_eq!(k.character(actor).unwrap().revision(), revision);
    k.characters.release_death(actor, 99).unwrap();
    k.sync_social_age().unwrap();
    assert_eq!(k.social_player_age(actor), Some(51));
    assert_eq!(k.character(actor).unwrap().revision(), revision + 1);
    k.sync_social_age().unwrap();
    assert_eq!(k.character(actor).unwrap().revision(), revision + 1);
    let binding = bace_gameplay_api::CharacterBinding {
        actor,
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
    };
    let snapshot = k.read_player_snapshot(binding).unwrap();
    assert_eq!(snapshot.chat_age(), Some(51));
    // Restore uses the persisted accepted age, and starts a new explicit tick epoch.
    let mut restored = magic_components_fixture::component_kernel(16);
    restored.social.chat_eligibility.insert(
        actor,
        (
            bace_social::ChatEligibility {
                player_age_seconds: snapshot.chat_age().unwrap(),
                ..Default::default()
            },
            0,
        ),
    );
    restored.tick = 60;
    restored.sync_social_age().unwrap();
    assert_eq!(restored.social_player_age(actor), Some(53));
}
