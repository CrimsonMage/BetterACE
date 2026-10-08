use bace_wire::MapTeleportInput;
#[test]
fn original_advocate_teleport_handler_reads_position_after_ignored_string() {
    let mut count = 0;
    for row in include_str!("fixtures/staff_map.csv").lines().skip(2) {
        let fields: Vec<_> = row.split(',').collect();
        let bytes: Vec<_> = fields[3]
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let input = MapTeleportInput::decode(&bytes, 128, 16).unwrap();
        assert_eq!(input.ignored_target, "map");
        assert_eq!(input.position.cell, 0x12340001);
        assert_eq!(input.position.origin, [10., 20., 9999.]);
        assert_eq!(input.position.rotation, [1., 0., 0., 0.]);
        assert_eq!(
            bytes.len() - input.trailing_bytes,
            fields[4].parse::<usize>().unwrap()
        );
        for n in 0..bytes.len() {
            assert!(MapTeleportInput::decode(&bytes[..n], 128, 16).is_err());
        }
        assert!(MapTeleportInput::decode(&bytes, bytes.len() - 1, 16).is_err());
        assert!(MapTeleportInput::decode(&bytes, 128, 2).is_err());
        let mut extra = bytes;
        extra.extend_from_slice(&[1, 2]);
        assert_eq!(
            MapTeleportInput::decode(&extra, 128, 16)
                .unwrap()
                .trailing_bytes,
            2
        );
        count += 1;
    }
    assert_eq!(count, 32);
}
