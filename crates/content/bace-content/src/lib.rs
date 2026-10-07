//! Frozen typed content DTOs and atomically replaceable immutable catalogs.
mod bounded;
mod catalog;
mod clothing;
mod model;
mod properties;
mod validation;

pub use catalog::{Catalog, CatalogSnapshot, Publication};
pub use clothing::*;
pub use model::{Property, SparseProperties, WeenieTemplate, WeenieV1};
pub use properties::*;
pub use validation::{ContentError, ContentLimits};
