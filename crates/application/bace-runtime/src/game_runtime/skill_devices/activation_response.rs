//! Source-limited skill-device activation response. Pinned ACE
//! WorldObject_Use.OnActivate invokes ActOnUse before OnTalk, and OnTalk sends
//! GameMessageSystemChat with ChatMessageType.Broadcast (0).
use bace_content::WeenieV1;

pub(super) fn inactive(source: &WeenieV1) -> bool {
    source
        .properties
        .ints
        .iter()
        .find(|property| property.id == 119)
        .is_some_and(|property| property.value == 0)
}

pub(super) fn talk(source: &WeenieV1, max_bytes: usize) -> Result<Option<String>, String> {
    const USE: i32 = 0x2;
    const TALK: i32 = 0x10;
    let flags = source
        .properties
        .ints
        .iter()
        .find(|property| property.id == 83)
        .map_or(USE, |property| property.value);
    if flags != USE && flags != USE | TALK {
        return Err("skill device activation response requires separate effect owner".into());
    }
    if flags == USE {
        return Ok(None);
    }
    let text = source
        .properties
        .strings
        .iter()
        .find(|property| property.id == 17)
        .ok_or("skill device ActivationTalk missing")?
        .value
        .as_str();
    if text.len() > max_bytes.min(4096) || text.len() > u16::MAX as usize {
        return Err("skill device ActivationTalk exceeds message bound".into());
    }
    Ok(Some(text.to_owned()))
}
