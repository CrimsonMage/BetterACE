use bace_wire::{BookAction, BookRequest, WireError, opcode::GameActionType};

const MAX_PAYLOAD: usize = 1024;
const MAX_TEXT: usize = 4096;

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

fn decode(op: GameActionType, bytes: &[u8]) -> Result<BookRequest, WireError> {
    BookRequest::decode(op, bytes, MAX_PAYLOAD, MAX_TEXT)
}

/// Expected fields for each fixture row. The oracle writes the book as 0x1000,
/// the page index as 3, and the text as "Rune" (see oracle/book_generate.py).
fn expected(name: &str) -> BookAction {
    let rune = || "Rune".to_owned();
    match name {
        "BookData" => BookAction::BookData { book: 0x1000 },
        "BookModifyPage" => BookAction::BookModifyPage {
            book: 0x1000,
            page: 3,
            text: rune(),
        },
        "BookAddPage" => BookAction::BookAddPage { book: 0x1000 },
        "BookDeletePage" => BookAction::BookDeletePage {
            book: 0x1000,
            page: 3,
        },
        "BookPageData" => BookAction::BookPageData {
            book: 0x1000,
            page: 3,
        },
        "SetInscription" => BookAction::SetInscription {
            object: 0x1000,
            text: rune(),
        },
        other => panic!("no expectation for {other}"),
    }
}

fn fixture_rows() -> Vec<(&'static str, Vec<u8>, usize)> {
    include_str!("fixtures/book.csv")
        .lines()
        .filter(|l| l.starts_with("input,"))
        .map(|row| {
            let f: Vec<&str> = row.split(',').collect();
            (f[1], hex(f[2]), f[3].parse().unwrap())
        })
        .collect()
}

#[test]
fn fixture_vectors_decode_to_expected_fields() {
    let rows = fixture_rows();
    assert_eq!(
        rows.len(),
        6,
        "BookData, BookModifyPage, BookAddPage, BookDeletePage, BookPageData, SetInscription"
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
fn non_ascii_text_follows_retail_cp1252_byte_count() {
    // Retail CM_Writing::Event_SetInscription -> PStringBase::Pack: u16 byte count 2,
    // bytes E9 41 ("eA" in CP1252 is 0xE9 0x41 -> "\u{e9}A"), padded to 4 (2+2 = 4, no pad).
    let mut payload = vec![0, 0, 0, 0];
    payload.extend_from_slice(&[2, 0, 0xe9, 0x41]);
    let decoded = decode(op("SetInscription"), &payload).unwrap();
    assert_eq!(
        decoded.action,
        BookAction::SetInscription {
            object: 0,
            text: "\u{e9}A".into()
        }
    );
    assert_eq!(decoded.trailing_bytes, 0);
}

#[test]
fn text_padding_matches_retail_pack_alignment() {
    // 3 text bytes: 2 + 3 = 5, padded with 3 zero bytes to 8.
    let mut modify = vec![0x34, 0x12, 0, 0, 2, 0, 0, 0];
    modify.extend_from_slice(&[3, 0]);
    modify.extend_from_slice(b"abc");
    modify.extend_from_slice(&[0, 0, 0]);
    let decoded = decode(op("BookModifyPage"), &modify).unwrap();
    assert_eq!(
        decoded.action,
        BookAction::BookModifyPage {
            book: 0x1234,
            page: 2,
            text: "abc".into()
        }
    );
    assert_eq!(decoded.trailing_bytes, 0);
}

#[test]
fn extended_retail_text_length_is_rejected() {
    // Retail writes u16 0xFFFF then a u32 length for text of 65535 bytes or more.
    let mut payload = vec![0, 0, 0, 0];
    payload.extend_from_slice(&[0xff, 0xff, 0, 0, 0, 0]);
    assert_eq!(
        decode(op("SetInscription"), &payload),
        Err(WireError::LimitExceeded)
    );
}

#[test]
fn oversized_text_is_rejected_and_boundary_accepted() {
    let limit = 16;
    let mut over = vec![0, 0, 0, 0, 0, 0];
    over[4] = (limit + 1) as u8;
    over.extend(std::iter::repeat_n(b'A', limit + 1));
    over.extend_from_slice(&[0, 0, 0]);
    assert_eq!(
        BookRequest::decode(op("SetInscription"), &over, MAX_PAYLOAD, limit),
        Err(WireError::LimitExceeded)
    );

    // 2 + 16 = 18 bytes, padded with 2 zero bytes.
    let mut at = vec![0, 0, 0, 0, limit as u8, 0];
    at.extend(std::iter::repeat_n(b'A', limit));
    at.extend_from_slice(&[0, 0]);
    let decoded = BookRequest::decode(op("SetInscription"), &at, MAX_PAYLOAD, limit).unwrap();
    assert_eq!(
        decoded.action,
        BookAction::SetInscription {
            object: 0,
            text: "A".repeat(limit)
        }
    );
}

#[test]
fn negative_page_index_is_rejected() {
    let mut payload = vec![0, 0, 0, 0];
    payload.extend_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(
        decode(op("BookPageData"), &payload),
        Err(WireError::InvalidLength)
    );
    assert_eq!(
        decode(op("BookDeletePage"), &payload),
        Err(WireError::InvalidLength)
    );
}

#[test]
fn arbitrary_text_bytes_decode_without_panic() {
    // Windows-1252 maps every byte, so these bytes decode as text rather than panicking.
    let mut payload = vec![0, 0, 0, 0, 2, 0, 0x81, 0x9d];
    payload.extend_from_slice(&[0, 0]);
    assert!(decode(op("SetInscription"), &payload).is_ok());
}

#[test]
fn oversized_payload_is_rejected() {
    let bytes = hex("0010000003000000040052756E650000");
    assert_eq!(
        BookRequest::decode(op("BookModifyPage"), &bytes, 8, MAX_TEXT),
        Err(WireError::LimitExceeded)
    );
}

#[test]
fn opcodes_outside_this_family_are_not_decoded_here() {
    for name in ["BuyHouse", "SetMotd", "FellowshipCreate"] {
        assert_eq!(
            decode(op(name), &[]),
            Err(WireError::UnexpectedOpcode(op(name).0)),
            "{name}"
        );
    }
}
