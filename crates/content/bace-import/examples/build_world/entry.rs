//! Explicit offline conversion; uses only a new isolated private MariaDB instance.
use bace_import::{MariaDbBinaries, MariaDbStaging, import_complete_world};
use std::{path::PathBuf, sync::atomic::AtomicBool};
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err(
            "usage: build_world SQL_FILE OUTPUT_DIRECTORY (set BACE_MARIADB_BASEDIR)".into(),
        );
    }
    let basedir =
        PathBuf::from(std::env::var_os("BACE_MARIADB_BASEDIR").ok_or("set BACE_MARIADB_BASEDIR")?);
    let mut backend = MariaDbStaging::new(MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    });
    let world = import_complete_world(&mut backend, &PathBuf::from(&args[1]))?;
    eprintln!(
        "Extracted {} weenies, {} world rows; SQL SHA256 {}",
        world.weenies.len(),
        world.records.len(),
        world.manifest.source_sha256
    );
    let references = bace_content::inspect_world_references(&world.weenies, &world.records);
    eprintln!(
        "references checked={} missing={} categories={:?}",
        references.checked, references.missing, references.missing_by_field
    );
    for issue in references.examples.iter().take(20) {
        eprintln!("reference diagnostic: {:?}", issue);
    }
    let directory = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&directory)?;
    let build = bace_content_tools::build_world_pack(
        &world.weenies,
        &world.records,
        &directory,
        &AtomicBool::new(false),
    )?;
    eprintln!(
        "{} records in {}\nmanifest {}",
        build.records,
        build.file.display(),
        build.manifest.display()
    );
    Ok(())
}
