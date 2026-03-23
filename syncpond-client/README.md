# @syncpond/client

A TypeScript shared client library for Syncpond server realtime updates.

## Install

```bash
npm install @syncpond/client
```

## Usage

```ts
import { SyncpondClient } from "@syncpond/client";

const client = new SyncpondClient({
  url: "ws://localhost:3000/ws",
  jwt: "<your-jwt>",
  autoReconnect: true,
});

client.on("open", () => {
  console.log("WS connected");
});

client.on("auth_ok", (payload) => {
  console.log("Authed", payload);
});

client.on("update", (update) => {
  console.log("Update", update);
});

client.on("auth_error", (error) => {
  console.error("Auth failed", error);
});

client.connect().catch(console.error);
```

## Node.js usage

In Node, provide a WebSocket constructor from the `ws` package:

```ts
import WebSocket from "ws";
import { SyncpondClient } from "@syncpond/client";

const client = new SyncpondClient({
  url: "ws://localhost:3000/ws",
  jwt: "<your-jwt>",
  wsConstructor: WebSocket,
});
```
