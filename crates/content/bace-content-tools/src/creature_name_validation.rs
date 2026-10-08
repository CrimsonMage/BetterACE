//! Offline validation of the startup index against canonical accepted rows.
use bace_storage_codec::{MappedPack, PackKey, PackLookup};
use std::sync::Arc;
pub(super) fn validate(pack: &Arc<MappedPack>) -> Result<(), String> {
    let key = PackKey {
        namespace: 48,
        id: 1,
    };
    let index = match pack.lookup(key).map_err(|e| e.to_string())? {
        PackLookup::Record(record) if record.schema() == 1 => {
            Some(crate::decode_creature_names(record.bytes())?)
        }
        PackLookup::Missing => None,
        _ => return Err("creature name index cannot be a tombstone".into()),
    };
    let classes = match pack
        .lookup(PackKey {
            namespace: 49,
            id: 1,
        })
        .map_err(|e| e.to_string())?
    {
        PackLookup::Record(record) if record.schema() == 1 => {
            Some(crate::decode_template_classes(record.bytes())?)
        }
        PackLookup::Missing => None,
        _ => return Err("template class-name index cannot be a tombstone".into()),
    };
    let (index, classes) = match (index, classes) {
        (Some(index), Some(classes)) => (index, classes),
        (None, None) => return Ok(()), // Legacy packs require upgrade before creation/publication.
        _ => return Err("incomplete derived name-index set".into()),
    };
    let mut count = 0;
    let mut templates = 0;
    let mut cursor = None;
    while let Some((key, value)) = pack.scan(cursor, 1).map_err(|e| e.to_string())?.pop() {
        cursor = Some(key);
        if key.namespace > 1 {
            break;
        }
        if key.namespace != 1 {
            continue;
        }
        let PackLookup::Record(record) = value else {
            return Err("complete template source is a tombstone".into());
        };
        if record.schema() != 1 {
            return Err("unsupported name-index source schema".into());
        }
        let source = crate::decode(record.bytes()).map_err(|e| e.to_string())?;
        if u64::from(source.weenie_id) != key.id {
            return Err("name-index source identity mismatch".into());
        }
        let class = classes
            .entries
            .binary_search_by_key(&source.weenie_id, |entry| entry.template)
            .map_err(|_| "missing class-name index entry")?;
        if classes.entries[class].class_name != source.class_name {
            return Err("class-name index differs from source".into());
        }
        templates += 1;
        if source.weenie_type != 10 {
            continue;
        }
        if let Some(name) = source.properties.strings.iter().find(|p| p.id == 1) {
            let position = index
                .entries
                .binary_search_by_key(&source.weenie_id, |e| e.template)
                .map_err(|_| "creature name missing from index")?;
            if index.entries[position].name != name.value {
                return Err("creature name index differs from source".into());
            }
            count += 1;
        }
    }
    if count != index.entries.len() {
        return Err("extra creature name index entry".into());
    }
    if templates != classes.entries.len() {
        return Err("extra class-name index entry".into());
    }
    Ok(())
}
