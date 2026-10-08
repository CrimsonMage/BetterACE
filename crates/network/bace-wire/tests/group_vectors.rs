use bace_wire::{
    AllegianceMemberData, AllegianceProfileData, FellowData, FellowshipData, GroupAction,
    GroupEvent, GroupRequest, SocialCodecLimits, WirePosition, opcode::GameActionType,
};
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn limits() -> SocialCodecLimits {
    SocialCodecLimits {
        max_message_bytes: 16384,
        max_entries: 4096,
        max_filters: 4,
        max_string_bytes: 256,
    }
}
fn fellow(id: u32) -> FellowData {
    FellowData {
        actor: id,
        name: format!("Fellow{id}"),
        level: id,
        maximum: [100, 200, 300],
        current: [90, 80, 70],
    }
}
fn node(id: u32, rank: u16) -> AllegianceMemberData {
    AllegianceMemberData {
        actor: id,
        name: format!("Member{id}"),
        level: id,
        rank,
        cached: 100 + id,
        tithed: 200 + id,
        online: id != 32,
        may_pass_up: id != 1,
        gender: 1,
        heritage: 1,
        loyalty: 80,
        leadership: 90,
    }
}
fn profile() -> AllegianceProfileData {
    AllegianceProfileData {
        total_members: 3,
        total_vassals: 1,
        chat_room: 700,
        name: "Rune".into(),
        sanctuary: Some(WirePosition {
            cell: 0x12340001,
            origin: [10., 20., 42.],
            rotation: [1., 0., 0., 0.],
        }),
        monarch: Some(node(1, 5)),
        records: vec![(1, node(17, 2)), (17, node(32, 1))],
    }
}
#[test]
fn original_group_action_read_prefixes_and_bounds() {
    let mut count = 0;
    for row in include_str!("fixtures/groups.csv")
        .lines()
        .filter(|l| l.starts_with("input,"))
    {
        let f: Vec<_> = row.split(',').collect();
        let op = GameActionType::NAMED
            .iter()
            .find(|(n, _)| *n == f[1])
            .unwrap()
            .1;
        let bytes = hex(f[2]);
        let decoded = GroupRequest::decode(op, &bytes, 1024, 64).unwrap();
        assert_eq!(
            bytes.len() - decoded.trailing_bytes,
            f[3].parse::<usize>().unwrap()
        );
        if f[1] == "SetAllegianceOfficerTitle" {
            assert_eq!(
                decoded.action,
                GroupAction::SetTitle {
                    level: u32::MAX,
                    title: "Rune".into()
                }
            )
        }
        if f[1] == "ConfirmationResponse" {
            assert_eq!(
                decoded.action,
                GroupAction::Confirm {
                    kind: u32::MAX,
                    token: u32::MAX,
                    accepted: true
                }
            )
        }
        for n in 0..bytes.len() {
            assert!(
                GroupRequest::decode(op, &bytes[..n], 1024, 64).is_err(),
                "{} {n}",
                f[1]
            );
        }
        let mut extra = bytes;
        extra.extend_from_slice(&[1, 2]);
        assert_eq!(
            GroupRequest::decode(op, &extra, 1024, 64)
                .unwrap()
                .trailing_bytes,
            2
        );
        count += 1;
    }
    assert_eq!(count, 35);
}
#[test]
fn original_full_group_serializers_match_exact_bytes() {
    let selected_fellow = fellow(17);
    let fellowship = FellowshipData {
        name: "Rune".into(),
        leader: 17,
        members: vec![fellow(17), fellow(32), fellow(1)],
        share_xp: true,
        even_share: false,
        open: true,
        locked: true,
        departed: vec![(34, 123), (1, 456)],
        locks: vec![("Rune".into(), 789, 1)],
    };
    let profile = profile();
    let empty = AllegianceProfileData {
        total_members: 0,
        total_vassals: 0,
        chat_room: 0,
        name: String::new(),
        sanctuary: None,
        monarch: None,
        records: vec![],
    };
    let mut count = 0;
    for row in include_str!("fixtures/groups.csv")
        .lines()
        .filter(|l| l.starts_with("output,"))
    {
        let f: Vec<_> = row.split(',').collect();
        let event = match f[1] {
            "fellowship" => GroupEvent::Fellowship(&fellowship),
            "fellow" => GroupEvent::Fellow {
                fellow: &selected_fellow,
                share_loot: true,
                update_type: 1,
            },
            "quit" => GroupEvent::FellowQuit(17),
            "dismiss" => GroupEvent::FellowDismiss(17),
            "disband" => GroupEvent::FellowDisband,
            "fellowdone" => GroupEvent::FellowDone,
            "confirm" => GroupEvent::Confirmation {
                kind: 4,
                token: 77,
                text: "Rune",
            },
            "confirmdone" => GroupEvent::ConfirmationDone { kind: 4, token: 77 },
            "allegiancedone" => GroupEvent::AllegianceDone(0),
            "empty" => GroupEvent::Allegiance {
                rank: 0,
                profile: &empty,
            },
            "allegiance" => GroupEvent::Allegiance {
                rank: 2,
                profile: &profile,
            },
            "info" => GroupEvent::AllegianceInfo {
                subject: 17,
                profile: &profile,
            },
            other => panic!("{other}"),
        };
        assert_eq!(
            event.encode(99, 7, limits()).unwrap(),
            hex(f[2]),
            "{}",
            f[1]
        );
        count += 1;
    }
    assert_eq!(count, 12);
}
