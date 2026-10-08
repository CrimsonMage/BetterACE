use bace_replication::project_training_notice;
#[test]
fn all_skill_names_and_credit_boundaries_match_original_csharp_handler() {
    let mut count = 0;
    for row in include_str!("fixtures/training_notice.txt").lines() {
        let mut fields = row.splitn(4, '|');
        let skill = fields.next().unwrap().parse().unwrap();
        let credits = fields.next().unwrap().parse().unwrap();
        let success = fields.next().unwrap() == "1";
        let text = fields.next().unwrap();
        let bytes = project_training_notice(skill, credits, success).unwrap();
        // Source GameMessageSystemChat: 0xF7E0, String16 aligned to four, Int32 type.
        assert_eq!(&bytes[..4], &0xF7E0u32.to_le_bytes());
        let len = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;
        assert_eq!(&bytes[6..6 + len], text.as_bytes(), "source row {row}");
        assert_eq!(&bytes[bytes.len() - 4..], &13u32.to_le_bytes());
        count += 1;
    }
    assert_eq!(count, 440);
    assert!(project_training_notice(55, 0, true).is_err());
    assert!(project_training_notice(1, u32::MAX, true).is_err());
}
