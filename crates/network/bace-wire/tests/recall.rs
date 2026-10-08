use bace_wire::{RecallAction, RecallRequest, WireError, opcode::GameActionType};
#[test]
fn original_ace_recall_handlers_route_all_seven_actions_and_ignore_bounded_suffixes() {
    let mut count = 0;
    for line in include_str!("fixtures/recall.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let row: Vec<_> = line.split('\t').collect();
        let bytes: Vec<_> = row[1]
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let actual =
            RecallRequest::decode(GameActionType(row[0].parse().unwrap()), &bytes, 4).unwrap();
        let expected = match row[2] {
            "TeleToLifestone" => RecallAction::Lifestone,
            "TeleToHouse" => RecallAction::House,
            "TeleToMarketPlace" => RecallAction::Marketplace,
            "RecallAllegianceHometown" => RecallAction::AllegianceHometown,
            "TeleToMansion" => RecallAction::AllegianceHousing,
            "TeleToPkArena" => RecallAction::PkArena,
            "TeleToPklArena" => RecallAction::PklArena,
            other => panic!("unrecognized original handler: {other}"),
        };
        assert_eq!(actual.action, expected);
        assert_eq!(actual.trailing_bytes, row[3].parse::<usize>().unwrap());
        count += 1;
    }
    assert_eq!(count, 14);
}
#[test]
fn recall_budget_and_unsupported_actions_fail_without_inventing_fields() {
    assert_eq!(
        RecallRequest::decode(GameActionType::TeleToLifestone, &[0; 5], 4),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        RecallRequest::decode(GameActionType::Use, &[], 0),
        Err(WireError::UnexpectedOpcode(GameActionType::Use.0))
    );
    assert!(RecallRequest::decode(GameActionType::TeleToLifestone, &[], 0).is_ok());
}
