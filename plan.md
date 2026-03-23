# Project Plan: Rust Server + TypeScript Client Real-time JSON Sync

## 1. Goal
Build a real-time synchronization platform where server applications can set key/value pairs in named rooms and have that state immediately broadcast to all connected clients in the same room. Clients are read-only replicas (downstream only) and do not originate updates; they receive room state and deltas from the server. The server treats write payloads as opaque values and does not require application-level structure semantics. This includes:
- Room-based state partitions (multiple rooms)
- Join-in-progress semantics (new clients receive current room key/value state)
- Downstream-only update propagation (server -> clients)
- Basic reliability and data consistency for key/value objects

## 2. Architecture Overview

### 2.1 Server (Rust)
- Two interfaces:
  - Public real-time client socket (WebSocket) for browser/mobile clients
  - Server-app control socket (TCP/Unix socket, Redis-like command protocol) for server-side applications
- Room manager: `HashMap<u64, RoomState>` (roomId is `u64`)
- RoomState includes current map of fragments (key + opaque value), connected client sessions, per-fragment byte size, and total room byte size
- Client session: unique connection ID, room_id, send channel
- Public message routing: inbound client message parsed as JSON action (e.g., `join`, `sync-request`), but browser/mobile clients are read-only and do not send write commands; writes are accepted only from server-app control socket
- Data model uses `serde_json::Value` for payload values; server itself does not interpret or modify value contents beyond storing and broadcasting
- Concurrency inside a room is simplified: exactly one server-app writer instance per room is assumed, so room internal concurrent write conflict handling is not required; only clients are concurrent readers

### 2.2 Client (TypeScript)
- Web/client app using WebSocket native or `socket.io-client` with minimal wrapper
- Connect to server and send `join` command with room id
- On join ack, receive initial room snapshot JSON
- On server push updates, apply to local state and render

## 3. Protocol and Message Design

### 3.1 Server-app Protocol (TCP/Unix command socket)
- Commands from server-side apps:
  - `ROOM.CREATE`: create room and return auto-incremented `roomId` (u64)
  - `ROOM.DELETE <roomId>`: delete room and disconnect clients
  - `SET <roomId> <container> <key> <value-json>`: set/replace a fragment in a container (use `public` for public container)
  - `DEL <roomId> <container> <key>`: remove fragment from container state
  - `GET <roomId> <container> <key>`: return fragment value and version (if exists and authorized)
  - `VERSION <roomId>`: return counter
  - `SET.JWTKEY <base64-key>`: set shared HMAC secret for JWT validation
  - `TX.BEGIN <roomId>`: begin transaction 
  - `TX.END <roomId>`: commit buffered writes and broadcast
  - `TX.ABORT <roomId>`: discard buffered writes
  - `SET.JWTKEY <base64-key>`: set shared HMAC secret for JWT validation
  - `TX.BEGIN <roomId>`: begin transaction 
  - `TX.END <roomId>`: commit buffered writes and broadcast
  - `TX.ABORT <roomId>`: discard buffered writes
- Responses:
  - `OK`, `ERROR <msg>`
  - On queries, return JSON payload for `GET`/`STATE`
- Room state/concurrency operations follow the same model as in 2.1.

### 3.2 Client sync protocol (WebSocket)
- Client must send first message: `auth` JSON with signed JWT:
  - `type`: "auth"
  - `jwt`: token
  - JWT claims: `sub`, `room`, `containers`, `exp`
  - `room` claim is tied to server's room ID namespace; `room` must be a stable identifier (e.g., numeric u64 string, or mapped by server lookup)
  - `containers` is a list of additional container names the client is allowed to receive (server always includes `public`)
- Server response:
  - `auth_ok` with current room counter and filtered state snapshot (public + allowed containers)
  - `auth_error` and close if invalid or room missing or unauthorized containers requested
