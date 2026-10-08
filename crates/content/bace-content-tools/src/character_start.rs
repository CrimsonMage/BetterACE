//! Native authoring/compiler boundary; runtime consumes only frozen binary bytes.
use bace_content::CharacterStartProfileV1;
use bace_storage_codec::CodecLimits;
const LIMITS: CodecLimits = CodecLimits {
    max_payload_bytes: 1024 * 1024,
};
pub fn parse_character_start(source: &str) -> Result<CharacterStartProfileV1, String> {
    if source.len() > LIMITS.max_payload_bytes {
        return Err("character-start authoring limit".into());
    }
    let profile: CharacterStartProfileV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}
pub fn compile_character_start(profile: &CharacterStartProfileV1) -> Result<Vec<u8>, String> {
    profile.validate()?;
    bace_storage_codec::encode(22, 1, profile, LIMITS).map_err(|e| e.to_string())
}
pub fn decode_character_start(bytes: &[u8]) -> Result<CharacterStartProfileV1, String> {
    let profile: CharacterStartProfileV1 =
        bace_storage_codec::decode(bytes, 22, 1, LIMITS).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}
pub fn compile_creature_names(
    index: &bace_content::CreatureNameIndexV1,
) -> Result<Vec<u8>, String> {
    index.validate()?;
    bace_storage_codec::encode(
        23,
        1,
        index,
        CodecLimits {
            max_payload_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())
}
pub fn decode_creature_names(bytes: &[u8]) -> Result<bace_content::CreatureNameIndexV1, String> {
    let index: bace_content::CreatureNameIndexV1 = bace_storage_codec::decode(
        bytes,
        23,
        1,
        CodecLimits {
            max_payload_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())?;
    index.validate()?;
    Ok(index)
}
pub fn compile_template_classes(
    index: &bace_content::TemplateClassIndexV1,
) -> Result<Vec<u8>, String> {
    index.validate()?;
    bace_storage_codec::encode(
        24,
        1,
        index,
        CodecLimits {
            max_payload_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())
}
pub fn decode_template_classes(bytes: &[u8]) -> Result<bace_content::TemplateClassIndexV1, String> {
    let index: bace_content::TemplateClassIndexV1 = bace_storage_codec::decode(
        bytes,
        24,
        1,
        CodecLimits {
            max_payload_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())?;
    index.validate()?;
    Ok(index)
}
