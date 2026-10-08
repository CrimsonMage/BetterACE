//! Pinned ACE UI action layouts. Parse completely before domain mutation.
use crate::{Reader, WireError, opcode::GameActionType as Op};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiShortcut {
    pub index: u32,
    pub object: u32,
    pub spell: u16,
    pub layer: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiOptions {
    pub options1: u32,
    pub options2: Option<u32>,
    pub filters: u32,
    pub shortcuts: Option<Vec<UiShortcut>>,
    pub bars: Vec<Vec<u32>>,
    pub components: Option<Vec<(u32, i32)>>,
    pub gameplay: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    Options(UiOptions),
    SingleOption { option: u32, enabled: bool },
    AddShortcut(UiShortcut),
    RemoveShortcut(u32),
    AddFavorite { spell: u32, position: u32, bar: u32 },
    RemoveFavorite { spell: u32, bar: u32 },
    Filters(u32),
    Component { template: u32, quantity: i32 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiInput {
    pub action: UiAction,
    pub trailing_bytes: usize,
}
impl UiInput {
    pub fn decode(op: Op, payload: &[u8], max_bytes: usize) -> Result<Self, WireError> {
        if payload.len() > max_bytes || payload.len() > 131072 {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match op {
            Op::SetCharacterOptions => UiAction::Options(options(&mut r)?),
            Op::SetSingleCharacterOption => UiAction::SingleOption {
                option: r.u32()?,
                enabled: r.u32()? != 0,
            },
            Op::AddShortCut => UiAction::AddShortcut(shortcut(&mut r)?),
            Op::RemoveShortCut => UiAction::RemoveShortcut(r.u32()?),
            Op::AddSpellFavorite => UiAction::AddFavorite {
                spell: r.u32()?,
                position: r.u32()?,
                bar: r.u32()?,
            },
            Op::RemoveSpellFavorite => UiAction::RemoveFavorite {
                spell: r.u32()?,
                bar: r.u32()?,
            },
            Op::SpellbookFilter => UiAction::Filters(r.u32()?),
            Op::SetDesiredComponentLevel => UiAction::Component {
                template: r.u32()?,
                quantity: r.u32()? as i32,
            },
            _ => return Err(WireError::InvalidEncoding),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}
fn count(r: &mut Reader<'_>, maximum: usize, width: usize) -> Result<usize, WireError> {
    let n = r.u32()? as usize;
    if n > maximum {
        return Err(WireError::LimitExceeded);
    }
    if n > r.remaining() / width {
        return Err(WireError::Truncated);
    }
    Ok(n)
}
fn shortcut(r: &mut Reader<'_>) -> Result<UiShortcut, WireError> {
    Ok(UiShortcut {
        index: r.u32()?,
        object: r.u32()?,
        spell: r.u16()?,
        layer: r.u16()?,
    })
}
fn options(r: &mut Reader<'_>) -> Result<UiOptions, WireError> {
    let flags = r.u32()?;
    // Timestamp/GenericQualities have no implemented authoritative projection.
    // Refuse them before changing any options rather than silently discarding them.
    if flags & !0x67d != 0 {
        return Err(WireError::InvalidEncoding);
    }
    let options1 = r.u32()?;
    let shortcuts = if flags & 1 != 0 {
        let n = count(r, 18, 12)?;
        Some((0..n).map(|_| shortcut(r)).collect::<Result<_, _>>()?)
    } else {
        None
    };
    let bar_count = 1
        + if flags & 4 != 0 { 4 } else { 0 }
        + if flags & 16 != 0 { 6 } else { 0 }
        + if flags & 1024 != 0 { 7 } else { 0 };
    if bar_count > 8 {
        return Err(WireError::InvalidEncoding);
    }
    let mut bars = Vec::with_capacity(bar_count);
    for _ in 0..bar_count {
        let n = count(r, 4096, 4)?;
        bars.push((0..n).map(|_| r.u32()).collect::<Result<_, _>>()?);
    }
    let components = if flags & 8 != 0 {
        let n = (r.u32()? & 0xffff) as usize;
        if n > 4096 {
            return Err(WireError::LimitExceeded);
        }
        if n > r.remaining() / 8 {
            return Err(WireError::Truncated);
        }
        Some(
            (0..n)
                .map(|_| Ok((r.u32()?, r.u32()? as i32)))
                .collect::<Result<_, WireError>>()?,
        )
    } else {
        None
    };
    let filters = if flags & 32 != 0 { r.u32()? } else { 0x3fff };
    let options2 = if flags & 64 != 0 {
        Some(r.u32()?)
    } else {
        None
    };
    let gameplay = if flags & 512 != 0 {
        if r.remaining() > 65536 {
            return Err(WireError::LimitExceeded);
        }
        Some(r.take(r.remaining())?.to_vec())
    } else {
        None
    };
    Ok(UiOptions {
        options1,
        options2,
        filters,
        shortcuts,
        bars,
        components,
        gameplay,
    })
}
