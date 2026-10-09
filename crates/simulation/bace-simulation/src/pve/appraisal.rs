//! Pinned ACE Player.OnAppraisal and Monster_Awareness.WakeUp owner state.
//! Live Identify stays unsupported until the response profile, these ordered
//! emotes, and AlertFriendly have one retained completion path.
use super::*;
use bace_gameplay_api::selection::TargetSelection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppraisalResponseKind {
    None,
    Empty,
    Object { success: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppraisalSelectionResult {
    pub next: TargetSelection,
    pub response: AppraisalResponseKind,
    pub consumed_draw: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppraisalSelectionError {
    InvalidTime,
    InvalidRandom,
}

/// The inputs read by pinned ACE `Player.Examine`. A pet still performs the
/// creature roll before its final guaranteed-success override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppraisalTargetCheck {
    Object {
        resist_item_appraisal: i32,
    },
    Creature {
        assess_skill: i32,
        deception_skill: i32,
        resist_item_appraisal: i32,
        kind: AppraisalCreatureKind,
        untrained_assess_creature: bool,
        focus: u32,
        self_attribute: u32,
        assess_creature_mod: bool,
        cloaked_admin_or_sentinel: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppraisalCreatureKind {
    Monster,
    Pet,
    Player {
        self_target: bool,
        attempting_deception: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppraisalRoll {
    pub success: bool,
    pub consumed_draw: bool,
}

/// Pinned `Player.Examine` and `SkillCheck.GetSkillChance` ordering. The
/// multiplication uses the source's default single-precision 0.03f factor
/// before `Math.Exp` receives a double. Valid creature skills and attributes
/// are small nonnegative values; wider arithmetic avoids ACE's unchecked
/// integer overflow on malformed extremes. No runtime owner calls this until
/// the complete Identify response and authored NPC wake can be retained together.
pub fn evaluate_appraisal_roll(
    target: AppraisalTargetCheck,
    draw: Option<f32>,
) -> Result<AppraisalRoll, AppraisalSelectionError> {
    let (mut success, consumed_draw, resist, pet) = match target {
        AppraisalTargetCheck::Object {
            resist_item_appraisal,
        } => {
            if draw.is_some() {
                return Err(AppraisalSelectionError::InvalidRandom);
            }
            (true, false, resist_item_appraisal, false)
        }
        AppraisalTargetCheck::Creature {
            assess_skill,
            deception_skill,
            resist_item_appraisal,
            kind,
            untrained_assess_creature,
            focus,
            self_attribute,
            assess_creature_mod,
            cloaked_admin_or_sentinel,
        } => {
            let draw = draw.ok_or(AppraisalSelectionError::InvalidRandom)?;
            if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
                return Err(AppraisalSelectionError::InvalidRandom);
            }
            let skill = if !matches!(kind, AppraisalCreatureKind::Player { .. })
                && assess_creature_mod
                && untrained_assess_creature
            {
                ((u64::from(focus) + u64::from(self_attribute)) / 2) as i64
            } else {
                i64::from(assess_skill)
            };
            let difference = skill - i64::from(deception_skill);
            let exponent = 0.03f32 * difference as f32;
            let mut chance = (1.0 - 1.0 / (1.0 + f64::from(exponent).exp())).clamp(0.0, 1.0);
            if deception_skill == 0
                || matches!(
                    kind,
                    AppraisalCreatureKind::Player {
                        self_target: true,
                        ..
                    } | AppraisalCreatureKind::Player {
                        attempting_deception: false,
                        ..
                    }
                )
                || cloaked_admin_or_sentinel
            {
                chance = 1.0;
            }
            (
                chance > f64::from(draw),
                true,
                resist_item_appraisal,
                matches!(kind, AppraisalCreatureKind::Pet),
            )
        }
    };
    if resist >= 999 {
        success = false;
    }
    if pet {
        success = true;
    }
    Ok(AppraisalRoll {
        success,
        consumed_draw,
    })
}

/// Pinned Player.HandleActionIdentifyObject state order. `known` is an
/// authoritative FindObject(Everywhere) result. The caller supplies a single
/// explicit draw only for a new recognized target; repeats consume none.
pub fn select_appraisal(
    selection: TargetSelection,
    target: EntityId,
    known: bool,
    now: f64,
    chance: f32,
    draw: Option<f32>,
) -> Result<AppraisalSelectionResult, AppraisalSelectionError> {
    use AppraisalSelectionError as E;
    if !now.is_finite()
        || now < 0.0
        || !selection.appraisal_requested_at.is_finite()
        || selection.appraisal_requested_at < 0.0
    {
        return Err(E::InvalidTime);
    }
    let mut next = selection;
    if target.0 == 0 {
        if draw.is_some() {
            return Err(E::InvalidRandom);
        }
        next.requested_appraisal = None;
        next.current_appraisal = None;
        return Ok(AppraisalSelectionResult {
            next,
            response: AppraisalResponseKind::None,
            consumed_draw: false,
        });
    }
    if !known {
        if draw.is_some() {
            return Err(E::InvalidRandom);
        }
        return Ok(AppraisalSelectionResult {
            next,
            response: AppraisalResponseKind::Empty,
            consumed_draw: false,
        });
    }
    if selection.requested_appraisal == Some(target) {
        if selection.current_appraisal == Some(target) {
            if draw.is_some() {
                return Err(E::InvalidRandom);
            }
            return Ok(AppraisalSelectionResult {
                next,
                response: AppraisalResponseKind::Object { success: true },
                consumed_draw: false,
            });
        }
        if now < selection.appraisal_requested_at + 5.0 {
            if draw.is_some() {
                return Err(E::InvalidRandom);
            }
            return Ok(AppraisalSelectionResult {
                next,
                response: AppraisalResponseKind::Object { success: false },
                consumed_draw: false,
            });
        }
    }
    let draw = draw.ok_or(E::InvalidRandom)?;
    if !draw.is_finite()
        || !(0.0..1.0).contains(&draw)
        || !chance.is_finite()
        || !(0.0..=1.0).contains(&chance)
    {
        return Err(E::InvalidRandom);
    }
    let success = chance > draw;
    next.requested_appraisal = Some(target);
    next.appraisal_requested_at = now;
    if success {
        next.current_appraisal = Some(target);
    }
    Ok(AppraisalSelectionResult {
        next,
        response: AppraisalResponseKind::Object { success },
        consumed_draw: true,
    })
}

/// Source WakeUp calls OnWakeUp (Scream=18), then OnNewEnemy (17).
/// This is an obligation for the authored NPC emote owner, not a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppraisalWake {
    pub creature: EntityId,
    pub examiner: EntityId,
    pub emote_categories: [u32; 2],
}

impl Population {
    /// Preview the source Idle + Tolerance.Appraise transition without changing
    /// combat or consuming an emote. A caller must reserve both authored emote
    /// effects and the response before adopting this preview.
    pub(crate) fn preview_appraisal_wake(
        &self,
        creature: EntityId,
        examiner: EntityId,
        world: &World,
    ) -> Result<Option<AppraisalWake>, PveError> {
        let npc = self.npcs.get(&creature).ok_or(PveError::MissingActor)?;
        let examiner_alive = world
            .combatant(examiner)
            .is_some_and(|c| c.profile().player && c.health() > 0);
        if !examiner_alive
            || world.actor_state(examiner).is_err()
            || world.actor_state(creature).is_err()
            || world.combatant(creature).is_none_or(|c| c.health() == 0)
        {
            return Err(PveError::MissingActor);
        }
        Ok(
            (!npc.awake && npc.tolerance & 2 != 0).then_some(AppraisalWake {
                creature,
                examiner,
                emote_categories: [18, 17],
            }),
        )
    }

    /// Adopt only the exact preflighted wake. Repeated appraisal of an awake
    /// creature cannot replay Scream/NewEnemy or replace its attack target.
    pub(crate) fn adopt_appraisal_wake(
        &mut self,
        expected: AppraisalWake,
        world: &World,
    ) -> Result<(), PveError> {
        if self.preview_appraisal_wake(expected.creature, expected.examiner, world)?
            != Some(expected)
        {
            return Err(PveError::InvalidReceipt);
        }
        let npc = self
            .npcs
            .get_mut(&expected.creature)
            .expect("previewed creature still in owner");
        npc.target = Some(expected.examiner);
        npc.awake = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Property, SparseProperties, WeenieV1};
    use bace_geometry::Aabb;
    use bace_physics::SyntheticScene;

    #[test]
    fn pinned_tolerance_bit_and_wake_order() {
        // ACE PropertyInt.Tolerance=67; Tolerance.Appraise=2. Source WakeUp
        // calls OnWakeUp/Scream before OnNewEnemy, on success or failure.
        let source = WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "appraisal_monster".into(),
            weenie_type: 10,
            last_modified: None,
            properties: SparseProperties {
                ints: vec![Property { id: 67, value: 2 }],
                ..Default::default()
            },
        };
        assert_eq!(ace::source_tolerance(&source), 2);
        let effect = AppraisalWake {
            creature: EntityId(10),
            examiner: EntityId(20),
            emote_categories: [18, 17],
        };
        assert_eq!(effect.emote_categories, [18, 17]);
        assert_eq!(source.properties.ints[0].id, 67);
    }

    #[test]
    fn pinned_examine_roll_and_override_order() {
        // Generated by oracle/appraisal_roll.py after hash and method-order
        // checks against pinned Player.cs and SkillCheck.cs. This is a source
        // extraction fixture, not output from executing the C# implementation.
        let fixture = include_str!("../../tests/fixtures/appraisal_roll.tsv");
        let mut cases = 0;
        for line in fixture.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 15, "{line}");
            let number = |index: usize| fields[index].parse::<i32>().unwrap();
            let truth = |index: usize| number(index) != 0;
            let target = match fields[1] {
                "object" => AppraisalTargetCheck::Object {
                    resist_item_appraisal: number(4),
                },
                kind => AppraisalTargetCheck::Creature {
                    assess_skill: number(2),
                    deception_skill: number(3),
                    resist_item_appraisal: number(4),
                    kind: match kind {
                        "monster" => AppraisalCreatureKind::Monster,
                        "pet" => AppraisalCreatureKind::Pet,
                        "player" => AppraisalCreatureKind::Player {
                            self_target: truth(10),
                            attempting_deception: truth(11),
                        },
                        _ => panic!("unknown fixture kind: {kind}"),
                    },
                    assess_creature_mod: truth(5),
                    untrained_assess_creature: truth(6),
                    focus: number(7).try_into().unwrap(),
                    self_attribute: number(8).try_into().unwrap(),
                    cloaked_admin_or_sentinel: truth(9),
                },
            };
            let draw = (fields[12] != "-").then(|| fields[12].parse().unwrap());
            assert_eq!(
                evaluate_appraisal_roll(target, draw),
                Ok(AppraisalRoll {
                    success: truth(13),
                    consumed_draw: truth(14),
                }),
                "{}",
                fields[0]
            );
            cases += 1;
        }
        assert_eq!(cases, 16);
        let object = AppraisalTargetCheck::Object {
            resist_item_appraisal: 0,
        };
        assert_eq!(
            evaluate_appraisal_roll(object, Some(0.5)),
            Err(AppraisalSelectionError::InvalidRandom)
        );
        let creature = AppraisalTargetCheck::Creature {
            assess_skill: 100,
            deception_skill: 100,
            resist_item_appraisal: 0,
            kind: AppraisalCreatureKind::Monster,
            assess_creature_mod: false,
            untrained_assess_creature: false,
            focus: 0,
            self_attribute: 0,
            cloaked_admin_or_sentinel: false,
        };
        for draw in [None, Some(f32::NAN), Some(1.0), Some(-0.1)] {
            assert_eq!(
                evaluate_appraisal_roll(creature, draw),
                Err(AppraisalSelectionError::InvalidRandom)
            );
        }
        let wide_attributes = AppraisalTargetCheck::Creature {
            assess_skill: 0,
            deception_skill: i32::MAX,
            resist_item_appraisal: 0,
            kind: AppraisalCreatureKind::Monster,
            untrained_assess_creature: true,
            focus: u32::MAX,
            self_attribute: u32::MAX,
            assess_creature_mod: true,
            cloaked_admin_or_sentinel: false,
        };
        assert!(
            evaluate_appraisal_roll(wide_attributes, Some(0.5))
                .unwrap()
                .success
        );
    }

    fn combat_profile(player: bool) -> CombatantProfile {
        CombatantProfile {
            maximum_health: 20,
            melee_damage: 2,
            melee_range: 1.,
            attack_duration: 0.5,
            strike_offsets: vec![0.25],
            player,
        }
    }

    fn actor(world: &mut World, id: EntityId, x: f32, player: bool) {
        let cell = CellId(0x12340001);
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(x, 10., 0.5),
            0.5,
            Capabilities {
                speed: 1.,
                jump_impulse: 0.,
            },
        )
        .unwrap();
        world.insert(Actor { id, cell, body }).unwrap();
        world
            .register_combatant(id, Combatant::new(combat_profile(player)).unwrap())
            .unwrap();
    }

    #[test]
    fn appraise_tolerant_idle_npc_wakes_once_and_keeps_examiner_target() {
        let mut world = World::default();
        let cell = CellId(0x12340001);
        world
            .register_scene(
                cell,
                SyntheticScene::new(
                    0.,
                    Aabb::new(Vec3::new(0., 0., -1.), Vec3::new(24., 24., 24.)).unwrap(),
                    vec![],
                )
                .unwrap(),
            )
            .unwrap();
        let npc = EntityId(10);
        let examiner = EntityId(20);
        let closer_player = EntityId(21);
        let mut population = Population::new(8);
        population
            .spawn(
                npc,
                NpcBlueprint {
                    cell,
                    position: Vec3::new(10., 10., 0.5),
                    radius: 0.5,
                    capabilities: Capabilities {
                        speed: 1.,
                        jump_impulse: 0.,
                    },
                    combat: combat_profile(false),
                    visual_range: 18.,
                    think_interval: 1,
                    corpse_template: 10,
                    xp_override: None,
                    loot: vec![],
                    death_animation_ticks: 1,
                    respawn_ticks: 1,
                    corpse_decay_ticks: 10,
                },
                &mut world,
                0,
            )
            .unwrap();
        actor(&mut world, examiner, 14., true);
        actor(&mut world, closer_player, 12., true);
        population.npcs.get_mut(&npc).unwrap().tolerance = 64;
        assert_eq!(
            population.preview_appraisal_wake(npc, examiner, &world),
            Ok(None)
        );
        population.npcs.get_mut(&npc).unwrap().tolerance = 2;
        assert_eq!(
            population.preview_appraisal_wake(npc, EntityId(999), &world),
            Err(PveError::MissingActor)
        );
        let mut combat = Combat::new(8);
        population.think(&mut world, &mut combat, 0, |_| false);
        assert_eq!(population.npcs[&npc].target, None);
        assert!(!population.npcs[&npc].awake);

        let wake = population
            .preview_appraisal_wake(npc, examiner, &world)
            .unwrap()
            .unwrap();
        assert_eq!(wake.emote_categories, [18, 17]);
        assert_eq!(population.npcs[&npc].target, None);
        population.adopt_appraisal_wake(wake, &world).unwrap();
        assert_eq!(population.npcs[&npc].target, Some(examiner));
        assert!(population.npcs[&npc].awake);
        assert_eq!(
            population.adopt_appraisal_wake(wake, &world),
            Err(PveError::InvalidReceipt)
        );
        assert_eq!(
            population.preview_appraisal_wake(npc, examiner, &world),
            Ok(None)
        );
        population.think(&mut world, &mut combat, 1, |_| false);
        assert_eq!(population.npcs[&npc].target, Some(examiner));
    }

    #[test]
    fn source_tolerance_preserves_each_authored_bit() {
        let mut source = WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "appraisal_monster".into(),
            weenie_type: 10,
            last_modified: None,
            properties: SparseProperties::default(),
        };
        assert_eq!(ace::source_tolerance(&source), 0);
        source.properties.ints.push(Property { id: 67, value: 64 });
        assert_eq!(ace::source_tolerance(&source), 64);
        source.properties.ints[0].value = 66;
        assert_eq!(ace::source_tolerance(&source), 66);
    }

    #[test]
    fn pinned_selection_repeats_success_and_throttles_failure_for_five_seconds() {
        let a = EntityId(20);
        let b = EntityId(21);
        let empty = TargetSelection::default();
        // A missing object replies empty and leaves the current selection.
        let missing = select_appraisal(empty, a, false, 100.0, 0.0, None).unwrap();
        assert_eq!(missing.response, AppraisalResponseKind::Empty);
        assert_eq!(missing.next, empty);
        let failed = select_appraisal(empty, a, true, 100.0, 0.2, Some(0.5)).unwrap();
        assert_eq!(
            failed.response,
            AppraisalResponseKind::Object { success: false }
        );
        assert!(failed.consumed_draw);
        let repeat = select_appraisal(failed.next, a, true, 104.999, 1.0, None).unwrap();
        assert_eq!(repeat.response, failed.response);
        assert!(!repeat.consumed_draw);
        let success = select_appraisal(failed.next, a, true, 105.0, 1.0, Some(0.9)).unwrap();
        assert_eq!(
            success.response,
            AppraisalResponseKind::Object { success: true }
        );
        assert_eq!(success.next.current_appraisal, Some(a));
        let repeated_success = select_appraisal(success.next, a, true, 200.0, 0.0, None).unwrap();
        assert_eq!(repeated_success.response, success.response);
        assert!(!repeated_success.consumed_draw);
        // New failed targets do not erase a previously successful target.
        let failed_b = select_appraisal(success.next, b, true, 201.0, 0.0, Some(0.1)).unwrap();
        assert_eq!(failed_b.next.current_appraisal, Some(a));
        let cleared =
            select_appraisal(failed_b.next, EntityId(0), false, 202.0, 0.0, None).unwrap();
        assert_eq!(cleared.response, AppraisalResponseKind::None);
        assert_eq!(cleared.next.current_appraisal, None);
        assert_eq!(cleared.next.requested_appraisal, None);
    }

    #[test]
    fn invalid_time_or_draw_never_produces_selection() {
        let target = EntityId(20);
        let state = TargetSelection::default();
        assert_eq!(
            select_appraisal(state, target, true, f64::NAN, 1.0, Some(0.5)),
            Err(AppraisalSelectionError::InvalidTime)
        );
        assert_eq!(
            select_appraisal(state, target, true, 10.0, 1.0, Some(1.0)),
            Err(AppraisalSelectionError::InvalidRandom)
        );
        assert_eq!(
            select_appraisal(state, target, true, 10.0, 1.0, None),
            Err(AppraisalSelectionError::InvalidRandom)
        );
        assert_eq!(
            select_appraisal(state, EntityId(0), false, 10.0, 0.0, Some(0.5)),
            Err(AppraisalSelectionError::InvalidRandom)
        );
    }
}
