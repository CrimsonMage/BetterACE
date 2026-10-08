use bace_content::{AnimationSwapOperationV1, AnimationSwapPatchV1};
use bace_dat::{Animation, AnimationHook, AnimationHookPayload};

fn replacement(direction: i32, raw_part_index: u16, object_id: u32) -> AnimationHook {
    AnimationHook {
        kind: 5,
        direction,
        payload: AnimationHookPayload::ReplaceObject {
            raw_part_index,
            part_index: raw_part_index as u8,
            object_id,
        },
    }
}

/// Apply ordered native edits to a clone. A failed edit never mutates the DAT
/// value supplied by the caller or returns a partially changed animation.
pub fn resolve_animation_swaps(
    original: &Animation,
    patch: &AnimationSwapPatchV1,
) -> Result<Animation, String> {
    patch.validate()?;
    if original.id != patch.animation_id {
        return Err("Animation swap patch targets another DAT animation".into());
    }
    let mut resolved = original.clone();
    for edit in &patch.edits {
        let frame_index = edit.frame as usize;
        let frame = resolved
            .frames
            .get_mut(frame_index)
            .ok_or("Animation swap frame is missing")?;
        match edit.operation {
            AnimationSwapOperationV1::Insert => {
                let index = edit.hook_index as usize;
                if index > frame.hooks.len() {
                    return Err("Animation swap insert position is missing".into());
                }
                frame.hooks.insert(
                    index,
                    replacement(
                        edit.direction.ok_or("Missing direction")?,
                        edit.part_index.ok_or("Missing part")?,
                        edit.object_id.ok_or("Missing object")?,
                    ),
                );
            }
            AnimationSwapOperationV1::Replace => {
                let hook = frame
                    .hooks
                    .get_mut(edit.hook_index as usize)
                    .ok_or("Animation swap hook is missing")?;
                if !matches!(hook.payload, AnimationHookPayload::ReplaceObject { .. }) {
                    return Err("Only ReplaceObject hooks can be replaced".into());
                }
                *hook = replacement(
                    edit.direction.ok_or("Missing direction")?,
                    edit.part_index.ok_or("Missing part")?,
                    edit.object_id.ok_or("Missing object")?,
                );
            }
            AnimationSwapOperationV1::Remove => {
                let index = edit.hook_index as usize;
                if !matches!(
                    frame.hooks.get(index).map(|hook| &hook.payload),
                    Some(AnimationHookPayload::ReplaceObject { .. })
                ) {
                    return Err("Only an existing ReplaceObject hook can be removed".into());
                }
                frame.hooks.remove(index);
            }
        }
    }
    Ok(resolved)
}
