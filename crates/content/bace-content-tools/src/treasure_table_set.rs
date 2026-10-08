use bace_content::TreasureTableSetV1;
use bace_storage_codec::CodecLimits;

const LIMIT: CodecLimits = CodecLimits {
    max_payload_bytes: 16 * 1024 * 1024,
};

pub fn parse_treasure_table_set(source: &str) -> Result<TreasureTableSetV1, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("treasure TOML source limit".into());
    }
    let value: TreasureTableSetV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    value.validate()?;
    Ok(value)
}

pub fn compile_treasure_table_set(value: &TreasureTableSetV1) -> Result<Vec<u8>, String> {
    value.validate()?;
    bace_storage_codec::encode(27, 1, value, LIMIT).map_err(|e| e.to_string())
}

pub fn decode_treasure_table_set(bytes: &[u8]) -> Result<TreasureTableSetV1, String> {
    let value: TreasureTableSetV1 =
        bace_storage_codec::decode(bytes, 27, 1, LIMIT).map_err(|e| e.to_string())?;
    value.validate()?;
    Ok(value)
}