- No client write commands accepted after auth; clients may only send auth + optional heartbeat/ping/last-seen counter for reconnection. Any unrecognized or write-intent payload is protocol violation and connection close.
- Server broadcasts updates to clients in room:
  - `update`: contains changed fragments and room counter; each update includes container+key info and is only forwarded to clients authorized for that container (plus `public`)
  - `state`: optional filtered full snapshot on initial join
- Clients can reconnect with `auth` and include last-seen counter to receive delta updates (as filtered by their container authorization)

### 3.3 Room management
- Rooms are created/destroyed only through server-app control socket (`ROOM.CREATE`, `ROOM.DELETE`)
- `ROOM.DELETE`: remove room, disconnect all clients in that room, clear state
- Client join requires room to already exist; otherwise `auth_error` is returned
- On join, delivers current room state snapshot
- On write, merges into room state and broadcasts (unless in transaction mode)
- On disconnect, remove client from room
- If room becomes empty, optional time-based evacuation (or keep-if-configured)

### 3.3 Room-level transactions
- `TX.BEGIN roomId` starts transactional mode for a room
  - Writes for that room are buffered (in memory) and not broadcast
- `TX.END roomId` commits buffered changes:
  - apply updates to room state
  - increment room counter for each change
  - calculate delta for keys changed
  - broadcast `update` (or `diff`) to connected clients
- `TX.ABORT roomId` discards buffered writes
- no strict ordering guarantee required across transactional commits

## 4. Sync behavior
- Writes are authoritative on the server; clients are read replicas
- Room versioning with Lamport-style clock (u64):
  - Each room maintains `room_counter: u64` (increment for every SET/DEL)
  - Each key also tracks `key_version: u64` equal to `room_counter` when it was last changed
  - Room state: map<key, (value, key_version)> and total byte size
- Identity fragments concept:
  - Identity fragments are room entries scoped to specific client identities (JWT `sub` or identity token), e.g., `player-details` with per-user values
  - Identity fragments use a shared base key namespace and may have duplicate key names across identities; the server resolves by `identity+key`
  - Room-level state snapshot for a client includes
    - public fragments (visible to all clients)
    - identity fragments intended for that client's identity only
  - Broadcast updates include both public fragments and any identity fragments for authenticated recipients only (or separate per-client routed feed)
  - Server `GET` for identity fragment requires identity context and should only return allowed identity fragments
- Join-in-progress:
  - Full hydrate: send complete room state + current room_counter (public + authorized containers only)
  - Partial reconnect: client sends last-seen counter `client_counter`
  - Server responds with `diff` containing only fragments (container/key) where `key_version > client_counter`, plus updated room_counter
- Optional: per-object IDs and LWW for eventual consistency

## 5. Persistence (out of scope)
- In-memory only; no disk snapshot/restore or durability guarantees in this MVP

## 6. Security
- Validate incoming JSON shape
- Room authorization placeholder (future feature)
- Rate limiting incoming actions

## 7. Milestones
1. Setup Rust project + basic WS server and client echo example
2. Implement room create/join with server-side state
3. Implement write + broadcast semantics
4. Add join-in-progress initial snapshot to clients
5. Implement client data flow and UI hydration in TypeScript
6. Add tests: unit (room logic), integration (server + client emulation)
7. Add error handling and reconnection
8. Polish docs and README

## 8. Tech stack and dependencies
- Rust: tokio, warp or axum, serde, serde_json, futures, tracing
- TypeScript: WebSocket API (or socket.io), React/Vue/Svelte (optional), vite, esbuild
- Testing: Rust tokio-test, wasm-based client test if needed

## 9. API docs and data contracts
- `plan.md` + `README.md` to document message schema
- Provide TypeScript types matching server message structures

## 10. Deployment and run
- `cargo run` for server
- `npm run dev` for client
- Production: build binary + static host/spa behind nginx

## 11. Further enhancements
- Delta patch + CRDT support
- ACLs and auth (JWT)
- Metrics (latency, room size)
- Horizontal scaling (Redis pub/sub)
