//! Character-local preferences; IDs and amounts from clients remain untrusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiShortcut {
    pub index: u32,
    pub object: u32,
    pub spell: u16,
    pub layer: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterUi {
    pub options1: u32,
    pub options2: u32,
    pub filters: u32,
    pub shortcuts: Vec<UiShortcut>,
    pub bars: [Vec<u32>; 8],
    pub components: Vec<(u32, u32)>,
    pub gameplay: Vec<u8>,
}
impl Default for CharacterUi {
    fn default() -> Self {
        Self {
            options1: 0,
            options2: 0,
            filters: 0x3fff,
            shortcuts: Vec::new(),
            bars: Default::default(),
            components: Vec::new(),
            gameplay: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    Options {
        options1: u32,
        options2: Option<u32>,
        filters: u32,
        shortcuts: Option<Vec<UiShortcut>>,
        bars: Vec<Vec<u32>>,
        components: Option<Vec<(u32, i32)>>,
        gameplay: Option<Vec<u8>>,
    },
    SingleOption {
        group: u8,
        mask: u32,
        enabled: bool,
    },
    AddShortcut(UiShortcut),
    RemoveShortcut(u32),
    AddFavorite {
        spell: u32,
        bar: u32,
        position: u32,
    },
    RemoveFavorite {
        spell: u32,
        bar: u32,
    },
    Filters(u32),
    Component {
        template: u32,
        quantity: i32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    Invalid,
    Capacity,
    UnknownSpell,
    UnknownComponent,
    BeforeEntry,
    RevisionExhausted,
    Ownership,
    DurabilityPending,
}
