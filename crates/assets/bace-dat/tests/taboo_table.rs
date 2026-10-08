use bace_dat::{DatTableLimits, TabooTable};
// Synthetic ACE BinaryWriter fixture: ID, byte marker/count, dictionary keys,
// TabooTableEntry uint/ushort/count, BinaryReader 7-bit UTF8 strings. No assets.
fn record() -> Vec<u8> {
    vec![
        30, 0, 0, 14, 1, 2, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 2, 0, 0, 0, 3, b'f', b'o', b'o', 5, b'*',
        b'b', b'a', b'r', b'*', 4, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 4, b'b', b'a', b'z',
        b'*',
    ]
}
#[test]
fn pinned_layout_retains_category_order_and_patterns() {
    let table = TabooTable::decode(&record()).unwrap();
    assert_eq!(table.marker, 1);
    assert_eq!(table.entries.len(), 2);
    assert_eq!(table.entries[0].flags, 1);
    assert_eq!(table.entries[0].unknown1, 0x10101);
    assert_eq!(table.entries[0].unknown2, 0);
    assert_eq!(table.entries[0].patterns, ["foo", "*bar*"]);
    assert_eq!(table.entries[1].flags, 4);
    assert_eq!(table.entries[1].patterns, ["baz*"]);
}
#[test]
fn truncation_duplicate_categories_and_aggregate_budgets_fail() {
    let bytes = record();
    for end in 0..bytes.len() {
        assert!(TabooTable::decode(&bytes[..end]).is_err(), "{end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(TabooTable::decode(&trailing).is_err());
    let mut duplicate = bytes.clone();
    duplicate[30] = 1;
    assert!(TabooTable::decode(&duplicate).is_err());
    for limits in [
        DatTableLimits {
            max_entries: 4,
            ..Default::default()
        },
        DatTableLimits {
            max_string_bytes: 2,
            ..Default::default()
        },
        DatTableLimits {
            max_record_bytes: 8,
            ..Default::default()
        },
    ] {
        assert!(TabooTable::decode_with_limits(&bytes, limits).is_err());
    }
}
#[test]
#[ignore = "requires fingerprint-approved user portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_taboo_table_decodes_with_bounded_patterns() {
    let path =
        std::path::PathBuf::from(std::env::var("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY"))
            .join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut archive = bace_dat::DatArchive::open(path).unwrap();
    let header = archive.header();
    let version = bace_dat::DatTableVersion {
        engine_version: header.engine_version,
        game_version: header.game_version,
        record_iteration: archive.records()[&TabooTable::RECORD_ID].iteration,
    };
    let table = TabooTable::load_verified(&mut archive, version).unwrap();
    assert!(!table.entries.is_empty());
    assert!(!table.entries[0].patterns.is_empty());
    eprintln!(
        "taboo categories={} first-category patterns={}",
        table.entries.len(),
        table.entries[0].patterns.len()
    );
    // Report only grammar counts, never proprietary pattern bytes.
    let unsupported = table.entries[0]
        .patterns
        .iter()
        .filter(|p| {
            p.chars()
                .any(|c| !c.is_alphabetic() && !matches!(c, '*' | '-' | '\''))
        })
        .count();
    eprintln!("patterns outside documented literal-letter/wildcard grammar={unsupported}");
}
#[test]
fn independent_official_binary_writer_and_decoder_fixture() {
    let fixture = include_str!("../../../gameplay/bace-character/tests/fixtures/taboo.csv");
    let hex = fixture
        .lines()
        .find_map(|line| line.strip_prefix("bytes,"))
        .unwrap();
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(bytes, record());
    let table = TabooTable::decode(&bytes).unwrap();
    for (entry, line) in table.entries.iter().zip(
        fixture
            .lines()
            .filter_map(|line| line.strip_prefix("entry,")),
    ) {
        let fields: Vec<_> = line.split(',').collect();
        assert_eq!(entry.flags, fields[0].parse::<u32>().unwrap());
        assert_eq!(entry.unknown1, fields[1].parse::<u32>().unwrap());
        assert_eq!(entry.unknown2, fields[2].parse::<u16>().unwrap());
        assert_eq!(entry.patterns, fields[3].split(';').collect::<Vec<_>>());
    }
}
