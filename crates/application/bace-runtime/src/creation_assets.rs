//! Immutable cold creation inputs and source UTC birth-date formatting. Name
//! approval and geometry admission precede freezing; persistence still owns success.
use std::sync::Arc;
pub struct PreparedCreationClosure {
    pub content_generation: u64,
    pub character: Arc<crate::character_assets::PreparedCharacterAssets>,
    pub creation: Arc<bace_character::PreparedCreationAssets>,
    pub names: Arc<bace_character::NamePolicy>,
}
/// ACE Player constructor formats DateOfBirth as `dd MMMM yyyy` from UTC.
/// Explicit English month names keep this server field CP1252-representable.
pub fn creation_birth_date(unix_millis: u64) -> Result<String, String> {
    if unix_millis > 253_402_300_799_999 {
        return Err("creation UTC clock outside supported Gregorian range".into());
    }
    let days = i64::try_from(unix_millis / 86_400_000).map_err(|_| "creation UTC day overflow")?;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let name = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(month - 1) as usize];
    Ok(format!("{day:02} {name} {year:04}"))
}
/// Mutates only the still-unpublished proposal, after trusted clock validation.
pub fn stamp_creation_birth(
    prepared: &mut bace_character::PreparedCharacter,
    unix_millis: u64,
) -> Result<(), String> {
    let value = creation_birth_date(unix_millis)?;
    let fields = &mut prepared.state.properties.strings;
    if let Some(existing) = fields.iter_mut().find(|p| p.id == 43) {
        existing.value = value;
    } else {
        fields.push(bace_content::Property { id: 43, value });
        fields.sort_by_key(|p| p.id);
    }
    Ok(())
}
/// Pinned CharacterGenerationVerificationResponse distinguishes invalid names,
/// malformed selections and unavailable server assets before durable creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationFailure {
    pub response: u32,
    pub message: String,
}
impl CreationFailure {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            response: 6,
            message: message.into(),
        }
    }
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self {
            response: 5,
            message: message.into(),
        }
    }
}
