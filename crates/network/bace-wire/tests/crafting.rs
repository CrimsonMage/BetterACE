use bace_wire::{
    CraftingAction, CraftingEvent, CraftingRequest, GameEventEnvelope, Reader, SalvageWireResult,
    WireError, opcode::GameActionType as Action,
};
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn compiled_pinned_ace_serializers_and_handlers() {
    let mut count = 0;
    for line in include_str!("fixtures/crafting.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<_> = line.split('\t').collect();
        let bytes = hex(v[1]);
        let event = match v[0] {
            "confirm" => Some(CraftingEvent::ConfirmationRequest {
                confirmation_type: 5,
                context: 17,
                text: "Apply salvage?",
            }),
            "confirm-alter-skill" | "confirm-augmentation" => {
                Some(CraftingEvent::ConfirmationRequest {
                    confirmation_type: if v[0] == "confirm-alter-skill" { 2 } else { 6 },
                    context: 17,
                    text: "Apply skill device?",
                })
            }
            "done" => Some(CraftingEvent::ConfirmationDone {
                confirmation_type: 5,
                context: 17,
            }),
            "salvage" | "salvage-100" => Some(CraftingEvent::SalvageResult {
                skill: 40,
                unsuitable: &[],
                result: Some(SalvageWireResult {
                    material: 61,
                    workmanship: 9.5,
                    units: if v[0] == "salvage-100" { 100 } else { 101 },
                }),
                augmentation_bonus: 100,
            }),
            "salvage-empty" => Some(CraftingEvent::SalvageResult {
                skill: 40,
                unsuitable: &[],
                result: None,
                augmentation_bonus: 100,
            }),
            "input-salvage" => {
                let request =
                    CraftingRequest::decode(Action::CreateTinkeringTool, &bytes, 4096).unwrap();
                assert_eq!(
                    request.action,
                    CraftingAction::Salvage {
                        tool_id: 2,
                        items: vec![3, 4]
                    }
                );
                assert_eq!(request.trailing_bytes, 1);
                assert_eq!(v[2], "2,3:4");
                None
            }
            name if name.starts_with("input-confirm-") => {
                let request =
                    CraftingRequest::decode(Action::ConfirmationResponse, &bytes, 4096).unwrap();
                let accepted = v[2] == "5,17,1";
                assert_eq!(
                    request.action,
                    CraftingAction::Confirmation {
                        confirmation_type: 5,
                        context: 17,
                        accepted
                    }
                );
                assert_eq!(request.trailing_bytes, 1);
                None
            }
            name => panic!("unknown fixture {name}"),
        };
        if let Some(event) = event {
            assert_eq!(
                event.encode(0x50000001, 9, 4096).unwrap(),
                bytes,
                "{}",
                v[0]
            );
        }
        count += 1;
    }
    assert_eq!(count, 11);
}
#[test]
fn malformed_counts_and_truncation_are_rejected_before_allocation() {
    for count in [301, u32::MAX] {
        let bytes = [2u32.to_le_bytes(), count.to_le_bytes()].concat();
        assert_eq!(
            CraftingRequest::decode(Action::CreateTinkeringTool, &bytes, 4096),
            Err(WireError::LimitExceeded)
        );
    }
    let bytes = [2u32.to_le_bytes(), 300u32.to_le_bytes()].concat();
    assert_eq!(
        CraftingRequest::decode(Action::CreateTinkeringTool, &bytes, 4096),
        Err(WireError::Truncated)
    );
    for length in 0..12 {
        assert!(
            CraftingRequest::decode(Action::ConfirmationResponse, &vec![0; length], 4096).is_err()
        );
    }
    assert_eq!(
        CraftingRequest::decode(Action::CreateTinkeringTool, &[0; 8], 7),
        Err(WireError::LimitExceeded)
    );
    let bytes = [2u32.to_le_bytes(), 3u32.to_le_bytes()].concat();
    assert_eq!(
        CraftingRequest::decode(Action::UseWithTarget, &bytes, 8)
            .unwrap()
            .action,
        CraftingAction::UseWithTarget {
            source_id: 2,
            target_id: 3
        }
    );
}
#[test]
fn unsuitable_only_emits_empty_result_and_no_message_can_contain_multiple_results() {
    let event = CraftingEvent::SalvageResult {
        skill: 40,
        unsuitable: &[2, 3],
        result: None,
        augmentation_bonus: 0,
    };
    let bytes = event.encode(1, 2, 4096).unwrap();
    let envelope = GameEventEnvelope::decode(&bytes, 4096).unwrap();
    assert_eq!(envelope.event.0, 0x2b4);
    let mut r = Reader::new(envelope.payload);
    assert_eq!(r.u32().unwrap(), 40);
    assert_eq!(r.u32().unwrap(), 2);
    assert_eq!(r.u32().unwrap(), 2);
    assert_eq!(r.u32().unwrap(), 3);
    assert_eq!(r.u32().unwrap(), 0);
    assert_eq!(r.u32().unwrap(), 0);
    assert_eq!(r.remaining(), 0);
    assert_eq!(
        event.encode(1, 2, bytes.len() - 1),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        CraftingEvent::SalvageResult {
            skill: 40,
            unsuitable: &[1; 301],
            result: None,
            augmentation_bonus: 0
        }
        .encode(1, 2, 4096),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        CraftingEvent::SalvageResult {
            skill: 40,
            unsuitable: &[],
            result: Some(SalvageWireResult {
                material: 61,
                workmanship: f64::NAN,
                units: 1
            }),
            augmentation_bonus: 0
        }
        .encode(1, 2, 4096),
        Err(WireError::InvalidEncoding)
    );
}
