//! Account-dependent taper substitution from pinned ACE SpellTable. These hashes
//! are legacy content algorithms, not security or the gameplay random stream.
use crate::{DatError, SpellBase, spell_hash_cp1252};
impl SpellBase {
    pub fn formula_for_account(&self, account_cp1252: &[u8]) -> Result<Vec<u32>, DatError> {
        if account_cp1252.len() > 1024 || self.formula.len() > 8 {
            return Err(DatError::Format("formula limit"));
        }
        let mut c = self.formula.clone();
        let key = spell_hash_cp1252(account_cp1252);
        let seed = key % 0x13d573;
        let bad = || DatError::Format("invalid spell formula");
        match self.formula_version {
            1 => {
                if c.len() < 5 {
                    return Err(bad());
                }
                let first = c.len() > 5;
                let second = c.len() > 6;
                let third = c.len() > 7;
                let herb = if first { 2 } else { 1 };
                let powder = herb + 1 + usize::from(second);
                let potion = powder + 1;
                let talisman = potion + 1 + usize::from(third);
                let (s, h, p, o, t) = (
                    c[0],
                    c[herb],
                    *c.get(powder).ok_or_else(bad)?,
                    *c.get(potion).ok_or_else(bad)?,
                    *c.get(talisman).ok_or_else(bad)?,
                );
                if first {
                    c[1] = p
                        .wrapping_add(2_u32.wrapping_mul(h))
                        .wrapping_add(o)
                        .wrapping_add(t)
                        .wrapping_add(s)
                        % 12
                        + 63;
                }
                if second {
                    let denominator = s.wrapping_add(p.wrapping_add(o));
                    if denominator == 0 {
                        return Err(bad());
                    }
                    c[3] = s
                        .wrapping_add(h)
                        .wrapping_add(t)
                        .wrapping_add(2_u32.wrapping_mul(p.wrapping_add(o)))
                        .wrapping_mul(seed / denominator)
                        % 12
                        + 63;
                }
                if third {
                    let denominator = t.wrapping_add(s);
                    if denominator == 0 {
                        return Err(bad());
                    }
                    c[6] = p
                        .wrapping_add(2_u32.wrapping_mul(t))
                        .wrapping_add(o)
                        .wrapping_add(h)
                        .wrapping_add(s)
                        .wrapping_mul(seed / denominator)
                        % 12
                        + 63;
                }
            }
            2 => {
                if c.len() != 8 {
                    return Err(bad());
                }
                let (p, a, x, z) = (c[0], c[4], c[5], c[7]);
                let denominator = c[1].wrapping_mul(z).wrapping_add(2_u32.wrapping_mul(a));
                if denominator == 0 {
                    return Err(bad());
                }
                c[3] = z
                    .wrapping_add(2_u32.wrapping_mul(p))
                    .wrapping_add(2_u32.wrapping_mul(a).wrapping_mul(x))
                    .wrapping_add(p)
                    .wrapping_add(c[2])
                    .wrapping_add(c[1])
                    % 12
                    + 63;
                c[6] = z
                    .wrapping_add(2_u32.wrapping_mul(p).wrapping_mul(c[2]))
                    .wrapping_add(2_u32.wrapping_mul(x))
                    .wrapping_add(p.wrapping_mul(c[2]))
                    .wrapping_add(a)
                    .wrapping_mul(seed / denominator)
                    % 12
                    + 63;
            }
            3 => {
                if c.len() < 7 {
                    return Err(bad());
                }
                let a = (seed.wrapping_add(c[0])) % 12;
                let b = (key % 0x4aefd).wrapping_add(c[1]) % 12;
                let d = (key % 0x96a7f).wrapping_add(c[2]) % 12;
                let e = (key % 0x100a03).wrapping_add(c[4]) % 12;
                let f = (key % 0xeb2ef).wrapping_add(c[5]) % 12;
                let g = (key % 0x121e7d).wrapping_add(c.get(7).copied().unwrap_or(0)) % 12;
                c[3] = (a + b + d + e + f + d * f + a * b + g * (e + 1)) % 12 + 63;
                c[6] = (a
                    + b
                    + d
                    + e
                    + key % 0x65039 % 12
                    + g * (e * (a * b * d * f + 7) + 1)
                    + f
                    + 4 * a * b
                    + a * b
                    + 11 * d * f)
                    % 12
                    + 63;
            }
            _ => {}
        }
        Ok(c)
    }
}
