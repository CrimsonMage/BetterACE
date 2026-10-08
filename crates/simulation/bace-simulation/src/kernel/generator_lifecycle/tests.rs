use super::*;
fn identity(entity: u32) -> GeneratorIdentity {
    GeneratorIdentity {
        entity: EntityId(entity),
        incarnation: 1,
        content_revision: 1,
        random_identity: [1; 16],
    }
}
#[test]
fn full_outer_fifo_drains_nested_lifecycle_before_parent() {
    let mut kernel = Kernel::new(World::default(), 2).unwrap();
    let child = identity(20);
    // A prepared child transition is retained separately while the original
    // parent request occupies the entire outer queue's first slot.
    kernel.generators.lifecycle_frames.insert(
        child.entity,
        [GeneratorLifecycleEffect::DestroySelf(child)].into(),
    );
    let parent = GeneratorLifecycleEffect::DestroyMember {
        generator: identity(10),
        member: GeneratorSpawnMember {
            entity: child.entity,
            contribution: 1,
        },
        recursive: true,
        include_dead: true,
        from_unload: false,
    };
    kernel.generators.effects.push_back(parent.clone());
    kernel
        .generators
        .effects
        .push_back(GeneratorLifecycleEffect::DetachMember {
            generator: identity(30),
            entity: EntityId(31),
        });
    kernel.process_generator_lifecycle().unwrap();
    assert!(kernel.generators.lifecycle_frames.is_empty());
    assert_eq!(kernel.generators.effects.len(), 1);
    assert_eq!(
        kernel.generators.events.pop_front(),
        Some(GeneratorWorldEvent::Lifecycle(
            GeneratorLifecycleEffect::DestroySelf(child)
        ))
    );
    assert_eq!(
        kernel.generators.events.pop_front(),
        Some(GeneratorWorldEvent::Lifecycle(parent))
    );
    kernel.process_generator_lifecycle().unwrap();
    assert!(kernel.generators.effects.is_empty());
}
#[test]
fn full_event_lane_retains_nested_work_until_consumer_drains() {
    let mut kernel = Kernel::new(World::default(), 1).unwrap();
    let child = identity(20);
    kernel.generators.lifecycle_frames.insert(
        child.entity,
        [GeneratorLifecycleEffect::DestroySelf(child)].into(),
    );
    let parent = GeneratorLifecycleEffect::DestroyMember {
        generator: identity(10),
        member: GeneratorSpawnMember {
            entity: child.entity,
            contribution: 1,
        },
        recursive: true,
        include_dead: true,
        from_unload: false,
    };
    kernel.generators.effects.push_back(parent.clone());
    kernel.process_generator_lifecycle().unwrap();
    assert_eq!(kernel.generators.effects.front(), Some(&parent));
    assert_eq!(kernel.generators.events.len(), 1);
    kernel.generators.events.clear();
    kernel.process_generator_lifecycle().unwrap();
    assert!(kernel.generators.effects.is_empty());
    assert!(kernel.generators.lifecycle_frames.is_empty());
}
