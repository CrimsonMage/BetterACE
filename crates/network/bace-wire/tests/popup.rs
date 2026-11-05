use bace_wire::{PopupString, WireError};

const OBJECT_ID: u32 = 0x5000_0001;
const SEQUENCE: u32 = 42;

struct Row<'a> {
    name: &'a str,
    unit: &'a str,
    repeat: usize,
    /// Full ACE packet hex; empty for summarised rows.
    packet: &'a str,
    /// Exact ACE packet length for summarised rows.
    packet_len: Option<usize>,
    head64: &'a str,
    tail64: &'a str,
}

fn rows() -> Vec<Row<'static>> {
    include_str!("fixtures/popup.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), 7, "malformed fixture row: {line}");
            Row {
                name: fields[0],
                unit: fields[1],
                repeat: fields[2].parse().unwrap(),
                packet: fields[3],
                packet_len: (!fields[4].is_empty()).then(|| fields[4].parse().unwrap()),
                head64: fields[5],
                tail64: fields[6],
            }
        })
        .collect()
}

fn from_hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

#[test]
fn original_popup_packets_match_exactly() {
    let mut count = 0;
    for row in rows() {
        // Rejected and summarised rows have their own tests below.
        if row.name == "unmappable_substitution" || row.packet.is_empty() {
            continue;
        }
        let unit = String::from_utf8(from_hex(row.unit)).unwrap();
        let text = unit.repeat(row.repeat);
        let bytes = PopupString { message: &text }
            .encode(OBJECT_ID, SEQUENCE)
            .unwrap_or_else(|error| panic!("{}: {error:?}", row.name));
        assert_eq!(to_hex(&bytes), row.packet, "{}", row.name);
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn maximum_length_popup_matches_ace_length_head_and_tail() {
    // The 65535-byte message is built here, not stored in the fixture. The
    // workspace has no sha2 dependency for bace-wire, so ACE's exact length and
    // its first and last 64 bytes are the recorded evidence.
    let row = rows()
        .into_iter()
        .find(|row| row.name == "max_length")
        .unwrap();
    assert!(row.packet.is_empty());
    let unit = String::from_utf8(from_hex(row.unit)).unwrap();
    let text = unit.repeat(row.repeat);
    assert_eq!(text.len(), 65_535);
    let bytes = PopupString { message: &text }
        .encode(OBJECT_ID, SEQUENCE)
        .unwrap();
    assert_eq!(Some(bytes.len()), row.packet_len);
    assert_eq!(bytes.len(), 65_556);
    assert_eq!(to_hex(&bytes[..64]), row.head64);
    assert_eq!(to_hex(&bytes[bytes.len() - 64..]), row.tail64);
}

#[test]
fn over_long_popup_is_rejected_and_maximum_is_accepted() {
    let maximum = "A".repeat(65_535);
    assert!(
        PopupString { message: &maximum }
            .encode(OBJECT_ID, SEQUENCE)
            .is_ok()
    );
    let too_long = "A".repeat(65_536);
    assert_eq!(
        PopupString { message: &too_long }.encode(OBJECT_ID, SEQUENCE),
        Err(WireError::InvalidLength)
    );
}

#[test]
fn unmappable_character_is_rejected_where_ace_substitutes() {
    // Deliberate deviation: ACE writes '?' (0x3F) for U+4E2D. The pinned
    // oracle row records that substitution; this encoder must not emit it.
    assert_eq!(
        PopupString {
            message: "\u{4E2D}"
        }
        .encode(OBJECT_ID, SEQUENCE),
        Err(WireError::InvalidEncoding)
    );
    let row = rows()
        .into_iter()
        .find(|row| row.name == "unmappable_substitution")
        .unwrap();
    // 16-byte header (opcode, guid, sequence, event), then length 1, byte '?', pad.
    assert_eq!(&row.packet[32..], "01003F00");
}

#[test]
fn windows_1252_character_uses_single_byte() {
    let bytes = PopupString { message: "\u{E9}" }
        .encode(OBJECT_ID, SEQUENCE)
        .unwrap();
    assert_eq!(&bytes[16..], &[0x01, 0x00, 0xE9, 0x00]);
}
