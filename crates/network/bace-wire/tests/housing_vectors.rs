use bace_wire::{HousingAction, HousingRequest, WireError, opcode::GameActionType};

const MAX_PAYLOAD: usize = 1024;
const MAX_STRING: usize = 64;
const MAX_ITEMS: usize = 16;

fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

fn op(name: &str) -> GameActionType {
    GameActionType::NAMED
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("unknown ACE action {name}"))
        .1
}

/// Expected fields for each fixture row. The fixture writes scalars as 0x1000+k,
/// the string as "Rune", and lists as [0x2000, 0x2001] (see oracle/housing_generate.py).
fn expected(name: &str) -> HousingAction {
    let items = vec![0x2000, 0x2001];
    let rune = || "Rune".to_owned();
    match name {
        "BuyHouse" => HousingAction::BuyHouse {
            slumlord: 0x1000,
            items,
        },
        "HouseQuery" => HousingAction::HouseQuery,
        "AbandonHouse" => HousingAction::AbandonHouse,
        "RentHouse" => HousingAction::RentHouse {
            slumlord: 0x1000,
            items,
        },
        "AddPermanentGuest" => HousingAction::AddPermanentGuest(rune()),
        "RemovePermanentGuest" => HousingAction::RemovePermanentGuest(rune()),
        "SetOpenHouseStatus" => HousingAction::SetOpenHouseStatus(true),
        "ChangeStoragePermission" => HousingAction::ChangeStoragePermission {
            name: rune(),
            enabled: true,
        },
        "BootSpecificHouseGuest" => HousingAction::BootSpecificHouseGuest(rune()),
        "RemoveAllStoragePermission" => HousingAction::RemoveAllStoragePermission,
        "RequestFullGuestList" => HousingAction::RequestFullGuestList,
        "QueryLord" => HousingAction::QueryLord(0x1000),
        "AddAllStoragePermission" => HousingAction::AddAllStoragePermission,
        "RemoveAllPermanentGuests" => HousingAction::RemoveAllPermanentGuests,
        "BootEveryone" => HousingAction::BootEveryone,
        "SetHooksVisibility" => HousingAction::SetHooksVisibility(true),
        "ModifyAllegianceGuestPermission" => HousingAction::ModifyAllegianceGuestPermission(true),
        "ModifyAllegianceStoragePermission" => {
            HousingAction::ModifyAllegianceStoragePermission(true)
        }
        "ListAvailableHouses" => HousingAction::ListAvailableHouses(0x1000),
        other => panic!("no expectation for {other}"),
    }
}

fn fixture_rows() -> Vec<(&'static str, Vec<u8>, usize)> {
    include_str!("fixtures/housing.csv")
        .lines()
        .filter(|l| l.starts_with("input,"))
        .map(|row| {
            let f: Vec<&str> = row.split(',').collect();
            (f[1], hex(f[2]), f[3].parse().unwrap())
        })
        .collect()
}

fn decode(op: GameActionType, bytes: &[u8]) -> Result<HousingRequest, WireError> {
    HousingRequest::decode(op, bytes, MAX_PAYLOAD, MAX_STRING, MAX_ITEMS)
}

#[test]
fn fixture_vectors_decode_to_expected_fields() {
    let rows = fixture_rows();
    assert_eq!(
        rows.len(),
        19,
        "one row per housing opcode without a prior decoder"
    );
    for (name, bytes, consumed) in &rows {
        let decoded = decode(op(name), bytes).unwrap();
        assert_eq!(decoded.action, expected(name), "{name}");
        assert_eq!(bytes.len() - decoded.trailing_bytes, *consumed, "{name}");
    }
}

#[test]
fn truncated_input_is_rejected_and_trailing_bytes_are_reported() {
    for (name, bytes, _) in fixture_rows() {
        for n in 0..bytes.len() {
            assert!(decode(op(name), &bytes[..n]).is_err(), "{name} prefix {n}");
        }
        let mut extra = bytes.clone();
        extra.extend_from_slice(&[1, 2]);
        assert_eq!(
            decode(op(name), &extra).unwrap().trailing_bytes,
            2,
            "{name} trailing"
        );
    }
}

