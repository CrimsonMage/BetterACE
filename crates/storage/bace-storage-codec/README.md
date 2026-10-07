# bace-storage-codec

Also implemented: BACE version-1 immutable indexed `.bace` packs, streaming compilation, read-only mmap, bounded lazy integrity checks/scans, retained borrowed handles, base/delta/tombstone overlays and immutable manifests. See [pack-format.md](../../../../docs/pack-format.md) for layout, operating requirements and the sole audited mapping exception.

Pack payloads are currently independently bounded opaque bytes. Complete typed world views, DAT geometry packing and gameplay cache integration remain separate work. Cold mapped reads MUST run off the simulation thread. Immutable identical publication retries verify complete bytes with bounded buffers; the application owns durable manifest acceptance and generation activation.

Implemented: generic frozen-DTO postcard encoding/decoding with bounded output and a checked envelope. Default payload cap is 16 MiB; callers must also validate domain limits before accepting decoded state.

V1 envelope is exactly: `ACERBIN\0` (8 bytes), envelope version/kind/schema/flags (four little-endian u16), payload length (little-endian u32), SHA256 (32 bytes), postcard payload. Header length is 52 bytes. Digest covers the first 20 header bytes plus payload. It detects accidental corruption; it does not authenticate a writer.

`inspect` validates magic, envelope version, flags, length, limit and digest. `decode` additionally validates aggregate kind/schema and rejects trailing postcard bytes. Unsupported versions fail explicitly. Domain DTOs must remain frozen; gameplay structs are not storage schemas. Future migrations require explicit old/new DTOs and tests.
