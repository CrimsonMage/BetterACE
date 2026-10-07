# BACE host console

The default server command starts a localhost dashboard and a separate supervised
foundation child. The dashboard survives child restart and can reconnect after a
supervisor restart. **The foundation child opens no game socket, world or database;
stock-client game serving remains unsupported.** Restarting it is a process
lifecycle test, not evidence of a playable game restart or integrated save drain.

Provision the host password once with `bace-cli host-init --state-directory
state/host`, passing the password through piped stdin from a hidden prompt. Never
put a real password in command arguments, shell history or a committed file.
Provisioning never overwrites an existing operator. Operator credentials are
Argon2id PHC records outside the game database; game accounts have no host rights.

Run `bace-server` and open `http://127.0.0.1:8080`. An optional
`bace-server host --config server.toml` accepts the existing server settings plus:

```toml
[host]
bind_address = "127.0.0.1:8080"
state_directory = "state/host"
log_directory = "state/logs"
session_idle_seconds = 1800
session_lifetime_seconds = 28800
max_sessions = 8
max_log_streams = 8
startup_timeout_seconds = 60
drain_timeout_seconds = 30
```

Only loopback binding is supported. The exact configured host/origin is checked;
use the printed URL rather than a different localhost alias. Remote reverse-proxy
deployment, HTTPS termination policy, player character pages and trading are not
implemented. Browser sessions are bounded, expire, and use HttpOnly/SameSite
cookies. Mutations additionally require the matching Origin and session CSRF
token. Password work is bounded and runs outside the current-thread HTTP runtime.

Unix state directories/files require private modes. Native Windows credential and
capability-file ACL provisioning is explicitly unsupported in this build; the
console refuses startup there rather than treating read-only attributes as ACLs.
The TCP control/frame/locking design is portable, but that is not a claim that
the complete Windows host-console path is ready. Linux is the exercised platform;
macOS execution still requires its native CI checks.

## Diagnostics

The console shows supervisor startup, authentication, process lifecycle and drain
failures. It does **not** yet collect arbitrary child stdout/stderr; those streams
are discarded and a child exit becomes a sanitized lifecycle event. A separate
bounded child diagnostic channel remains to be connected when gameplay exists.
No complete application-console capture is claimed.

Diagnostic producers use a bounded 256-record channel. The supervisor retains up
to 2,000 records/4 MiB, exposes at most 128 records per SSE batch, and rotates four
files of at most 8 MiB each. Browser display is capped at 500 lines. Queue overflow
is counted; slow/failed disk writing never waits on the simulation owner. Disk
logs survive supervisor restarts; only the in-memory recent buffer is shown by the
current web interface. Diagnostics are best effort and are not a durable audit or
save journal. Callers MUST supply sanitized text; no general-purpose secret
redactor makes arbitrary packet/SQL/error dumps safe to log.

## Control and durability boundary

The child listens on an ephemeral IPv4 loopback port. A per-generation random
256-bit capability stays in a private metadata file. Both peers prove knowledge
with fresh nonce, role-separated HMAC-SHA256 challenges; raw capabilities are
never sent over the socket. Frames have a 64 KiB maximum. Standard-library file
locks prevent duplicate supervisor/child owners. PID files alone grant no rights.

`HostBackend` is the integration port for the authoritative world/save owner.
Its drain future MUST retain dirty state on cancellation or failure. The control
lease-loss callback MUST close admission without destroying that state. A drain
operation survives control reconnection and rejects a different operation ID.
The child refuses exit before successful drain. Retry resumes the same operation;
there is no force-kill endpoint. A lost acknowledgment or timeout remains visibly
blocked. The foundation backend has no persistence, so its successful drain is
explicitly an empty-state drain.

No production world/save coordinator is wired to this port yet. Real gameplay
integration must demonstrate durable final snapshots, uncertain-commit recovery,
admission closure and retained world ownership before advertising safe game
restart. Compiled `.bace` dataset status will be supplied by that backend; the
host supervisor must not load or duplicate the mapped world catalog.
