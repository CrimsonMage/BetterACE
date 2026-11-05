//! Book and inscription GameAction readers: BookData (0xAA), BookModifyPage (0xAB),
//! BookAddPage (0xAC), BookDeletePage (0xAD), BookPageData (0xAE), SetInscription (0xBF).
//!
//! Layout sources, in order of authority for this family:
//! - Retail senders, `CM_Writing.cpp`:
//!   - `Event_BookData` writes 170 (0xAA): u32 book.
//!   - `Event_BookAddPage` writes 172 (0xAC): u32 book.
//!   - `Event_BookDeletePage` writes 173 (0xAD): u32 book, i32 page.
//!   - `Event_BookPageData` writes 174 (0xAE): u32 book, i32 page.
//!   - `Event_BookModifyPage` writes 171 (0xAB): u32 book, i32 page, PString16 text.
//!   - `Event_SetInscription` writes 191 (0xBF): u32 object, PString16 text.
//! - ACE `GameAction/Actions/GameActionBook*.cs` and `GameActionSetInscription.cs` at
//!   47edade3bd3f6044b676d4eb877c4965c7eda62b. Opcode names agree with retail.
//!
//! GDLE `PhatSDK/GameEnums.h` labels 0xAA as BOOK_ADD_PAGE and 0xAC as BOOK_DATA. That is
//! reversed relative to both retail and ACE, so it is not followed here.
//!
//! Text: retail `AC1Legacy::PStringBase<char>::Pack` writes a u16 byte count, the raw bytes,
//! then pads the total to 4. Those bytes are decoded as Windows-1252, which matches ACE's own
//! writer `Extensions.WriteString16L`. ACE's reader `ReadString16L` interprets the bytes as
//! UTF-8, which disagrees with retail for bytes >= 0x80. Retail's extended form (u16 0xFFFF then
//! u32 length, for 65535+ bytes) is not accepted: the caller's `max_text` must be below 65535.
//!
//! Page indices are retail `int` values. Negative indices are rejected at decode.
//! No client-enforced text limit exists in the decompile; ACE limits page text per book
//! (`MaxNumCharsPerPage`, content data) and does not limit inscriptions. Both are bounded here
//! by the caller-supplied `max_text`, which the runtime should set to the same 4096 cap used for
//! social text in `game_runtime/social.rs`.
use crate::{Reader, WireError, opcode::GameActionType as Op};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookAction {
    BookData {
        book: u32,
    },
    BookModifyPage {
        book: u32,
        page: u32,
        text: String,
    },
    BookAddPage {
        book: u32,
    },
    BookDeletePage {
        book: u32,
        page: u32,
    },
    BookPageData {
        book: u32,
        page: u32,
    },
    /// Retail `Event_SetInscription`; the object is the item or book being inscribed.
    SetInscription {
        object: u32,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookRequest {
    pub action: BookAction,
    pub trailing_bytes: usize,
}

impl BookRequest {
    /// `max_text` bounds page text and inscriptions in bytes and must be below 65535.
    pub fn decode(
        op: Op,
        payload: &[u8],
        max_payload: usize,
        max_text: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match op {
            Op::BookData => BookAction::BookData { book: r.u32()? },
            Op::BookModifyPage => {
                let book = r.u32()?;
                let page = page_index(&mut r)?;
                let text = r.string16(max_text)?;
                BookAction::BookModifyPage { book, page, text }
            }
            Op::BookAddPage => BookAction::BookAddPage { book: r.u32()? },
            Op::BookDeletePage => {
                let book = r.u32()?;
                let page = page_index(&mut r)?;
                BookAction::BookDeletePage { book, page }
            }
            Op::BookPageData => {
                let book = r.u32()?;
                let page = page_index(&mut r)?;
                BookAction::BookPageData { book, page }
            }
            Op::SetInscription => {
                let object = r.u32()?;
                let text = r.string16(max_text)?;
                BookAction::SetInscription { object, text }
            }
            _ => return Err(WireError::UnexpectedOpcode(op.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}

/// Retail and ACE read a little-endian `int`; a negative value is not a valid 0-based index.
fn page_index(r: &mut Reader<'_>) -> Result<u32, WireError> {
    let value = i32::from_le_bytes(r.u32()?.to_le_bytes());
    u32::try_from(value).map_err(|_| WireError::InvalidLength)
}
