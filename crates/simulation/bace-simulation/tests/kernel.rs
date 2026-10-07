use bace_geometry::Vec3;
use bace_motion::MotionIntent;
use bace_simulation::{Command, Kernel, synthetic_scenario};
use bace_types::EntityId;
use bace_world::World;

#[test]
fn repeated_trace_is_deterministic_on_same_build() {
    let mut a = synthetic_scenario(2, 3).unwrap();
    let mut b = synthetic_scenario(2, 3).unwrap();
    for _ in 0..120 {
        assert!(a.step().unwrap().is_empty());
        assert!(b.step().unwrap().is_empty());
        assert_eq!(
            a.world().states().collect::<Vec<_>>(),
            b.world().states().collect::<Vec<_>>()
        );
    }
}

#[test]
fn queue_is_bounded_and_invalid_actor_does_not_stop_tick() {
    let mut kernel = Kernel::new(World::default(), 1).unwrap();
    let command = || Command::Movement {
        actor: EntityId(99),
        epoch: 0,
        sequence: 0,
        intent: MotionIntent::new(Vec3::ZERO, false).unwrap(),
    };
    kernel.enqueue(command()).unwrap();
    assert!(kernel.enqueue(command()).is_err());
    assert_eq!(kernel.step().unwrap().len(), 1);
    assert_eq!(kernel.ticks(), 1);
}
