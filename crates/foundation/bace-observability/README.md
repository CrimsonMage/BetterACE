# bace-observability

Logging, metrics and diagnostic interfaces.

Status: foundation. Bounded adapter diagnostic queue, recent ring and rotating
disk files. Callers supply sanitized records. Child stdout/stderr capture and a
complete gameplay logging integration remain unsupported.

Implementation belongs in named modules. Crate roots remain declaration-only.

`try_record_exact` returns the original record with a typed `Full`, `Closed`, or
`Oversized` reason. Its encoded 32 KiB record limit accommodates escaped accepted
game output; the 256-entry worker queue has an 8 MiB worst-case payload bound.
The existing ordinary diagnostic API keeps its smaller truncation policy. Neither
API equates queue admission with a durable disk acknowledgement.
