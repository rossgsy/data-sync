# syncpond TypeScript Client Usage Examples

This document provides reference examples for using the `syncpond-client` library in the browser and Node.js.

## Browser Example

```ts
import { SyncpondClient, extractRoomSnapshot } from "syncpond-client";

const client = new SyncpondClient({
  url: "ws://localhost:8000",
  jwt: "eyJhb...",
  autoReconnect: true,
  reconnectIntervalMs: 3000,
  maxReconnectAttempts: 5,
});

client.on("open", () => {
  console.log("connected");
});

client.on("auth_ok", (payload) => {
  console.log("initial state", extractRoomSnapshot(payload));
});

client.on("update", (update) => {
  console.log("data changed", update);
});

client.on("error", (err) => {
  console.error("ws error", err);
});

await client.connect();
```

## Node.js Example

```ts
import WebSocket from "ws";
import { SyncpondClient } from "syncpond-client";

const client = new SyncpondClient({
  url: "ws://localhost:8000",
  jwt: process.env.SYNCPOND_JWT ?? "",
  wsConstructor: WebSocket,
});

client.on("close", (event) => {
  console.log("disconnected", event.code, event.reason);
});

client.on("room_update", (payload) => {
  console.log("room counter", payload.room_counter);
});

await client.connect();

// later
client.disconnect();
```
