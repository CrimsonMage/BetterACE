use bace_dat::{DatArchive, DatStore, Landblock, fingerprint};
use std::io::Write;

fn image() -> Vec<u8> {
    let mut bytes = vec![0; 4096];
    let put = |b: &mut [u8], p: usize, n: u32| b[p..p + 4].copy_from_slice(&n.to_le_bytes());
    put(&mut bytes, 0x140, 0x5442);
    put(&mut bytes, 0x144, 1024);
    put(&mut bytes, 0x148, 4096);
    put(&mut bytes, 0x14c, 2);
    put(&mut bytes, 0x160, 1024);
    put(&mut bytes, 1024, 2048); // Directory chain.
    put(&mut bytes, 1028 + 248, 1);
    put(&mut bytes, 1028 + 256, 0x1234ffff);
    put(&mut bytes, 1028 + 260, 3072);
    put(&mut bytes, 1028 + 264, 252);
    put(&mut bytes, 3076, 0x1234ffff);
    put(&mut bytes, 3080, 1);
    bytes[3076 + 170] = 12;
    bytes
}

fn file(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file
}

#[test]
fn bounded_archive_and_landblock_decode() {
    let file = file(&image());
    let mut dat = DatArchive::open(file.path()).unwrap();
    assert_eq!(dat.records().len(), 1);
    let landblock = Landblock::decode(&dat.read(0x1234ffff).unwrap()).unwrap();
    assert!(landblock.has_objects);
    assert_eq!(landblock.heights[0], 12);
    assert!(dat.read(0x9999).is_err());
}

#[test]
fn directory_cycles_and_bad_counts_are_rejected() {
    let mut bytes = image();
    bytes[1024..1028].copy_from_slice(&1024_u32.to_le_bytes());
    assert!(DatArchive::open(file(&bytes).path()).is_err());
    let mut bytes = image();
    bytes[1276..1280].copy_from_slice(&62_u32.to_le_bytes());
    assert!(DatArchive::open(file(&bytes).path()).is_err());
    let mut bytes = image();
    bytes[2048..2052].copy_from_slice(&2048_u32.to_le_bytes());
    assert!(DatArchive::open(file(&bytes).path()).is_err());
    let mut bytes = image();
    bytes[3072..3076].copy_from_slice(&3072_u32.to_le_bytes());
    let fixture = file(&bytes);
    let mut dat = DatArchive::open(fixture.path()).unwrap();
    assert!(dat.read(0x1234ffff).is_err());
}

#[test]
fn multi_sector_record_preserves_payload_and_rejects_extra_link() {
    let mut bytes = image();
    bytes.resize(5120, 0);
    bytes[0x148..0x14c].copy_from_slice(&5120_u32.to_le_bytes());
    bytes[1292..1296].copy_from_slice(&1021_u32.to_le_bytes());
    bytes[3072..3076].copy_from_slice(&4096_u32.to_le_bytes());
    for (index, byte) in bytes[3076..4096].iter_mut().enumerate() {
        *byte = index as u8;
    }
    bytes[4100] = 0xa5;
    let fixture = file(&bytes);
    let mut dat = DatArchive::open(fixture.path()).unwrap();
    let record = dat.read(0x1234ffff).unwrap();
    assert_eq!(record.len(), 1021);
    assert_eq!(&record[..1020], &bytes[3076..4096]);
    assert_eq!(record[1020], 0xa5);

    bytes[4096..4100].copy_from_slice(&3072_u32.to_le_bytes());
    let fixture = file(&bytes);
    let mut dat = DatArchive::open(fixture.path()).unwrap();
    assert!(dat.read(0x1234ffff).is_err());
}

#[test]
fn verified_archive_clones_read_concurrently_and_reject_wrong_hash() {
    let fixture = file(&image());
    let expected = fingerprint(fixture.path()).unwrap();
    assert!(DatArchive::open_verified(fixture.path(), &"0".repeat(64)).is_err());
    let archive = DatArchive::open_verified(fixture.path(), &expected).unwrap();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let mut reader = archive.clone();
                scope.spawn(move || {
                    for _ in 0..100 {
                        let bytes = reader.read(0x1234ffff).unwrap();
                        assert_eq!(bytes.len(), 252);
                        assert_eq!(Landblock::decode(&bytes).unwrap().id, 0x1234ffff);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
}

#[test]
fn store_rejects_crossed_datasets() {
    let fixture = file(&image());
    let raw = DatArchive::open(fixture.path()).unwrap();
    assert!(DatStore::new(raw.clone(), raw.clone(), raw).is_err());
    let archive =
        DatArchive::open_verified(fixture.path(), &fingerprint(fixture.path()).unwrap()).unwrap();
    assert!(DatStore::new(archive.clone(), archive.clone(), archive).is_err());
}

#[test]
#[ignore = "requires user-supplied DAT assets; set BACE_DAT_DIRECTORY and run --ignored"]
fn supplied_dat_archives_and_actual_landblocks() {
    let root = std::path::PathBuf::from(
        std::env::var("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY required"),
    );
    for (name, dataset) in [
        ("client_portal.dat", 1),
        ("client_cell_1.dat", 2),
        ("client_local_English.dat", 3),
    ] {
        let mut dat = DatArchive::open(root.join(name)).unwrap();
        assert_eq!(dat.header().dataset, dataset);
        assert!(!dat.records().is_empty());
        eprintln!("{name}: {} records", dat.records().len());
        if dataset == 2 {
            let ids: Vec<_> = dat
                .records()
                .keys()
                .copied()
                .filter(|id| id & 0xffff == 0xffff)
                .collect();
            assert!(!ids.is_empty());
            for id in ids {
                assert_eq!(Landblock::decode(&dat.read(id).unwrap()).unwrap().id, id);
            }
        }
    }
}
