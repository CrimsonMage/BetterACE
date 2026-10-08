use bace_content::{LootGraphV1, RareProfileV1};
use bace_storage_codec::{CodecLimits, PackKey, PackLimits, PackManifest, PackRecord};
use std::path::Path;

pub fn parse_loot_graph(source: &str) -> Result<LootGraphV1, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("loot authoring limit".into());
    }
    let graph: LootGraphV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    graph.validate()?;
    Ok(graph)
}
pub fn parse_rare_profile(source: &str) -> Result<RareProfileV1, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("rare authoring limit".into());
    }
    let profile: RareProfileV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}
pub fn compile_loot_graph(graph: &LootGraphV1) -> Result<Vec<u8>, String> {
    graph.validate()?;
    bace_storage_codec::encode(20, 1, graph, CodecLimits::default()).map_err(|e| e.to_string())
}
pub fn decode_loot_graph(bytes: &[u8]) -> Result<LootGraphV1, String> {
    let graph: LootGraphV1 = bace_storage_codec::decode(bytes, 20, 1, CodecLimits::default())
        .map_err(|e| e.to_string())?;
    graph.validate()?;
    Ok(graph)
}
pub fn compile_rare_profile(profile: &RareProfileV1) -> Result<Vec<u8>, String> {
    profile.validate()?;
    bace_storage_codec::encode(21, 1, profile, CodecLimits::default()).map_err(|e| e.to_string())
}
pub fn decode_rare_profile(bytes: &[u8]) -> Result<RareProfileV1, String> {
    let profile: RareProfileV1 = bace_storage_codec::decode(bytes, 21, 1, CodecLimits::default())
        .map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}

/// Compile one aggregate supplement. Application publication must validate item
/// references against its accepted catalog and journal acceptance atomically.
pub fn build_loot_pack(
    graphs: &[LootGraphV1],
    rares: &[RareProfileV1],
    directory: &Path,
) -> Result<crate::PackBuild, String> {
    if graphs.len() + rares.len() == 0 || graphs.len() + rares.len() > 4096 {
        return Err("loot pack profile limit".into());
    }
    let mut records = Vec::with_capacity(graphs.len() + rares.len());
    let mut bytes = 0usize;
    for graph in graphs {
        let value = compile_loot_graph(graph)?;
        bytes = bytes
            .checked_add(value.len())
            .ok_or("loot pack byte overflow")?;
        if bytes > 64 * 1024 * 1024 {
            return Err("loot pack exceeds bounded offline input".into());
        }
        records.push(PackRecord {
            key: PackKey {
                namespace: 46,
                id: u64::from(graph.id),
            },
            schema: 1,
            value: Some(value),
        });
    }
    for rare in rares {
        let value = compile_rare_profile(rare)?;
        bytes = bytes
            .checked_add(value.len())
            .ok_or("loot pack byte overflow")?;
        if bytes > 64 * 1024 * 1024 {
            return Err("loot pack exceeds bounded offline input".into());
        }
        records.push(PackRecord {
            key: PackKey {
                namespace: 47,
                id: u64::from(rare.id),
            },
            schema: 1,
            value: Some(value),
        });
    }
    if bytes > 64 * 1024 * 1024 {
        return Err("loot pack exceeds bounded offline input".into());
    }
    records.sort_by_key(|r| r.key);
    let limits = PackLimits::default();
    let base = bace_storage_codec::compile_pack(directory, records.into_iter().map(Ok), limits)
        .map_err(|e| e.to_string())?;
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base: base.clone(),
        deltas: Vec::new(),
    };
    let manifest = bace_storage_codec::write_manifest(directory, &manifest, limits)
        .map_err(|e| e.to_string())?;
    Ok(crate::PackBuild {
        file: directory.join(base.file_name),
        manifest,
        records: base.record_count,
    })
}
