//! Pinned ACE Player.OnAppraisal and Monster_Awareness.WakeUp owner state.
//! Live Identify stays unsupported until the response profile, these ordered
//! emotes, and AlertFriendly have one retained completion path.
use super::*;

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
}
