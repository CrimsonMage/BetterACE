use super::{kernel_with_actors, npc};
use bace_gameplay_api::{ActionContext, CharacterBinding, SessionId};
use bace_simulation::Command;
use bace_types::AccountId;
const PLAYER: u32 = 0x50000001;
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(PLAYER),
    }
}
fn combat(sequence: u32, request: CombatRequest) -> Command {
    Command::Combat {
        context: ActionContext {
            session: SessionId(9),
            account: AccountId(4),
            actor: EntityId(PLAYER),
            sequence,
        },
        request,
    }
}
use bace_content::*;
use bace_gameplay_api::{CharacterRareState, CombatRequest, GeneratedItemMutation};
use bace_loot::{LootGraph, RareEvaluator};
use bace_random::RandomRoot;
use bace_simulation::{NativeLootPolicy, OwnedCharacterState};
use bace_types::EntityId;
use std::sync::Arc;
fn evidence() -> RareEvidence {
    RareEvidence {
        classification: UncertainEvidence::OperatorAssumption,
        note: "Synthetic all-success test, not a retail rate".into(),
        reference: None,
    }
}
fn chance() -> RareProbabilityV1 {
    RareProbabilityV1 {
        value: ProbabilityV1 {
            numerator: 1,
            denominator: 1,
        },
        evidence: evidence(),
    }
}
fn policy() -> NativeLootPolicy {
    let graph = LootGraphV1 {
        schema_version: 1,
        id: 7,
        root: "item".into(),
        nodes: vec![
            LootNodeV1 {
                id: "item".into(),
                selection: LootSelectionV1::Item,
                item: Some(LootItemV1 {
                    template: 42,
                    minimum_stack: 1,
                    maximum_stack: 1,
                }),
                mutation: None,
                branches: vec![LootBranchV1 {
                    id: "value".into(),
                    target: "mutate".into(),
                    minimum_rolls: 1,
                    maximum_rolls: 1,
                    weight: None,
                    probability: None,
                }],
            },
            LootNodeV1 {
                id: "mutate".into(),
                selection: LootSelectionV1::Mutation,
                item: None,
                mutation: Some(LootMutationV1::Int {
                    property: 19,
                    minimum: 123,
                    maximum: 123,
                }),
                branches: vec![],
            },
        ],
    };
    let rare = RareProfileV1 {
        schema_version: 1,
        id: 8,
        enabled: true,
        standard: Some(chance()),
        realtime: Some(RealtimeRarePolicyV1 {
            minimum_seconds: 10,
            maximum_seconds: 10,
            distribution: RareIntervalDistribution::UniformSeconds,
            initialization: RareTimerInitialization::FirstEligibleKill,
            reset: RareTimerReset::RealtimeSuccess,
            bonus_chance: chance(),
            timing_evidence: evidence(),
        }),
        tiers: (1..=6)
            .map(|tier| RareTierV1 {
                tier,
                weight: 1,
                evidence: evidence(),
                items: vec![WeightedRareItemV1 {
                    template: 100 + u32::from(tier),
                    weight: 1,
                }],
            })
            .collect(),
    };
    NativeLootPolicy {
        graph: Arc::new(LootGraph::prepare(graph).unwrap()),
        graph_revision: [7; 32],
        content_generation: [9; 32],
        rare: Some(Arc::new(RareEvaluator::prepare(rare).unwrap())),
        rare_profile_revision: Some([8; 32]),
        creature_level: 100,
    }
}
#[test]
fn native_death_freezes_mutations_and_rares_until_exact_corpse_receipt() {
    let mut kernel =
        kernel_with_actors(16, 2, 16, [PLAYER, 0x50000002], Some(200), false, binding());
    let progression = kernel.take_character(binding()).unwrap();
    let original = CharacterRareState {
        character: PLAYER,
        random_identity: [17; 16],
        key_version: 1,
        attempt_ordinal: 0,
        timer_ordinal: 0,
        next_realtime_at: None,
        last_effective_time: 0,
    };
    kernel
        .register_character_with_rare(
            binding(),
            OwnedCharacterState {
                native_services: None,
                contracts: None,
                progression,
                rares: Some(original),
                ui: None,
            },
        )
        .unwrap();
    assert!(kernel.take_character(binding()).is_err());
    kernel
        .configure_native_loot(Arc::new(RandomRoot::new([37; 32], 1).unwrap()), 1000)
        .unwrap();
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel
        .register_native_loot(EntityId(3), policy(), [2; 16])
        .unwrap();
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel
        .enqueue(combat(
            2,
            CombatRequest::TargetedMelee {
                target: EntityId(3),
                height: 2,
                power: 0.5,
            },
        ))
        .unwrap();
    for _ in 0..20 {
        kernel.step().unwrap();
    }
    let proposal = kernel
        .take_death_proposal()
        .expect("death must not require the legacy shared draw queue");
    let native = proposal.native.as_ref().unwrap();
    assert_eq!(native.event_id, [2; 16]);
    assert_eq!(native.graph_id, 7);
    assert_eq!(native.graph_revision, [7; 32]);
    assert_eq!(native.rare_profile_revision, Some([8; 32]));
    assert_eq!(native.content_generation, [9; 32]);
    assert_eq!(native.generated.len(), 2);
    assert_eq!(
        native.generated[0].mutations,
        vec![GeneratedItemMutation::Int(19, 123)]
    );
    let decision = native.rare.as_ref().unwrap();
    assert!(decision.standard_success);
    assert!(!decision.realtime_success);
    assert_eq!(decision.character, PLAYER);
    assert_eq!(decision.next.attempt_ordinal, 1);
    assert_eq!(
        kernel.character_rare_state(EntityId(PLAYER)),
        Some(original)
    );
    kernel.retry_death(proposal.operation).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.take_death_proposal().unwrap(), proposal);
    let mut wrong = proposal.experience_state.clone();
    wrong[0].1.after_available += 1;
    assert!(
        kernel
            .confirm_death_committed(
                proposal.operation,
                EntityId(1000),
                &[EntityId(1001), EntityId(1002)],
                &wrong
            )
            .is_err()
    );
    assert_eq!(
        kernel.character_rare_state(EntityId(PLAYER)),
        Some(original)
    );
    assert!(kernel.take_character_with_rare(binding()).is_err());
    kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001), EntityId(1002)],
            &proposal.experience_state,
        )
        .unwrap();
    assert_eq!(
        kernel.character_rare_state(EntityId(PLAYER)),
        Some(decision.next)
    );
    assert_eq!(
        kernel.world().corpse(EntityId(1000)).unwrap().items,
        vec![EntityId(1001), EntityId(1002)]
    );
    assert!(
        kernel
            .confirm_death_committed(
                proposal.operation,
                EntityId(1000),
                &[EntityId(1001), EntityId(1002)],
                &proposal.experience_state
            )
            .is_err()
    );
}

