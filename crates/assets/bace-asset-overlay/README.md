# bace-asset-overlay

Owns pure, checked application of native immutable content patches to decoded
DAT assets. It never reads or modifies DAT files or `.bace` packs. Callers load
and fingerprint approved assets before invoking these functions. Content Studio
uses this resolver for offline inspection. Runtime motion preparation does not
yet consume these patches, so live publication is unsupported.

`resolve_animation_swaps` applies a schema-1 `AnimationSwapPatchV1` to a clone
of one decoded animation. Edits are ordered and use zero-based frame and hook
indices in the currently resolved view. Only `ReplaceObject` hooks may be
replaced or removed; inserts create that hook type. Any invalid edit rejects
the entire patch without changing the source animation.
