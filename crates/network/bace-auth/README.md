# bace-auth

Implemented fresh-account repository contracts, canonical account names, official access-level numbers, and bounded Argon2id password hashing/verification. `AccountRepository` has async-compatible `find_by_name` and atomic `create`; PostgreSQL implementation belongs exclusively to `bace-db-postgres`. A duplicate creation must preserve the existing password and privilege. Fresh accounts are Player/enabled; the first account is never automatically promoted.

`PasswordService` uses Argon2id v19, 19 MiB memory, two passes, one lane, 32-byte output and an OS-random 16-byte salt. The PHC record stores algorithm/parameters/salt/hash in one bounded string. See the [RustCrypto Argon2 0.5.3 API](https://docs.rs/argon2/0.5.3/argon2/). Passwords are nonempty and limited to 1,024 bytes. PHC parsing happens before hash work and rejects other algorithms/versions, omitted/extra parameters, invalid salt/hash lengths, and costs outside 19–64 MiB, 2–4 passes, 1–4 lanes.

Share one `PasswordService` across bounded blocking workers; its nonblocking permit gate returns `Busy` under overload. Never run these synchronous memory-hard methods on an async reactor or simulation thread. `verify_account` also rejects disabled accounts; raw `verify` is available for maintenance and does not represent account authorization. Hash and credential Debug output is redacted.

`AccountName` applies explicit locale-independent Unicode lowercase with a 50 UTF-16-code-unit limit, excluding empty names and controls. It does not trim whitespace or perform Unicode normalization. This is the native fresh-account naming policy, not a claim of complete compatibility with every legacy database collation/culture.

Tests cover unique random salts, correct/wrong passwords, PHC reloading, disabled accounts, malformed/expensive hash rejection, bounded concurrent work and account-name limits. Legacy ACE password migration, admin provisioning, bans, login attempt rate limits, unknown-account timing equalization, account recovery and production service composition remain deferred. No stock-client login server is exposed.


Native login orchestration is now available through `authenticate` and the
`PasswordWorker` port. Auto-creation uses the atomic repository create operation,
re-reads a concurrent winner, and verifies the winner's password before success.
Unknown/disabled accounts never authenticate; credential diagnostics stay redacted.
`LoginAttempts` bounds pre-authentication addresses and fixed-window attempts.
Runtime uses a fixed worker pool and bounded admission/completion queues. Native
account auto-creation defaults on and can be disabled via `[accounts]`.
These are tested contracts, not a stock-client world-readiness claim.
