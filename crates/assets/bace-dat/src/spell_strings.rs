use crate::{DatError, table_reader::TableReader};
pub(crate) fn obfuscated(reader: &mut TableReader<'_>) -> Result<(String, u32), DatError> {
    let count = usize::from(reader.u16()?);
    if count > reader.limits.max_string_bytes {
        return Err(DatError::Format("spell string limit"));
    }
    let bytes: Vec<u8> = reader
        .take(count)?
        .iter()
        .map(|b| b.rotate_right(4))
        .collect();
    reader.align()?;
    let mut output = String::with_capacity(count);
    for b in &bytes {
        let c = match *b {
            0x80..=0x9f => [
                0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160,
                0x2039, 0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022,
                0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
            ][usize::from(*b - 0x80)],
            _ => u32::from(*b),
        };
        output.push(char::from_u32(c).expect("fixed CP1252 table"));
    }
    Ok((output, spell_hash_cp1252(&bytes)))
}
/// ACE SpellTable.ComputeHash takes signed CP1252 bytes, not UTF-8 bytes.
pub fn spell_hash_cp1252(bytes: &[u8]) -> u32 {
    let mut result = 0_i64;
    for b in bytes {
        result = i64::from(*b as i8).wrapping_add(result.wrapping_shl(4));
        if result & 0xf0000000 != 0 {
            result = (result ^ ((result & 0xf0000000) >> 24)) & 0x0fffffff;
        }
    }
    result as u32
}
