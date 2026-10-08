use bace_content::AnimationSwapPatchV1;
use bace_storage_codec::CodecLimits;

pub fn parse_animation_swap(source: &str) -> Result<AnimationSwapPatchV1, String> {
    if source.len() > 1024 * 1024 {
        return Err("Animation swap source exceeds 1 MiB".into());
    }
    let patch: AnimationSwapPatchV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    patch.validate()?;
    Ok(patch)
}

pub fn compile_animation_swap(patch: &AnimationSwapPatchV1) -> Result<Vec<u8>, String> {
    patch.validate()?;
    bace_storage_codec::encode(26, 1, patch, CodecLimits::default()).map_err(|e| e.to_string())
}

pub fn decode_animation_swap(bytes: &[u8]) -> Result<AnimationSwapPatchV1, String> {
    let patch: AnimationSwapPatchV1 =
        bace_storage_codec::decode(bytes, 26, 1, CodecLimits::default())
            .map_err(|e| e.to_string())?;
    patch.validate()?;
    Ok(patch)
}