#[test]
fn zero_flags_decode_to_false() {
    let open = hex("00000000");
    assert_eq!(
        decode(op("SetOpenHouseStatus"), &open).unwrap().action,
        HousingAction::SetOpenHouseStatus(false)
    );
    // Length-prefixed "Rune" with its 2-byte pad, then a zero u32 flag.
    let storage = hex("040052756E65000000000000");
    assert_eq!(
        decode(op("ChangeStoragePermission"), &storage)
            .unwrap()
            .action,
        HousingAction::ChangeStoragePermission {
            name: "Rune".into(),
            enabled: false
        }
    );
}

#[test]
fn oversized_item_count_is_rejected() {
    // BuyHouse: slumlord, then a count above MAX_ITEMS with enough bytes present.
    let mut over = vec![0, 0, 0, 0];
    over.extend_from_slice(&17u32.to_le_bytes());
    over.extend(std::iter::repeat_n(0u8, 17 * 4));
    for name in ["BuyHouse", "RentHouse"] {
        assert_eq!(
            decode(op(name), &over),
            Err(WireError::LimitExceeded),
            "{name} count 17"
        );
    }
    // Maximal count with almost no payload: rejected by the bound before any allocation.
    let mut huge = vec![0, 0, 0, 0];
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    huge.extend_from_slice(&[0, 0, 0, 0]);
    assert_eq!(decode(op("BuyHouse"), &huge), Err(WireError::LimitExceeded));
    // Count within bound but items missing.
    let mut short = vec![0, 0, 0, 0];
    short.extend_from_slice(&3u32.to_le_bytes());
    short.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(decode(op("RentHouse"), &short), Err(WireError::Truncated));
}

#[test]
fn item_count_at_limit_is_accepted() {
    let mut at_limit = vec![0, 0, 0, 0];
    at_limit.extend_from_slice(&16u32.to_le_bytes());
    at_limit.extend(std::iter::repeat_n(7u8, 16 * 4));
    let decoded = decode(op("BuyHouse"), &at_limit).unwrap();
    match decoded.action {
        HousingAction::BuyHouse { items, .. } => assert_eq!(items.len(), 16),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn oversized_string_is_rejected_and_boundary_accepted() {
    let mut over = vec![65u8, 0];
    over.extend(std::iter::repeat_n(b'A', 65));
    over.push(0);
    assert_eq!(
        decode(op("AddPermanentGuest"), &over),
        Err(WireError::LimitExceeded)
    );

    let mut at_limit = vec![64u8, 0];
    at_limit.extend(std::iter::repeat_n(b'A', 64));
    at_limit.extend_from_slice(&[0, 0]);
    let decoded = decode(op("AddPermanentGuest"), &at_limit).unwrap();
    assert_eq!(
        decoded.action,
        HousingAction::AddPermanentGuest("A".repeat(64))
    );
    assert_eq!(decoded.trailing_bytes, 0);
}

#[test]
fn invalid_string_bytes_are_rejected() {
    let bad = [1u8, 0, 0xff, 0];
    assert_eq!(
        decode(op("BootSpecificHouseGuest"), &bad),
        Err(WireError::InvalidEncoding)
    );
}

#[test]
fn oversized_payload_is_rejected() {
    let bytes = hex("00100000020000000020000001200000");
    assert_eq!(
        HousingRequest::decode(op("BuyHouse"), &bytes, 8, MAX_STRING, MAX_ITEMS),
        Err(WireError::LimitExceeded)
    );
}

#[test]
fn opcodes_outside_this_family_are_not_decoded_here() {
    // Motd and DoAllegianceHouseAction are owned by group_input.
    for name in [
        "SetMotd",
        "QueryMotd",
        "ClearMotd",
        "DoAllegianceHouseAction",
        "FellowshipCreate",
    ] {
        assert_eq!(
            decode(op(name), &[]),
            Err(WireError::UnexpectedOpcode(op(name).0)),
            "{name}"
        );
    }
}
