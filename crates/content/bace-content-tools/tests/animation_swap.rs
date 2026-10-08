use bace_content::{AnimationSwapEditV1, AnimationSwapOperationV1, AnimationSwapPatchV1};
use bace_content_tools::{compile_animation_swap, decode_animation_swap, parse_animation_swap};

#[test]
fn animation_swap_schema_round_trips_and_rejects_invalid_targets() {
    let source = "schema_version = 1\nanimation_id = 50331649\n\n[[edits]]\noperation = 'insert'\nframe = 0\nhook_index = 0\ndirection = 1\npart_index = 258\nobject_id = 16777217\n";
    let parsed = parse_animation_swap(source).unwrap();
    assert_eq!(
        parsed.edits,
        vec![AnimationSwapEditV1 {
            operation: AnimationSwapOperationV1::Insert,
            frame: 0,
            hook_index: 0,
            direction: Some(1),
            part_index: Some(258),
            object_id: Some(0x01000001)
        }]
    );
    assert_eq!(
        decode_animation_swap(&compile_animation_swap(&parsed).unwrap()).unwrap(),
        parsed
    );
    let wrong = AnimationSwapPatchV1 {
        animation_id: 0x02000001,
        ..parsed.clone()
    };
    assert!(compile_animation_swap(&wrong).is_err());
    assert!(parse_animation_swap(&(source.to_owned() + "unexpected = 1\n")).is_err());
    let mut corrupt = compile_animation_swap(&parsed).unwrap();
    corrupt[55] ^= 1;
    assert!(decode_animation_swap(&corrupt).is_err());
}
