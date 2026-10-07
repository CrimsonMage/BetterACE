//! Offline desktop conversion, reusing the checked legacy import boundaries.
mod app;
mod conversion;
mod document;
mod editor;
mod editor_io;
#[cfg(test)]
mod editor_tests;
mod entry;
mod form_schema;
mod forms;
mod legacy_bundle;
mod pack_builder;
mod theme;
mod worker;

pub use conversion::{ConversionOptions, ConversionSummary, convert_file};
pub use entry::run;

mod preview;
mod preview_pixels;
mod preview_render;
mod preview_scene;

#[cfg(test)]
mod preview_tests;
