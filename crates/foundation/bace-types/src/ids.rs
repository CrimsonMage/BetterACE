use serde::{Deserialize, Serialize};

macro_rules! identifier {
    ($name:ident, $width:ty) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $width);
    };
}

identifier!(EntityId, u32);
identifier!(WeenieId, u32);
identifier!(CellId, u32);
identifier!(AccountId, u64);
