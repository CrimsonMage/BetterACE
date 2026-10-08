//! Native ClothingBase override records in the immutable aggregate.
use bace_content::ClothingPatchV1;
use bace_storage_codec::CodecLimits;

pub fn parse_clothing_patch(source: &str) -> Result<ClothingPatchV1, String> {
    if source.len() > 1024 * 1024 {
        return Err("ClothingBase source exceeds 1 MiB".into());
    }
    let patch: ClothingPatchV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    patch.validate().map_err(|e| e.to_string())?;
    Ok(patch)
}

pub fn compile_clothing_patch(patch: &ClothingPatchV1) -> Result<Vec<u8>, String> {
    patch.validate().map_err(|e| e.to_string())?;
    bace_storage_codec::encode(25, 1, patch, CodecLimits::default()).map_err(|e| e.to_string())
}

pub fn decode_clothing_patch(bytes: &[u8]) -> Result<ClothingPatchV1, String> {
    let patch: ClothingPatchV1 = bace_storage_codec::decode(bytes, 25, 1, CodecLimits::default())
        .map_err(|e| e.to_string())?;
    patch.validate().map_err(|e| e.to_string())?;
    Ok(patch)
}
