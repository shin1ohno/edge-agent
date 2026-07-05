# WeaveCore

Domain models and persistence for the weave Universal app.

- Pure-Swift `Codable` value types (`Device`, `Service`, `Route`, `Connection`).
- `WeaveStore` actor wrapping SwiftData for local persistence.
- No CloudKit, no networking — those live in `WeaveServer` and `WeaveServices`.

See `weave/project/handoff/CLAUDE_CODE_HANDOFF.md` §3 for the model contract.
