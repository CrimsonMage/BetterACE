use bace_asset_overlay::resolve_animation_swaps;
use bace_content::{AnimationSwapEditV1, AnimationSwapOperationV1, AnimationSwapPatchV1};
use bace_dat::{Animation, AnimationFrame, AnimationHook, AnimationHookPayload};

fn original() -> Animation {
    Animation {
        id: 0x03000001,
        flags: 0,
        num_parts: 1,
        num_frames: 1,
        position_frames: Vec::new(),
        frames: vec![AnimationFrame {
            parts: Vec::new(),
            hooks: vec![AnimationHook {
                kind: 4,
                direction: 0,
                payload: AnimationHookPayload::Empty,
            }],
        }],
    }
}

#[test]
fn ordered_insert_replace_remove_preserves_dat_and_other_hooks() {
    let base = original();
    let patch = AnimationSwapPatchV1 {
        schema_version: 1,
        animation_id: base.id,
        edits: vec![
            AnimationSwapEditV1 {
                operation: AnimationSwapOperationV1::Insert,
                frame: 0,
                hook_index: 0,
                direction: Some(1),
                part_index: Some(258),
                object_id: Some(0x01000001),
            },
            AnimationSwapEditV1 {
                operation: AnimationSwapOperationV1::Replace,
                frame: 0,
                hook_index: 0,
                direction: Some(-1),
                part_index: Some(3),
                object_id: Some(0x01000002),
            },
        ],
    };
    let resolved = resolve_animation_swaps(&base, &patch).unwrap();
    assert_eq!(base.frames[0].hooks.len(), 1);
    assert_eq!(resolved.frames[0].hooks.len(), 2);
    assert_eq!(resolved.frames[0].hooks[1], base.frames[0].hooks[0]);
    assert!(matches!(
        resolved.frames[0].hooks[0].payload,
        AnimationHookPayload::ReplaceObject {
            raw_part_index: 3,
            part_index: 3,
            object_id: 0x01000002
        }
    ));
    let removed = AnimationSwapPatchV1 {
        schema_version: 1,
        animation_id: base.id,
        edits: vec![
            patch.edits[0].clone(),
            AnimationSwapEditV1 {
                operation: AnimationSwapOperationV1::Remove,
                frame: 0,
                hook_index: 0,
                direction: None,
                part_index: None,
                object_id: None,
            },
        ],
    };
    assert_eq!(resolve_animation_swaps(&base, &removed).unwrap(), base);
    let invalid = AnimationSwapPatchV1 {
        schema_version: 1,
        animation_id: base.id,
        edits: vec![AnimationSwapEditV1 {
            operation: AnimationSwapOperationV1::Remove,
            frame: 0,
            hook_index: 0,
            direction: None,
            part_index: None,
            object_id: None,
        }],
    };
    assert!(resolve_animation_swaps(&base, &invalid).is_err());
    assert_eq!(base, original());
}
