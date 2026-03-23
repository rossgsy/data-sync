# syncpond-server TODO

## 🛡️ Security and robustness
- Implement rate limiting on command API and WS join/auth. ✅ implemented (in-memory per-IP limits)
- Add JWT verification audience/issuer checks in `ws.rs` and `state.rs`. ✅ implemented

## 🧹 Consistency and correctness edge cases
- `room_delta` currently sends only non-empty container keys, does not emit tombstone markers for deleted keys; clients with stale state might not be fully consistent.
- Transaction commit paths do not broadcast each intermediate update; only broadcast after `TX.END` as generic `room_update` event. Consider per-fragment events or true atomic delta semantics.
- `room_snapshot` includes all container keys at snapshot time; no versioning for deleted keys.
- `tx` sequence for concurrent room writes may cause lost updates if not conflict-detected.

## 🧪 Testing and tooling
- Add integration tests for full WS auth/subscribe/update workflow across multiple clients.
- Add fuzz tests for malformed commands and invalid JWT payloads.
- Add load tests for command and WebSocket concurrency.

## 🗂️ Future APIs / protocol extensions
- Add `GET.DELTA room_id since` API to fetch incremental changes server-side.
- Add `ROOM.TRANSFER` or access delegation endpoints (for sharing rooms between users).
- Add optional `container metadata` (ACL, tombstone w/ deletion version).
- Add optional user identity in JWT (`sub`) and implement RBAC.
