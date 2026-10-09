use bace_dat::DatArchive;
use bace_dat_service::{
    DddCatalog, DddError, DddLimits, DddSession, DddStart, prepare_archive_record,
};
use bace_wire::{DddDatabase as Db, DddInterrogationResponse, DddIterationSet};
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const FILE_ID: u32 = 0x0100_0001;
const VERSION_ID: u32 = 0xffff_0001;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        for _ in 0..16 {
            let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bace-ddd-catalog-{}-{serial}.dat",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let fixture = Self { path };
                    let written = file.write_all(bytes);
                    drop(file);
                    written.unwrap();
                    return fixture;
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create DAT fixture: {error}"),
            }
        }
        panic!("cannot allocate a unique DAT fixture");
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_file(&self.path).expect("remove DAT fixture");
    }
}

fn fixture(dataset: u32, version: &[u8], file: Option<(u32, u32, &[u8])>) -> Fixture {
    // Independent synthetic DAT image exercising the real bace-dat sector and
    // directory reader. No proprietary DAT bytes are checked into this test.
    let mut bytes = vec![0; 5120];
    let put = |bytes: &mut [u8], offset: usize, value: u32| {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    put(&mut bytes, 0x140, 0x5442);
    put(&mut bytes, 0x144, 1024);
    put(&mut bytes, 0x148, 5120);
    put(&mut bytes, 0x14c, dataset);
    put(&mut bytes, 0x160, 1024);
    put(&mut bytes, 1024, 2048);
    put(&mut bytes, 1028 + 248, if file.is_some() { 2 } else { 1 });
    if let Some((id, iteration, raw)) = file {
        put(&mut bytes, 1028 + 256, id);
        put(&mut bytes, 1028 + 260, 3072);
        put(&mut bytes, 1028 + 264, raw.len() as u32);
        put(&mut bytes, 1028 + 272, iteration);
        bytes[3076..3076 + raw.len()].copy_from_slice(raw);
        put(&mut bytes, 1028 + 280, VERSION_ID);
        put(&mut bytes, 1028 + 284, 4096);
        put(&mut bytes, 1028 + 288, version.len() as u32);
    } else {
        put(&mut bytes, 1028 + 256, VERSION_ID);
        put(&mut bytes, 1028 + 260, 4096);
        put(&mut bytes, 1028 + 264, version.len() as u32);
    }
    bytes[4100..4100 + version.len()].copy_from_slice(version);
    Fixture::new(&bytes)
}

fn iteration(total: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(-(total as i32)).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out
}

fn response(portal: u32) -> DddInterrogationResponse {
    DddInterrogationResponse {
        client_language: 1,
        with_keys: vec![
            DddIterationSet {
                dat_file_type: 0,
                dat_file_id: 1,
                iterations: portal,
                runs: if portal == 2 { vec![-2, 1] } else { vec![1] },
            },
            DddIterationSet {
                dat_file_type: 1,
                dat_file_id: 3,
                iterations: 1,
                runs: vec![1],
            },
        ],
        trailing_bytes: 0,
    }
}

#[test]
fn index_only_catalog_accepts_current_client_and_rejects_missing_data() {
    let portal = fixture(1, &iteration(2), Some((FILE_ID, 2, &[42; 256])));
    let language = fixture(3, &iteration(1), None);
    let mut archives = vec![
        (Db::Portal, DatArchive::open(portal.path()).unwrap()),
        (Db::Language, DatArchive::open(language.path()).unwrap()),
    ];
    let catalog =
        Arc::new(DddCatalog::from_archive_indexes(&mut archives, DddLimits::default()).unwrap());
    assert_eq!(catalog.database_iteration(Db::Portal), Some(2));
    assert_eq!(catalog.database_iteration(Db::Language), Some(1));
    assert_eq!(catalog.database_iteration(Db::Cell), None);
    let mut current = DddSession::new(catalog.clone(), DddLimits::default(), false, 1).unwrap();
    assert!(matches!(
        current.begin(&response(2)),
        Ok(DddStart::UpToDate(_))
    ));
    let mut older = DddSession::new(catalog.clone(), DddLimits::default(), false, 2).unwrap();
    assert!(matches!(older.begin(&response(1)), Err(DddError::Disabled)));
    let mut unsafe_patch = DddSession::new(catalog, DddLimits::default(), true, 3).unwrap();
    assert!(matches!(
        unsafe_patch.begin(&response(1)),
        Err(DddError::InvalidCatalog)
    ));
}

#[test]
fn exact_catalog_matches_later_preparation_and_begin_size() {
    let raw = [42; 256];
    let portal = fixture(1, &iteration(2), Some((FILE_ID, 2, &raw)));
    let language = fixture(3, &iteration(1), None);
    let cell_id = 0x1234_ffff;
    let cell = fixture(2, &iteration(2), Some((cell_id, 2, &raw)));
    let mut archives = vec![
        (Db::Portal, DatArchive::open(portal.path()).unwrap()),
        (Db::Language, DatArchive::open(language.path()).unwrap()),
        (Db::Cell, DatArchive::open(cell.path()).unwrap()),
    ];
    let limits = DddLimits::default();
    let catalog = Arc::new(DddCatalog::from_archives(&mut archives, limits).unwrap());
    let mut session = DddSession::new(catalog.clone(), limits, true, 4).unwrap();
    let prepared =
        prepare_archive_record(&mut archives[0].1, Db::Portal, FILE_ID, 6, limits).unwrap();
    assert_eq!(
        catalog.record(Db::Portal, FILE_ID),
        Some(prepared.metadata())
    );
    let cell_record = catalog.record(Db::Cell, cell_id).unwrap();
    assert!(!cell_record.compressed);
    assert_eq!(cell_record.transfer_size, raw.len() as u32);
    let on_demand = bace_dat_service::prepare_archive_record_uncompressed(
        &mut archives[2].1,
        Db::Cell,
        cell_id,
        1,
        limits,
    )
    .unwrap();
    assert_eq!(on_demand.metadata(), cell_record);
    assert!(
        matches!(session.begin(&response(1)), Ok(DddStart::Patch { queued_records: 1, transfer_bytes, .. }) if transfer_bytes == prepared.metadata().transfer_size)
    );
    let job = session.take_job().unwrap();
    session.complete_job(job, &prepared).unwrap();
    assert!(session.acknowledge_end().unwrap().is_some());
}

#[test]
fn stock_patch_limits_cover_large_source_record_with_finite_caps() {
    let limits = DddLimits::stock_patch();
    limits.validate().unwrap();
    assert!(limits.max_record_bytes > 8_949_784);
    assert!(limits.max_pending_records >= 805_348);
    assert!(limits.max_transfer_bytes > 852_705_094);
}

#[test]
fn malformed_version_and_wrong_dataset_fail_before_publication() {
    let bad = fixture(1, &[2, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], None);
    let language = fixture(3, &iteration(1), None);
    let mut archives = vec![
        (Db::Portal, DatArchive::open(bad.path()).unwrap()),
        (Db::Language, DatArchive::open(language.path()).unwrap()),
    ];
    assert!(matches!(
        DddCatalog::from_archive_indexes(&mut archives, DddLimits::default()),
        Err(DddError::InvalidIterations)
    ));
    archives[0].0 = Db::Cell;
    assert!(matches!(
        DddCatalog::from_archive_indexes(&mut archives, DddLimits::default()),
        Err(DddError::InvalidCatalog)
    ));

    let duplicate = fixture(
        1,
        &[
            2, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0,
        ],
        None,
    );
    archives[0] = (Db::Portal, DatArchive::open(duplicate.path()).unwrap());
    assert!(matches!(
        DddCatalog::from_archive_indexes(&mut archives, DddLimits::default()),
        Err(DddError::InvalidIterations)
    ));
    drop(archives);
}

#[test]
#[ignore = "requires user-supplied DATs; set BACE_DAT_DIRECTORY and run --ignored"]
fn supplied_dat_iteration_records_are_read_without_scanning_payloads() {
    let root = std::path::PathBuf::from(std::env::var("BACE_DAT_DIRECTORY").unwrap());
    let mut archives = vec![
        (
            Db::Portal,
            DatArchive::open(root.join("client_portal.dat")).unwrap(),
        ),
        (
            Db::Language,
            DatArchive::open(root.join("client_local_English.dat")).unwrap(),
        ),
        (
            Db::Cell,
            DatArchive::open(root.join("client_cell_1.dat")).unwrap(),
        ),
    ];
    let catalog = DddCatalog::from_archive_indexes(&mut archives, DddLimits::default()).unwrap();
    assert!(catalog.record(Db::Portal, FILE_ID).is_none());
}

#[test]
#[ignore = "requires user-supplied DATs and scans Portal/Language payloads; set BACE_DAT_DIRECTORY"]
fn supplied_dat_exact_catalog_prepares_with_stock_patch_limits() {
    let root = std::path::PathBuf::from(std::env::var("BACE_DAT_DIRECTORY").unwrap());
    let mut archives = vec![
        (
            Db::Portal,
            DatArchive::open(root.join("client_portal.dat")).unwrap(),
        ),
        (
            Db::Language,
            DatArchive::open(root.join("client_local_English.dat")).unwrap(),
        ),
        (
            Db::Cell,
            DatArchive::open(root.join("client_cell_1.dat")).unwrap(),
        ),
    ];
    let catalog = DddCatalog::from_archives(&mut archives, DddLimits::stock_patch()).unwrap();
    assert!(catalog.database_iteration(Db::Portal).unwrap() > 0);
    assert!(catalog.database_iteration(Db::Language).unwrap() > 0);
    assert!(catalog.database_iteration(Db::Cell).unwrap() > 0);
    assert!(catalog.record(Db::Portal, 0x0100_0001).is_some());
}
