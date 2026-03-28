# syncpond TypeScript Client Library Spec

This spec describes the `syncpond-client` TypeScript library behavior, API surface, and expected runtime semantics.

## 1. Purpose

`syncpond-client` is a lightweight WebSocket client wrapper for the syncpond server protocol. It provides:

- connection lifecycle management (`connect`, `disconnect`, optional auto-reconnect),
- auth handshake using JWT and optional resumption with `last_seen_counter`,
- parsed server event delivery via typed event listeners,
- typed runtime data structures aligned with `syncpond` socket protocol.

## 2. Package entrypoints

- `SyncpondClient` class (default public API)
- `extractRoomSnapshot` helper function
- Type definitions: `SyncpondClientOptions`, `SyncpondClientEvent`, `SyncpondServerMessage`, `SyncpondAuthOk`, etc.

## 3. Public API

### 3.1. constructor(options: SyncpondClientOptions)

`SyncpondClientOptions` fields:

- `url: string` (required): WebSocket URL to connect, e.g. `ws://localhost:8000`.
- `jwt: string` (required): token used in auth message.
- `lastSeenCounter?: number` (optional): room counter from last received state snapshot, sent in auth for recovery.
- `autoReconnect?: boolean` (optional; default `true`): reconnection enabled when disconnected unexpectedly.
- `reconnectIntervalMs?: number` (optional; default `2000`): time between reconnect attempts.
- `maxReconnectAttempts?: number` (optional; default `10`): max retries before giving up.
- `wsConstructor?: new (url:string) => WebSocket` (optional): custom constructor for environments where global `WebSocket` is unavailable (Node.js with `ws`).

### 3.2. connect(): Promise<void>

- resolves after WebSocket `open` + auth request sent.
- rejects on immediate connection failure or if server socket error occurs before open.
- if already connecting or connected, immediately resolves.

### 3.3. disconnect(): void

- closes underlying WebSocket and disables auto-reconnect by marking `closedByUser`.

### 3.5. isConnected (getter)

- returns `true` if WebSocket is in `WebSocket.OPEN` state.

### 3.6. on(event, listener) / off(event, listener)

- subscribe and unsubscribe typed listener callbacks.
- supports events:
  - `open`: `Event`
  - `close`: `CloseEvent`
  - `error`: `Event`
  - `auth_ok`: `SyncpondAuthOk`
  - `auth_error`: `SyncpondAuthError`
  - `room_update`: `SyncpondRoomUpdate`
  - `update`: `SyncpondUpdate`
  - `message`: `SyncpondServerMessage`

## 4. Event flow

1. `connect()` opens socket
2. `open` event triggers internal `sendAuth()` with:
   - `{ type: "auth", jwt, last_seen_counter? }`
3. Incoming messages are parsed as JSON into `SyncpondServerMessage`.
4. `message` event emitted for all parsed messages.
5. Type-specific events:
   - `auth_ok`, `auth_error`, `room_update`, `update`
6. Unknown types are ignored at typed event level (still delivered as `message`).

## 5. Reconnect behavior

- On `close` without `disconnect()` call (`closedByUser=false`) and if `autoReconnect`:
  - schedule reconnect after `reconnectIntervalMs`
  - `reconnectAttempts` increments until `maxReconnectAttempts`
  - once at/max, no further reconnects (stops silently)
- each `connect` success resets `reconnectAttempts` to `0`.

## 6. Error handling

- parse errors during JSON parse emit `error` event and ignore message.
- event listener exceptions are swallowed to avoid breaking client state.
- `ws not available` throws from `connect` if no `WebSocket` global and no `wsConstructor`.

## 7. Types

### 7.1. `SyncpondRoomSnapshot`

```
{ [containerName: string]: Record<string, unknown> }
```

### 7.2. `SyncpondAuthMessage`

```
{ type: "auth", jwt: string, last_seen_counter?: number }
```

### 7.3. `SyncpondAuthOk`

```
{ type: "auth_ok", room_counter: number, state: SyncpondRoomSnapshot }
```

### 7.4. `SyncpondRoomUpdate`

```
{ type: "room_update", room_id: number, room_counter: number }
```

### 7.5. `SyncpondUpdate`

```
{ type: "update", room_id: number, room_counter: number, container: string, key: string, value?: unknown, deleted?: boolean }
```

### 7.6. `SyncpondAuthError`

```
{ type: "auth_error", reason: string }
```

### 7.7. `SyncpondServerMessage`

Union of `SyncpondAuthOk | SyncpondRoomUpdate | SyncpondUpdate | SyncpondAuthError | { type: string; [key: string]: unknown }`

## 8. Helper API

### 8.1. `extractRoomSnapshot(event: SyncpondAuthOk): SyncpondRoomSnapshot`

- returns `event.state` and is a convenience helper for type-safe access.

## 9. Browser vs Node usage

- In browser, default global `WebSocket` is used.
- In Node, pass `wsConstructor` using `ws` package constructor
  - `new SyncpondClient({ url, jwt, wsConstructor: WebSocket })`

## 10. Spec cross-reference

- Aligns with protocol in `spec/shared/client-protocol.md` message types.
- `SyncpondClient` is the client-side protocol companion to server-side WebSocket transport.
