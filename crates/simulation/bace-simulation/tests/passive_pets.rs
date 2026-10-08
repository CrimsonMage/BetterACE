#[allow(dead_code, unused_imports)]
mod magic_common;
use bace_ai::{PetUseRequirements, PetUser};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_physics::{CollisionShape, CollisionSphere};
use bace_simulation::{
    InventoryReceipt, PetAction, PetCommand, PetDecision, PetEvent, PreparedPassivePet,
};
use magic_common::*;

fn receipt(ticket: &bace_simulation::InventoryTicket) -> InventoryReceipt {
    InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    }
}

fn setup() -> (Kernel, Arc<PreparedPassivePet>, PetUser) {
    let mut kernel = kernel_fixture(64, false, true);
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: Some(EntityId(1)),
            slots: 10,
            pack_slots: 1,
            burden_limit: 1000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    kernel
        .register_inventory_item(InventoryItem {
            id: EntityId(10),
            revision: 1,
            template: 500,
            stack_key: 1,
            place: ItemPlace::Contained {
                container: EntityId(1),
                slot: 0,
                equipped: 0,
            },
            stack: 1,
            structure: Some(3),
            maximum_stack: 1,
            unit_burden: 1,
            unit_value: 1,
            pack_slot: false,
            is_container: false,
            attuned: false,
            trade_reserved: false,
            active_pet: false,
            unique: false,
            quest_allowed: true,
            valid_wield: 0,
            incompatible_wield: 0,
            wield_requirements_met: true,
        })
        .unwrap();
    let user = PetUser {
        portal_space: false,
        owns_device: true,
        advancement: 2,
        skill: 0,
        level: 50,
        mastery: 0,
        charges: 3,
        active_combat_pet: false,
        cooldown_active: false,
    };
    kernel.register_pet_owner(EntityId(1), user).unwrap();
    let profile = Arc::new(PreparedPassivePet {
        template: 900,
        shape: Arc::new(
            CollisionShape::prepare(
                vec![CollisionSphere {
                    center: Vec3::new(0.0, 0.0, 0.2),
                    radius: 0.2,
                }],
                0.0,
                0.1,
            )
            .unwrap(),
        ),
        capabilities: Capabilities {
            speed: 5.0,
            jump_impulse: 0.0,
        },
        requirements: PetUseRequirements {
            skill_required: false,
            skill_level: 0,
            level: 0,
            mastery: 0,
            unlimited: false,
        },
    });
    (kernel, profile, user)
}

#[test]
fn passive_pet_receipt_and_toggle_stow_have_no_combat_owner() {
    let (mut kernel, profile, user) = setup();
    let pet = EntityId(0x8000_0050);
    kernel
        .enqueue(Command::Pet(PetCommand {
            correlation: 70,
            action: PetAction::SummonPassive {
                context: context(1),
                device: EntityId(10),
                device_revision: 1,
                pet,
                user,
                activation: None,
                profile: profile.clone(),
            },
        }))
        .unwrap();
    step(&mut kernel);
    let proposed = kernel.take_pet_outcome().unwrap();
    let ticket = match proposed.result {
        Ok(PetDecision::Proposed { ticket, .. }) => ticket,
        other => panic!("expected passive proposal: {other:?}"),
    };
    assert!(!kernel.world().contains_identity(pet));
    assert_eq!(ticket.proposal.changes[0].after.structure, Some(2));
    kernel
        .enqueue(Command::Pet(PetCommand {
            correlation: 71,
            action: PetAction::Resolve {
                receipt: receipt(&ticket),
                committed: true,
            },
        }))
        .unwrap();
    step(&mut kernel);
    assert!(matches!(
        kernel.take_pet_outcome().unwrap().result,
        Ok(PetDecision::Resolved {
            committed: true,
            ..
        })
    ));
    assert!(
        matches!(kernel.take_pet_event(), Some(PetEvent::Spawned { pet: id, .. }) if id == pet)
    );
    assert!(kernel.world().combatant(pet).is_none());
    assert_eq!(kernel.pet_owner(pet), Some(EntityId(1)));

    for _ in 0..5 {
        step(&mut kernel);
    }
    assert!(kernel.world().contains_identity(pet));

    kernel
        .enqueue(Command::Pet(PetCommand {
            correlation: 72,
            action: PetAction::SummonPassive {
                context: context(2),
                device: EntityId(10),
                device_revision: 2,
                pet: EntityId(0x8000_0051),
                user,
                activation: None,
                profile,
            },
        }))
        .unwrap();
    step(&mut kernel);
    let operation = match kernel.take_pet_outcome().unwrap().result {
        Ok(PetDecision::Stowing {
            operation: Some(operation),
        }) => operation,
        other => panic!("same device must stow active passive pet: {other:?}"),
    };
    let release = match kernel.take_pet_event() {
        Some(PetEvent::ReleaseProposed { ticket, .. }) => ticket,
        other => panic!("stow requires exact release: {other:?}"),
    };
    assert_eq!(release.operation, operation);
    assert!(kernel.world().contains_identity(pet));
    assert_eq!(release.proposal.changes[0].after.structure, Some(2));
    kernel
        .confirm_inventory_committed(&receipt(&release))
        .unwrap();
    assert!(!kernel.world().contains_identity(pet));
    assert!(
        matches!(kernel.take_pet_event(), Some(PetEvent::Despawned { pet: id, .. }) if id == pet)
    );
}
