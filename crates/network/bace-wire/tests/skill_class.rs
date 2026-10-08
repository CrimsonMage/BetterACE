use bace_wire::SkillClassUpdate;
#[test]
fn client_disassembly_offsets_are_unpadded() {
    // Independently specified client offsets: opcode0,timestamp4,object5,skill9,AC13.
    assert_eq!(
        SkillClassUpdate {
            sequence: 0xa5,
            object: 0x50000001,
            skill: 28,
            advancement: 2
        }
        .encode()
        .unwrap(),
        [0xe2, 2, 0, 0, 0xa5, 1, 0, 0, 0x50, 28, 0, 0, 0, 2, 0, 0, 0]
    );
    assert!(
        SkillClassUpdate {
            sequence: 0,
            object: 1,
            skill: 1,
            advancement: 4
        }
        .encode()
        .is_err()
    );
}