#[test]
fn rare_owner_is_top_damage_player_even_when_another_player_lands_final_blow() {
    let mut kernel =
        kernel_with_actors(32, 4, 32, [PLAYER, PLAYER + 1], Some(200), true, binding());
    let progression = kernel.take_character(binding()).unwrap();
    let state = |id| CharacterRareState {
        character: id,
        random_identity: (u128::from(id)).to_le_bytes(),
        key_version: 1,
        attempt_ordinal: 0,
        timer_ordinal: 0,
        next_realtime_at: None,
        last_effective_time: 0,
    };
    kernel
        .register_character_with_rare(
            binding(),
            OwnedCharacterState {
                native_services: None,
                contracts: None,
                progression,
                rares: Some(state(PLAYER)),
                ui: None,
            },
        )
        .unwrap();
    let second = CharacterBinding {
        session: SessionId(10),
        account: AccountId(5),
        actor: EntityId(PLAYER + 1),
    };
    kernel
        .register_character_with_rare(
            second,
            OwnedCharacterState {
                native_services: None,
                contracts: None,
                progression: super::character(),
                rares: Some(state(PLAYER + 1)),
                ui: None,
            },
        )
        .unwrap();
    kernel
        .configure_native_loot(Arc::new(RandomRoot::new([37; 32], 1).unwrap()), 1000)
        .unwrap();
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel
        .register_native_loot(EntityId(3), policy(), [3; 16])
        .unwrap();
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel
        .enqueue(combat(
            2,
            CombatRequest::TargetedMelee {
                target: EntityId(3),
                height: 2,
                power: 0.25,
            },
        ))
        .unwrap();
    for _ in 0..6 {
        kernel.step().unwrap();
    }
    assert_eq!(kernel.world().combatant(EntityId(3)).unwrap().health(), 5);
    for (sequence, request) in [
        (1, CombatRequest::ChangeMode(2)),
        (
            2,
            CombatRequest::TargetedMelee {
                target: EntityId(3),
                height: 2,
                power: 0.0,
            },
        ),
    ] {
        kernel
            .enqueue(Command::Combat {
                context: ActionContext {
                    session: second.session,
                    account: second.account,
                    actor: second.actor,
                    sequence,
                },
                request,
            })
            .unwrap();
    }
    for _ in 0..8 {
        kernel.step().unwrap();
    }
    let mut last_blow = false;
    while let Some(event) = kernel.take_combat_event() {
        if matches!(event,bace_simulation::CombatEvent::Damage{attacker,target,killed:true,..} if attacker==Some(second.actor) && target==EntityId(3))
        {
            last_blow = true;
        }
    }
    assert!(last_blow);
    let proposal = kernel.take_death_proposal().unwrap();
    assert_eq!(proposal.owner, Some(EntityId(PLAYER)));
    assert_eq!(
        proposal.experience,
        vec![(EntityId(PLAYER), 75), (second.actor, 25)]
    );
    let native = proposal.native.as_ref().unwrap();
    assert_eq!(native.rare.as_ref().unwrap().character, PLAYER);
    assert_eq!(
        native
            .generated
            .iter()
            .filter(|d| d.node == "$rare")
            .count(),
        1
    );
    assert_eq!(
        kernel.character_rare_state(second.actor),
        Some(state(PLAYER + 1))
    );
    kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001), EntityId(1002)],
            &proposal.experience_state,
        )
        .unwrap();
    assert_eq!(
        kernel.character_rare_state(second.actor),
        Some(state(PLAYER + 1))
    );
    assert_eq!(
        kernel
            .character_rare_state(EntityId(PLAYER))
            .unwrap()
            .attempt_ordinal,
        1
    );
}
