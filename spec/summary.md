# syncpond Project Summary

syncpond is a containerized, real-time room-based key/value synchronization platform. It includes:

- `syncpond-server` (Rust): WebSocket + command TCP interface, JWT auth, per-room state, in-memory delta sync.
- `syncpond-client` (TypeScript): WebSocket client library for room subscriptions and state updates.
- `demo` apps: sample CLI client/server for local testing.

Key capabilities:

- Room lifecycle management (create/list/delete/info)
- Per-room key/value data storage with versioning and tombstones
- Transaction commands (TX.BEGIN/END/ABORT)
- JWT-based authentication with optional issuer/audience checks
- WebSocket auth/updating with rate limiting
- health check endpoint, command API, and WebSocket protocol

Deployment:

- Built and packed Docker image at `syncpond-server/Dockerfile`
- `docker-compose.yml` sample environment
- Build script: `scripts/build-and-push-syncpond-server.sh` (pushes to `paleglyph/syncpond`)

This repository is designed for local development, experimentations, and as a foundation for production-grade sync services with external persistence and TLS fronting.
