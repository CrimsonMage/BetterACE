//! The sole unsafe boundary: read-only mappings of application-owned immutable
//! pack generations. Never expose mutable maps or map arbitrary shared targets.
use std::fs::File;

pub(crate) fn open_immutable(file: &File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY: The pack store only publishes new, uniquely content-addressed
    // files after closing all writable handles. It never modifies or truncates
    // a published generation. The operator must preserve this immutability
    // across processes as documented in pack-format.md. File lifetime is kept
    // by the OS mapping; all borrowed views retain its owning Arc. No mutable
    // map, remapping, raw-pointer cast or unchecked byte access is exposed.
    #[allow(unsafe_code)]
    let map = unsafe { memmap2::MmapOptions::new().map(file)? };
    Ok(map)
}
