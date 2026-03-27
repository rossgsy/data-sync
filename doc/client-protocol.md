# syncpond Client Protocol

This document describes the syncpond client protocol used by WebSocket clients and the command API.

## WebSocket interaction

1. WS connection:
   - Client opens WebSocket to `ws://<host>:<port>/` (or `wss://` if TLS is terminated upstream).

2. Auth message:
   - Client sends JSON object:
     ```json
     {"type":"auth","jwt":"<token>","last_seen_counter":<optional>}
     ```
   - `jwt` is required if server is configured with `jwt_key` (recommended in prod).
   - `last_seen_counter` can be used to recover missed updates.

3. Server auth response:
   - On success, server sends:
     ```json
     {"type":"auth_ok","room_counter":<n>,"state":{...}}
     ```
   - On failure:
     ```json
     {"type":"auth_failed","message":"..."}
     ```

4. Update stream:
   - Server sends change events with payloads including room/container/key/value deltas.
   - Client applies these to local UI/app state.

5. Heartbeats/keepalive:
   - Client should send periodic pings at moderate rate.
   - Server may close connection for idle clients.

## Versioning and compatibility

- The API is intended to remain backward-compatible for client protocol extensions.
- New update event fields should be optional; clients should ignore unknown fields.

## Note
- TLS termination and authentication are expected to be enforced by reverse proxy in production.
- `command_api_key` is mandatory and privileged. Keep this secret.
- The API is intended to remain backward-compatible for client protocol extensions.
- New update event fields should be optional; clients should ignore unknown fields.

## Note
- TLS termination and authentication are expected to be enforced by reverse proxy in production.
- `command_api_key` is mandatory and privileged. Keep this secret.
