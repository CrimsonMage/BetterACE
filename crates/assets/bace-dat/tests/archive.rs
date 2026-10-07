use bace_dat::{DatArchive, Landblock};
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
