use bace_dat::DualDidMapper;
fn fixture() -> Vec<u8> {
    let s = include_str!("fixtures/components_mapper.hex").trim();
    (0..s.len())
        .step_by(2)
        .map(|n| u8::from_str_radix(&s[n..n + 2], 16).unwrap())
        .collect()
}
#[test]
fn independently_compiled_ace_mapper_and_every_truncation() {
    let bytes = fixture();
    let mapper = DualDidMapper::decode(&bytes).unwrap();
    for row in include_str!("fixtures/components_mapper.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let fields: Vec<_> = row.split(',').collect();
        match fields[0] {
            "numbering" => assert_eq!(
                mapper.numbering,
                fields[1..]
                    .iter()
                    .map(|n| n.parse::<u8>().unwrap())
                    .collect::<Vec<_>>()
                    .as_slice()
            ),
            "client" => assert_eq!(
                mapper.client_ids.get(&fields[1].parse().unwrap()),
                Some(&fields[2].parse().unwrap())
            ),
            "server" => assert_eq!(
                mapper.server_ids.get(&fields[1].parse().unwrap()),
                Some(&fields[2].parse().unwrap())
            ),
            "name" => assert_eq!(
                mapper
                    .client_names
                    .get(&fields[1].parse().unwrap())
                    .map(String::as_str),
                Some(fields[2])
            ),
            "length" => assert_eq!(bytes.len(), fields[1].parse::<usize>().unwrap()),
            _ => panic!("oracle row"),
        }
    }
    for cut in 0..bytes.len() {
        assert!(DualDidMapper::decode(&bytes[..cut]).is_err(), "cut {cut}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(DualDidMapper::decode(&trailing).is_err());
}

#[test]
fn material_record_uses_source_layout_with_exact_record_identity() {
    let hex = include_str!("fixtures/materials_mapper.hex").trim();
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let limits = bace_dat::DatTableLimits::default();
    let mapper =
        DualDidMapper::decode_record(&bytes, DualDidMapper::MATERIAL_RECORD_ID, limits).unwrap();
    for row in include_str!("fixtures/materials_mapper.csv")
        .lines()
        .filter(|l| l.starts_with("name,"))
    {
        let fields: Vec<_> = row.split(',').collect();
        assert_eq!(
            mapper
                .client_names
                .get(&fields[1].parse().unwrap())
                .map(String::as_str),
            Some(fields[2])
        );
    }
    assert!(
        DualDidMapper::decode(&bytes).is_err(),
        "material record cannot impersonate components"
    );
    assert!(DualDidMapper::decode_record(&bytes, 0x26000000, limits).is_err());
    for cut in 0..bytes.len() {
        assert!(
            DualDidMapper::decode_record(&bytes[..cut], DualDidMapper::MATERIAL_RECORD_ID, limits)
                .is_err()
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(
        DualDidMapper::decode_record(&trailing, DualDidMapper::MATERIAL_RECORD_ID, limits).is_err()
    );
}
