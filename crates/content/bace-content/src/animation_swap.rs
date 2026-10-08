//! Native edits to DAT ReplaceObject hooks. The DAT remains immutable.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationSwapPatchV1 {
    pub schema_version: u16,
    pub animation_id: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub edits: Vec<AnimationSwapEditV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationSwapOperationV1 {
    Insert,
    Replace,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationSwapEditV1 {
    pub operation: AnimationSwapOperationV1,
    pub frame: u32,
    pub hook_index: u32,
    #[serde(default)]
    pub direction: Option<i32>,
    #[serde(default)]
    pub part_index: Option<u16>,
    #[serde(default)]
    pub object_id: Option<u32>,
}

impl AnimationSwapPatchV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.animation_id >> 24 != 3
            || self.animation_id & 0x00ff_ffff == 0
        {
            return Err("Invalid animation swap schema or animation DID".into());
        }
        if self.edits.is_empty() || self.edits.len() > 1024 {
            return Err("Animation swap patch must have 1–1024 edits".into());
        }
        for edit in &self.edits {
            if edit.frame > 65_535
                || edit.hook_index > 65_535
                || edit
                    .object_id
                    .is_some_and(|id| id >> 24 != 1 || id & 0x00ff_ffff == 0)
            {
                return Err("Invalid animation swap frame, hook or object DID".into());
            }
            match edit.operation {
                AnimationSwapOperationV1::Insert | AnimationSwapOperationV1::Replace => {
                    if edit.direction.is_none()
                        || edit.part_index.is_none()
                        || edit.object_id.is_none()
                    {
                        return Err(
                            "Animation swap insert/replace needs direction, part and object".into(),
                        );
                    }
                }
                AnimationSwapOperationV1::Remove => {
                    if edit.direction.is_some()
                        || edit.part_index.is_some()
                        || edit.object_id.is_some()
                    {
                        return Err(
                            "Animation swap removal has unexpected replacement fields".into()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
