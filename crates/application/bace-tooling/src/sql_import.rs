use bace_import::{MariaDbBinaries, MariaDbStaging, StagedWorld};
use std::path::Path;

pub fn stage(
    path: &Path,
    basedir: Option<&Path>,
) -> Result<StagedWorld, Box<dyn std::error::Error>> {
    let basedir = basedir
        .ok_or(
            "--mariadb-basedir or BACE_MARIADB_BASEDIR must identify a private MariaDB installation",
        )?
        .canonicalize()?;
    let binaries = MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    };
    let mut backend = MariaDbStaging::new(binaries);
    Ok(bace_import::import_staged_world(&mut backend, path)?)
}

pub fn export(staged: StagedWorld, directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if directory.exists() {
        return Err("output directory already exists; choose a new destination".into());
    }
    // Validate/serialize the entire batch before creating the output directory.
    let mut outputs = Vec::with_capacity(staged.weenies.len());
    for template in &staged.weenies {
        outputs.push((
            format!("{}.toml", template.weenie_id),
            bace_content_tools::export(template)?,
        ));
    }
    let mut manifest = toml::Table::new();
    manifest.insert("source_sha256".into(), staged.manifest.source_sha256.into());
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
    let manifest = toml::to_string_pretty(&manifest)?;
    std::fs::create_dir_all(directory)?;
    for (name, contents) in outputs {
        std::fs::write(directory.join(name), contents)?;
    }
    std::fs::write(directory.join("manifest.toml"), manifest)?;
    println!(
        "Converted {} weenies into {}. No database content published.",
        staged.weenies.len(),
        directory.display()
    );
    Ok(())
}
