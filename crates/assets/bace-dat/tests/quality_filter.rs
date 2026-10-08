use bace_dat::{DatTableLimits, QualityFilter};
#[test]
fn original_csharp_quality_filter_fields_and_all_truncations() {
    let mut lines = include_str!("fixtures/quality_filter.txt")
        .lines()
        .filter(|line| !line.starts_with('#'));
    let hex = lines.next().unwrap();
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    let decoded = QualityFilter::decode(&bytes).unwrap();
    let lists = [
        &decoded.ints,
        &decoded.int64s,
        &decoded.bools,
        &decoded.floats,
        &decoded.data_ids,
        &decoded.instance_ids,
        &decoded.strings,
        &decoded.positions,
        &decoded.attributes,
        &decoded.secondary_attributes,
        &decoded.skills,
    ];
    for (actual, line) in lists.into_iter().zip(lines) {
        assert_eq!(
            *actual,
            line.split(',')
                .map(|v| v.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        );
    }
    for n in 0..bytes.len() {
        assert!(QualityFilter::decode(&bytes[..n]).is_err(), "{n}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(QualityFilter::decode(&trailing).is_err());
    assert!(
        QualityFilter::decode_with_limits(
            &bytes,
            DatTableLimits {
                max_entries: 40,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut bad = bytes;
    bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(QualityFilter::decode(&bad).is_err());
    assert!(decoded.allows_int(100));
    assert!(!decoded.allows_int(512));
    assert!(decoded.allows_float(400));
}
