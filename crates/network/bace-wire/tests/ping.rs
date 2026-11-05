use bace_wire::{GameEventEnvelope, SimpleGameEvent, WireError, opcode::GameEventType};

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
fn original_ping_response_packets_match_exactly() {
    let mut count = 0;
    for row in include_str!("fixtures/ping.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = row.split(',').collect();
        assert_eq!(fields.len(), 4, "malformed fixture row: {row}");
        let object_id = u32::from_str_radix(fields[1], 16).unwrap();
        let sequence: u32 = fields[2].parse().unwrap();
        let bytes = SimpleGameEvent::PingResponse.encode(object_id, sequence);
        assert_eq!(to_hex(&bytes), fields[3], "{}", fields[0]);
        count += 1;
    }
    assert_eq!(count, 2);
}

#[test]
fn truncated_ping_response_is_rejected() {
    let bytes = from_hex("B0F70000010000502A000000EA010000");
    assert!(GameEventEnvelope::decode(&bytes, 0).is_ok());
    for length in 0..bytes.len() {
        assert!(
            GameEventEnvelope::decode(&bytes[..length], 0).is_err(),
            "truncated at {length} accepted"
        );
    }
    // The event word is read from the header, so a popup event is not a ping.
    let mut popup = bytes.clone();
    popup[12..16].copy_from_slice(&0x0004u32.to_le_bytes());
    assert_eq!(
        GameEventEnvelope::decode(&popup, 0).map(|e| e.event),
        Ok(GameEventType::PopupString)
    );
    let mut wrong_opcode = bytes;
    wrong_opcode[0] = 0x00;
    assert!(matches!(
        GameEventEnvelope::decode(&wrong_opcode, 0),
        Err(WireError::UnexpectedOpcode(_))
    ));
}
