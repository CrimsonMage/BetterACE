use bace_gameplay_api::*;
use bace_types::EntityId;
pub fn clock(tick: u64) -> GeneratorClock {
    GeneratorClock {
        tick,
        unix_seconds: 1000 + tick as i64 / 30,
        is_day: true,
        event: GeneratorEventState::Missing,
    }
}
pub fn profile(id: u32) -> GeneratorProfile {
    GeneratorProfile {
        id,
        probability: -1.,
        weenie_class_id: 10,
        delay: None,
        init_create: 1,
        max_create: 1,
        when_create: 1,
        where_create: 0,
        stack_size: None,
        palette_id: None,
        shade: None,
        position: Default::default(),
    }
}
pub fn definition() -> GeneratorDefinition {
    GeneratorDefinition {
        identity: GeneratorIdentity {
            entity: EntityId(1),
            incarnation: 1,
            content_revision: 1,
            random_identity: [1; 16],
        },
        profiles: vec![profile(0)],
        location: GeneratorLocation {
            cell: 0x01010001,
            origin: [10., 20., 30.],
            rotation: [0., 0., 0., 1.],
        },
        kind: GeneratorKind::Object,
        initial_count: 1,
        maximum_count: 1,
        regeneration_interval: 1.,
        initial_delay: 0.,
        regeneration_timestamp: 0.,
        time_type: GeneratorTimeType::Undefined,
        event: None,
        start_time: 0,
        end_time: 0,
        disabled: false,
        automatic_destruction: false,
        parent: None,
        destruction: GeneratorDestruction::Destroy,
        end_destruction: GeneratorDestruction::Destroy,
        rotation_type: GeneratorRotationType::Undefined,
        use_rotation_offset: true,
        radius: 0.,
        vendor_shop_uses_generator: false,
    }
}
