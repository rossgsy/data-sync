# syncpond-server review TODO

This todo file captures code quality, security, and robustness recommendations for `syncpond-server`.

## 1. Security hardening

1.1. Use constant-time API key comparison
- File: `src/main.rs` `handle_command_connection`
- Issue: `provided_key != expected_key` is direct equality. For protection against timing attacks on remote API key checks, compare using constant-time `subtle::ConstantTimeEq` (or equivalent).

1.2. Improve JWT claim validation
- File: `src/ws.rs` `validate_jwt_claims`
- Issue: `room` claim is parsed after decode, but `sub` is ignored. Enforce `sub` matches `room` or expected pattern, to reduce risk of overly broad tokens.
- Issue: issuer/audience are optional in config; if configured, they are validated, but do not require `sub`.

1.3. Reject expired or low-entropy JWTs proactively
- File: `src/ws.rs` and `src/state.rs` `create_room_token`
- Issue: JWT uses `HS256` with shared key; encourage fleshing out `jwt_key` policy and minimum length (e.g., at least 32 bytes) and 2FA methods for management.

1.4. Authorization/logging of command failures with rate limit
- File: `src/main.rs` `handle_command_connection`
- Issue: API key failure returns `ERROR invalid_api_key` but still in one request; a failed attempt still counts inside connection. Add per-IP key failure rate limiting to avoid forced disconnection by repeated invalid auth.

1.5. Websocket upgrade origin checks (WSS proxy support)
- File: `src/ws.rs` `handle_ws_connection`
- Issue: no Origin/CORS checks; consider verifying `Host` or origin header in scenario hosting public ws endpoints.

## 2. Robustness and resource control

2.1. Rate limiter key explosion and memory limits
- File: `src/rate_limiter.rs`
- Issue: `buckets.retain` happens each `allow`, but cleanup is only based on the current window and `now`; a high volume of distinct keys still uses memory, retains buckets if not stale. Add global cap/eviction (LRU/backoff) and explicit `max_keys` config.

2.2. Major shared-mutex contention risk
- File: `src/ws.rs` `WsHub::broadcast_update` and `src/main.rs` hub lock patterns
- Issue: broadcast lock is held across per-client `.try_send` loops. When there are many clients, this can cause delayed command processing. Use lock-free/concurrent structures or collect disconnects with scoped lock.

2.3. Add stronger command parsing protections
- File: `src/commands.rs` `take_token` and `process_command`
- Issue: token parsing allows unbounded container/key strings and unescaped JSON in `SET`; no maximum `key`/`container` lengths. Add limits (e.g., 256/1024) to prevent abusive payloads and DoS.

2.4. Validate room IDs and numeric values strictly  
- File: `src/commands.rs` and `src/state.rs`  
- Issue: `room_id` is `u64`, but `parse_room_id_from_remainder` can parse large values, then state map may use huge IDs and memory. Add explicit upper bounds (e.g., `<= 1_000_000_000`) or reject improbable values.

2.5. Health check request handling is basic
- File: `src/main.rs` `handle_health_connection`
- Issue: parses a single line with no HTTP path normalization. Accept `GET /health HTTP/1.1` and optional headers robustly; currently call may not support standard clients. Also avoid `HTTP/1.0` and long line (no line size limit at parse). Add limit and HTTP parser or use `hyper` for production.

2.6. Command TCP connections: big request payload handling
- File: `src/main.rs` `read_line_with_limit`
- Issue: limit is 8192; good but then command parser may allocate heavy for value JSON. Ensure enforcement of value size (not just line length). May still accept huge JSON with many spaces; at 8KB that is okay but verify expected.

## 3. Data consistency and concurrency correctness

3.1. Tx semantics and snapshot consistency
- File: `src/state.rs` `tx_begin`, `tx_end`, `room_snapshot`, `room_delta`
- Issue: optimistic read while transaction active: `room_snapshot` may expose stale state until TX.END. This is expected but document and potentially add `tx_read` semantics.

3.2. Room delete + cluster cross-check race
- File: `src/main.rs` inside command loop around `ROOM.DELETE`; removal from hub after command is processed.
- Issue: room events from concurrent command may race with STALE ws clients. Might attempt to use deleted room state. Consider applying room deletion under lock with a state-wide mutex or barrier and ensure blocked updates if room removed.

3.3. Persistent corrupt state protection
- No persistence; all in-memory. Add optional snapshot/append-only log (WAL) to survive restarts.

## 4. Code quality, maintenance & tests

4.1. Add more unit tests for error paths
- Validate invalid JWT `sub`/`room` combinations.
- Validate `SET` with max-length and delimiting across quotes.
- Validate `command_api_key` blank at runtime (currently checked in main only).

4.2. Introduce lints and static checks
- Add `clippy` and `fmt` to CI. There are no explicit clippy lint attributes.

4.3. Module-level docs for security assumptions
- Add text to README or code comments indicating `require_tls` requires external TLS proxy, trusted network for command socket, and no dynamic key rotation.

4.4. Improve metrics and observability
- Add additional stats: command auth failures, invalid command counts, ws disconnect reason breakdown. Potentially route to Prometheus style with labels.

4.5. Error wrapping in authentication and connection
- Use typed errors instead of `anyhow` strings to avoid generic catch-all. Avoid exposing internal error descriptions in client responses.

## 5. Operational checks

- Ensure `config.yaml` secret values are not logged on startup (currently not). Good.
- Add readiness/liveness endpoints in health path.
- Add signal handling for graceful shutdown of all clients with connection drains, not just tokio::select on ctrl-c.

---

### Immediate action items

- [ ] Implement constant-time API key check.
- [ ] Add maximum token length for `take_token` and command parameters.
- [ ] Add global rate-limit bucket memory cap.
- [ ] Switch health endpoint to a small HTTP parser or hyper.
- [ ] Add improved ws claim validation and origin controls.
- [ ] Add more tests for edge-case invalid inputs.

