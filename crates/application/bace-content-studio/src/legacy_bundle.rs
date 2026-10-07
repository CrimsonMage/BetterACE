use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum Format {
    Json,
    Sql,
}

/// Every compatibility export includes the complete native document. Metadata
/// absent from a legacy format is retained there and listed in export notes.
pub(crate) fn export(
    text: &str,
    parent: &Path,
    format: Format,
) -> Result<(PathBuf, String), String> {
    let mut template = bace_content_tools::parse(text).map_err(|e| e.to_string())?;
    let native = bace_content_tools::export(&template).map_err(|e| e.to_string())?;
    let mut notes = Vec::new();
    if template.properties.authoring_metadata.take().is_some() {
        notes.push("Authoring metadata is retained in native.toml only.");
    }
    if matches!(format, Format::Json) && template.last_modified.take().is_some() {
        notes.push("Last-modified metadata is retained in native.toml only.");
    }
    let mut group_metadata = false;
    let mut order_metadata = false;
    for emote in &mut template.properties.emotes {
        group_metadata |= emote.legacy_category_key.take().is_some();
        if matches!(format, Format::Json) {
            for action in &mut emote.actions {
                order_metadata |= action.legacy_order.take().is_some();
            }
        }
    }
    for generator in &mut template.properties.generators {
        group_metadata |= generator.legacy_slot.take().is_some();
    }
    if matches!(format, Format::Json) {
        for page in &mut template.properties.book_pages {
            order_metadata |= page.legacy_page_id.take().is_some();
        }
    }
    if group_metadata {
        notes.push("Legacy group/slot identifiers are retained in native.toml only; list ordering is preserved.");
    }
    if order_metadata {
        notes.push("Original SQL page/action sequence identifiers are retained in native.toml only; JSON array ordering is preserved.");
    }
    if matches!(format, Format::Sql) {
        notes.push("SQL assigns destination storage row IDs and fills missing page/action order IDs. Missing last-modified timestamps use the destination default. Existing weenie IDs cause INSERT errors; this export never replaces a database record automatically.");
    }
    let (extension, legacy) = match format {
        Format::Json => ("json", bace_import::export_weenie_json(&template)),
        Format::Sql => ("sql", bace_import::export_weenie_sql(&template)),
    };
    let legacy = legacy.map_err(|e| e.to_string())?;
    let directory = tempfile::Builder::new()
        .prefix("legacy-export-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    crate::document::write(&directory.path().join("native.toml"), &native, None)?;
    crate::document::write(
        &directory
            .path()
            .join(format!("{}.{}", template.weenie_id, extension)),
        &legacy,
        None,
    )?;
    let notes = if notes.is_empty() {
        "All authored fields are representable in this legacy export.".into()
    } else {
        notes.join("\n")
    };
    crate::document::write(&directory.path().join("export-notes.txt"), &notes, None)?;
    Ok((directory.keep(), notes))
}
