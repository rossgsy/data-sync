# Todo for syncpond project

## 1. Critical security/hardening

- [x] Handle encrypted transport expectations (TLS) for both WebSocket and command/health listeners.
- [x] Add explicit bounds on incoming line lengths for TCP command and health endpoints to prevent OOM/DOS via huge payloads.
- [x] Make health endpoint configurable/auth protected or bind to loopback only (current behavior has no auth).
- [x] Enforce explicit `JWT` validation for possibly missing `exp` claim and reject expired tokens (jsonwebtoken default behavior may be permissive if not set; confirm/explicitly test).
- [x] Ensure command API key is not logged and is treated as a secret (avoid accidental structured logging of commands with secrets).
- [x] Add time-source fuzzing / monotonic-safe checks around `SystemTime::now` for JWT exp claims.

## 2. Command protocol and parsing robustness

- [ ] Improve `SET`/`DEL`/other text command token parsing to safely support JSON values with spaces, Unicode, and special chars. Current `splitn` + simple quote heuristics may break.
- [ ] Reject malformed commands early with clearer `ERROR` messages.
- [ ] Return distinct room-specific error codes for `GET` on tombstone vs missing key if intended.

## 3. Transaction semantics consistency

- [ ] Clarify `TX.END` behavior: currently `DEL` only tombstones if key existed at commit time; check intended semantics and align with spec.
- [ ] Add tests for transaction conflict/ordering edge cases (interleaving clients, concurrent writes in same room).

## 4. WebSocket flow and room state

- [ ] Fix `total_ws_connections` incrementing on successful `handle_ws_connection` auth (currently only decremented on close).
- [ ] Add explicit cleanup of stale clients on room deletion (hub may keep entries for removed rooms until disconnect).
- [ ] Add protection for large number of subscriptions or high-frequency broadcast; currently sends to all clients via unbounded channel, risking unbounded memory usage.

## 5. Rate limiter and DoS defenses

- [ ] Improve rate limiter to periodically evict stale keys (currently grows forever with new IPs).
- [ ] Add per-client and per-room rate limit settings for WS updates and command actions.

## 6. Reliability and observable improvements

- [ ] Add structured metrics, error counters, and request latency tracking (especially for long-lived WS operations).
- [ ] Add more unit tests for edge cases: `ROOM.INFO` with invalid IDs, invalid JSON in `SET`, race with deleted room while WS clients connected.
- [ ] Add integration tests exercising command server + ws client together.

## 7. Code quality and lint cleanup

- [ ] Remove unused `mut` bindings from `commands.rs` as warned by compiler.
- [ ] Consider converting `StateError` to `thiserror` style for `Display` less verbose.
- [ ] Add `#[deny(missing_docs)]` and docs for public API/commands.

## 8. Documentation

- [ ] Document protocol commands and channels in README (command wire format, JWT claim requirements, grant scope semantics).
- [ ] Document configuration defaults (`ws_addr`, `command_addr`, `health_addr`, `command_api_key`, etc.) and recommended security posture.

