/// Official ACE Hash32: wrapping little-endian words and high-byte trailing bytes.
pub fn hash32(data: &[u8]) -> u32 {
    let mut result = (data.len() as u32).wrapping_shl(16);
    let mut chunks = data.chunks_exact(4);
    for word in &mut chunks {
        result = result.wrapping_add(u32::from_le_bytes([word[0], word[1], word[2], word[3]]));
    }
    for (index, byte) in chunks.remainder().iter().enumerate() {
        result = result.wrapping_add(u32::from(*byte) << (24 - index * 8));
    }
    result
}
