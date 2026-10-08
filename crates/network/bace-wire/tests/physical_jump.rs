use bace_wire::*;
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn independent_local_client_methods_keep_position_jump_epochs_at_actual_offsets() {
    let mut count = 0;
    for line in include_str!("fixtures/physical_jump.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let row: Vec<_> = line.split(',').collect();
        let bytes = hex(row[3]);
        let extent = row[2].parse::<f32>().unwrap();
        match row[0] {
            "position" => {
                let jump = ClientPositionJump::decode(&bytes, 56).unwrap();
                assert_eq!(jump.extent, extent);
                assert_eq!(jump.reported_velocity, [1.25, -2.5, 3.75]);
                assert_eq!(jump.reported_position.cell, 0x12340100);
                assert_eq!(jump.reported_position.origin, [1., 2., 3.]);
                assert_eq!(
                    jump.epochs,
                    MovementEpochs {
                        instance: 0x1122,
                        server_control: 0x3344,
                        teleport: 0x5566,
                        force_position: 0x7788
                    }
                );
                for n in 0..bytes.len() {
                    assert!(ClientPositionJump::decode(&bytes[..n], 56).is_err());
                }
                assert!(ClientPositionJump::decode(&bytes, 55).is_err());
            }
            "nonauto" => {
                assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0xf7c9);
                assert_eq!(
                    ClientNonAutonomousJump::decode(&bytes[8..], 4)
                        .unwrap()
                        .extent,
                    extent
                );
                for n in 0..4 {
                    assert!(ClientNonAutonomousJump::decode(&bytes[8..8 + n], 4).is_err());
                }
            }
            _ => panic!(),
        }
        count += 1;
    }
    assert_eq!(count, 8);
}
