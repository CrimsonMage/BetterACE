use bace_content::{Property, WeenieV1};
use bace_gameplay_api::{ActionContext, SessionId, staff::StaffSpellTicket};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::staff_spell_saves::freeze_staff_spell;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};
use bace_types::{AccountId, EntityId};
#[test]
fn exact_spellbook_patch_preserves_other_fields_and_existing_probabilities() {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "player".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.spell_book = vec![Property {
        id: 10,
        value: 0.25,
    }];
    state.properties.ints = vec![Property {
        id: 9999,
        value: 123,
    }];
    let saved = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x50000001,
                template_revision: 1,
                mutation_revision: 4,
                state,
            },
            account_id: 1,
            name: "Player".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let ticket = StaffSpellTicket {
        operation: 1,
        context: ActionContext {
            session: SessionId(1),
            account: AccountId(1),
            actor: EntityId(0x50000001),
            sequence: 1,
        },
        spell: 20,
        name: "New spell".into(),
        learn: true,
        before_revision: 4,
        after_revision: 5,
        before: vec![10],
        after: vec![10, 20],
    };
    let lease = CharacterLease {
        character_id: 0x50000001,
        epoch: 2,
        state: OwnershipState::Online,
    };
    let pending = freeze_staff_spell([9; 16], &ticket, &saved, 3, lease).unwrap();
    let next = PlayerSaveV6::decode(&pending.operation().snapshots[0].bytes).unwrap();
    assert_eq!(
        next.player.entity.state.properties.spell_book,
        vec![
            Property {
                id: 10,
                value: 0.25
            },
            Property { id: 20, value: 1. }
        ]
    );
    assert_eq!(
        next.player.entity.state.properties.ints,
        saved.player.entity.state.properties.ints
    );
    assert_eq!(next.enchantments, saved.enchantments);
    assert_eq!(next.ui, saved.ui);
    let mut forged = ticket.clone();
    forged.after.push(30);
    assert!(freeze_staff_spell([9; 16], &forged, &saved, 3, lease).is_err());
    let mut stale = saved.clone();
    stale.player.entity.mutation_revision = 5;
    assert!(freeze_staff_spell([9; 16], &ticket, &stale, 3, lease).is_err());
    assert!(freeze_staff_spell([0; 16], &ticket, &saved, 3, lease).is_err());
}
