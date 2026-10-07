//! Bounded, integrity-checked storage envelopes for frozen DTO schemas.
mod bounded_output;
mod envelope;
mod mapping;
mod pack_format;
mod pack_generation;
mod pack_manifest;
mod pack_reader;
mod pack_writer;

pub use envelope::{CodecError, CodecLimits, EnvelopeInfo, decode, encode, inspect};
pub use pack_format::{PackDescriptor, PackError, PackKey, PackLimits, PackRecord};
pub use pack_generation::PackGeneration;
pub use pack_manifest::{PackManifest, load_manifest, write_manifest};
pub use pack_reader::{MappedPack, PackLookup, RecordHandle};
pub use pack_writer::compile_pack;
