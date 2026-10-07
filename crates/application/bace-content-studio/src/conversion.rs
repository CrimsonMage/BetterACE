use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const JSON_LIMIT: u64 = 16 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ConversionOptions {
    pub output_parent: PathBuf,
    pub mariadb_basedir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ConversionSummary {
    pub directory: PathBuf,
    pub weenies: usize,
}

type Error = Box<dyn std::error::Error>;

/// Converts one source, preserving existing output and removing partial results
/// on failure. Successful earlier files in a batch are independent exports.
pub fn convert_file(
    source: &Path,
    options: &ConversionOptions,
    cancel: &AtomicBool,
) -> Result<ConversionSummary, String> {
    convert(source, options, cancel).map_err(|error| {
        // Bound UI/channel diagnostics without cutting a UTF-8 code point.
        let mut message = error.to_string();
        if message.len() > 8192 {
            let mut end = 8192;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
            message.push('…');
        }
        message
    })
}

fn convert(
    source: &Path,
    options: &ConversionOptions,
    cancel: &AtomicBool,
) -> Result<ConversionSummary, Error> {
    cancelled(cancel)?;
    if !options.output_parent.is_dir() {
        return Err("Choose an existing output folder.".into());
    }
    if !source.is_file() {
        return Err("Input must be a regular JSON or SQL file.".into());
    }
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
    let mut manifest = toml::Table::new();
    manifest.insert("format_version".into(), 1.into());
    manifest.insert("source".into(), source.to_string_lossy().to_string().into());
    let templates = if extension.eq_ignore_ascii_case("json") {
        let mut bytes = Vec::new();
        File::open(source)?
            .take(JSON_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > JSON_LIMIT {
            return Err("JSON source exceeds the 16 MiB limit.".into());
        }
        manifest.insert(
            "source_sha256".into(),
            format!("{:x}", Sha256::digest(&bytes)).into(),
        );
        manifest.insert("source_format".into(), "legacy-weenie-json".into());
        vec![bace_import::import_weenie_json(std::str::from_utf8(
            &bytes,
        )?)?]
    } else if extension.eq_ignore_ascii_case("sql") {
        let staged = stage_sql(source, options)?;
        manifest.insert("source_sha256".into(), staged.manifest.source_sha256.into());
        manifest.insert("source_format".into(), "legacy-weenie-sql".into());
        manifest.insert(
            "source_release".into(),
            staged.manifest.source_release.into(),
        );
        manifest.insert("upstream_pin".into(), staged.manifest.upstream_pin.into());
        let mut counts = toml::Table::new();
        for (table, count) in staged.manifest.table_row_counts {
            counts.insert(table, i64::try_from(count)?.into());
        }
        manifest.insert("table_row_counts".into(), counts.into());
        staged.weenies
    } else {
        return Err("Unsupported file type; choose .json or .sql.".into());
    };
    cancelled(cancel)?;
    if templates.is_empty() {
        return Err("The source contains no weenies to convert.".into());
    }
    // A uniquely created directory cannot collide with or replace an earlier
    // export. TempDir owns cleanup until every output is written successfully.
    let directory = tempfile::Builder::new()
        .prefix("weenies-")
        .tempdir_in(&options.output_parent)?;
    let mut total = 0_usize;
    for template in &templates {
        cancelled(cancel)?;
        let text = bace_content_tools::export(template)?;
        total = total
            .checked_add(text.len())
            .ok_or("Output size overflow.")?;
        if total > OUTPUT_LIMIT {
            return Err("Converted source exceeds the 512 MiB output limit.".into());
        }
        write_new(
            &directory
                .path()
                .join(format!("{}.toml", template.weenie_id)),
            text.as_bytes(),
        )?;
    }
    manifest.insert(
        "weenie_count".into(),
        i64::try_from(templates.len())?.into(),
    );
    write_new(
        &directory.path().join("manifest.toml"),
        toml::to_string_pretty(&manifest)?.as_bytes(),
    )?;
    cancelled(cancel)?;
    Ok(ConversionSummary {
        directory: directory.keep(),
        weenies: templates.len(),
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = File::options().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn cancelled(cancel: &AtomicBool) -> Result<(), Error> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled; incomplete output removed.".into())
    } else {
        Ok(())
    }
}

fn stage_sql(
    source: &Path,
    options: &ConversionOptions,
) -> Result<bace_import::StagedWorld, Error> {
    if !cfg!(target_os = "linux") {
        return Err("SQL staging is currently supported by this tool on Linux only. JSON conversion is available on this platform.".into());
    }
    let basedir = options
        .mariadb_basedir
        .as_ref()
        .ok_or("Select a private MariaDB installation folder to convert SQL.")?
        .canonicalize()?;
    let binaries = bace_import::MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    };
    Ok(bace_import::import_staged_world(
        &mut bace_import::MariaDbStaging::new(binaries),
        source,
    )?)
}
