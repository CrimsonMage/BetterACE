//! Joint owner regression using the parent test's real DAT-prepared avatar.
use bace_gameplay_api::{ActionContext, CharacterBinding, InventoryRequest};
use bace_inventory::{InventoryAuthority, ItemPlace};
use bace_runtime::player_assets::*;
use bace_simulation::*;
use bace_storage_codec::{ItemPlacementV2, ItemSaveV4};
use bace_types::EntityId;
fn command(k: &mut Kernel, correlation: u64, kind: InventoryCommandKind) -> InventoryOutcome {
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation,
        kind,
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    let out = k.take_inventory_outcome().unwrap();
    assert_eq!(out.correlation, correlation);
    out
}
fn receipt(op: &InventoryOperation) -> InventoryReceipt {
    InventoryReceipt {
        operation: op.ticket.operation,
        revisions: op
            .ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
pub fn exercise(
    k: &mut Kernel,
    b: CharacterBinding,
    source: &bace_content::WeenieV1,
    dat: &PreparedAvatarDat,
    mut gear: ItemSaveV4,
) {
    let id = EntityId(gear.entity.object_id);
    gear.previous
        .entity
        .state
        .properties
        .strings
        .push(bace_content::Property {
            id: 1,
            value: "Authored equipment".into(),
        });
    gear.previous.entity.state.properties.ints.extend([
        bace_content::Property { id: 319, value: 6 },
        bace_content::Property { id: 320, value: 1 },
    ]);
    gear.previous
        .entity
        .state
        .properties
        .int64s
        .push(bace_content::Property { id: 5, value: 100 });
    gear.previous.placement = ItemPlacementV2::Contained {
        container: b.actor.0,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let (item, _) = bace_runtime::generator_items::prepare_inventory_item(
        &gear.entity.state,
        id,
        gear.entity.mutation_revision,
        ItemPlace::Contained {
            container: b.actor,
            slot: 0,
            equipped: 0,
        },
    )
    .unwrap();
    k.register_inventory_item(item).unwrap();
    k.register_magic_registry(
        id,
        bace_magic::EnchantmentRegistry::new(4096).unwrap(),
        false,
    )
    .ok()
    .unwrap();
    for (iteration, location) in [(0, 0x200), (1, 0)] {
        let before = k.read_player_snapshot(b).unwrap();
        let revision = before.character().progression().revision();
        let effects = bace_runtime::equipment_effects::prepare_equipment_effect_inputs(
            b.actor,
            revision,
            std::slice::from_ref(&gear),
            before.item_experience(),
            &dat.spells,
            &Default::default(),
        )
        .unwrap();
        let request = if location != 0 {
            InventoryRequest::Equip { item: id, location }
        } else {
            InventoryRequest::Move {
                item: id,
                container: b.actor,
                placement: 0,
            }
        };
        let authority = InventoryAuthority {
            actor: b.actor,
            busy: false,
            in_range: true,
            clear_path: true,
            geometry_ready: true,
            drop_validated: false,
            source_view: None,
            destination_view: None,
            new_item: None,
        };
        let result = command(
            k,
            100 + iteration * 10,
            InventoryCommandKind::ProposeEquipment(Box::new(PreparedEquipmentRequest {
                request: InventoryPreparedRequest {
                    context: ActionContext {
                        actor: b.actor,
                        session: b.session,
                        account: b.account,
                        sequence: iteration as u32 + 1,
                    },
                    request,
                    authority,
                    split: None,
                    drop: None,
                },
                wield: prepare_wield_policy(b.actor, &gear.entity.state, 0),
                slots: vec![prepare_wield_slot(&gear)],
                effects,
                stance: None,
            })),
        );
        let InventoryDecision::Proposed(op) = result.result.expect("equipment proposal") else {
            panic!("equipment proposal");
        };
        assert!(op.equipment.is_some());
        assert!(op.equipment_vitals.is_none());
        assert_eq!(
            k.inventory_item(id).unwrap().revision,
            gear.entity.mutation_revision
        );
        assert!(
            k.read_player_snapshot(b).is_err(),
            "ordinary reads cannot cross equipment hold"
        );
        assert!(
            command(
                k,
                101 + iteration * 10,
                InventoryCommandKind::Commit(receipt(&op))
            )
            .result
            .is_err(),
            "receipt cannot skip physical companion"
        );
        let captured = k
            .read_player_operation_snapshot(
                b,
                PlayerSnapshotOperation::Inventory(op.ticket.operation),
                revision,
            )
            .unwrap();
        let change = op
            .ticket
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == id)
            .unwrap();
        let mut after = gear.clone();
        after.previous.entity.mutation_revision = change.after.revision;
        after.previous.placement = ItemPlacementV2::Contained {
            container: b.actor.0,
            slot: 0,
            pack_slot: false,
            equipped: location,
        };
        let candidate = op.equipment.as_ref().unwrap().registry(b.actor).unwrap();
        let prepared = prepare_equipment_physical_view(EquipmentPhysicalViewInput {
            previous_style: captured.entry_motion().map(|m| m.style),
            actor: b.actor,
            before_revision: revision,
            source,
            character: captured.character().progression(),
            registry: candidate
                .as_ref()
                .or_else(|| captured.enchantments())
                .unwrap(),
            item_registries: captured
                .item_enchantments()
                .iter()
                .map(|(id, r)| (*id, r))
                .collect(),
            item_experience: captured.item_experience(),
            items: std::slice::from_ref(&after),
            dat,
            projectile_shapes: &Default::default(),
        })
        .unwrap();
        let mut wrong = prepared.clone();
        wrong.actor = EntityId(123);
        assert!(
            command(
                k,
                102 + iteration * 10,
                InventoryCommandKind::PrepareEquipmentPhysical {
                    operation: op.ticket.operation,
                    prepared: Box::new(wrong)
                }
            )
            .result
            .is_err()
        );
        let result = command(
            k,
            103 + iteration * 10,
            InventoryCommandKind::PrepareEquipmentPhysical {
                operation: op.ticket.operation,
                prepared: Box::new(prepared),
            },
        );
        let InventoryDecision::EquipmentPrepared(final_op) =
            result.result.expect("equipment physical companion")
        else {
            panic!("equipment physical companion");
        };
        let mut wrong = receipt(&final_op);
        wrong.revisions[0].1 += 1;
        assert!(
            command(k, 104 + iteration * 10, InventoryCommandKind::Commit(wrong))
                .result
                .is_err()
        );
        assert!(k.read_player_snapshot(b).is_err());
        assert_eq!(
            k.inventory_item(id).unwrap().revision,
            gear.entity.mutation_revision
        );
        assert!(matches!(
            command(
                k,
                105 + iteration * 10,
                InventoryCommandKind::Commit(receipt(&final_op))
            )
            .result,
            Ok(InventoryDecision::Committed(_))
        ));
        let after_snapshot = k.read_player_snapshot(b).unwrap();
        assert_eq!(
            after_snapshot.character().progression().revision(),
            revision + 1
        );
        assert_eq!(
            k.inventory_item(id).unwrap().revision,
            after.entity.mutation_revision
        );
        assert!(
            matches!(k.inventory_item(id).unwrap().place,ItemPlace::Contained{equipped,..}if equipped==location)
        );
        let expected = final_op
            .equipment
            .as_ref()
            .unwrap()
            .item_experience
            .iter()
            .find(|p| p.item == id)
            .unwrap()
            .after
            .as_ref();
        assert_eq!(
            after_snapshot
                .item_experience()
                .iter()
                .find(|p| p.item == id),
            expected,
            "carried metadata remains authoritative; placement controls eligibility"
        );
        let metadata = after_snapshot
            .item_experience()
            .iter()
            .find(|p| p.item == id)
            .expect("equipped and carried XP metadata is retained");
        assert_eq!(
            metadata.experience.unwrap().total,
            0,
            "dequip cannot erase or award item XP"
        );
        let reward = k.prepare_item_experience(b.actor, 1).unwrap();
        assert_eq!(
            reward.changes.len(),
            usize::from(location != 0),
            "only equipped items receive GrantItemXP"
        );
        if location != 0 {
            assert_eq!(reward.changes[0].0, id);
            assert_eq!(reward.changes[0].1.added, 1);
        }
        assert!(!k.has_inventory_command_state());
        gear = after;
    }
}
