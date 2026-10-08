# bace-random

Pure versioned keyed random derivation and unbiased bounded sampling. Runtime
supplies secret keys; this crate never reads entropy, files, clocks or SQL.
Rare contexts contain a persistent character random identity and eligible-attempt
ordinal, never a thread ID, global kill counter or ordinary loot draw position.
Other generation uses event-local contexts with stable purpose/node identifiers.

Algorithm v1 is HMAC-SHA256 with explicit little-endian framed inputs and separate
key derivation/draw/fork labels. Draw exhaustion is an error, never biased modulo
fallback. It is a BetterACE implementation policy, not a claim about retail's RNG.

`crafting_stream(character, operation)` scopes a frozen server-owned operation
under Domain 11 and the persistent character identity. Neither another character
nor an unrelated operation can advance it. The identity/operation framing has an
independent Python HMAC golden vector. A reconstructed stream repeats the same
outcome; persistence receipts must prevent committing that operation twice.
