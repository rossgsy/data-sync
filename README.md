# syncpond

A real-time room-based containerized key/value sync platform (Rust server + TypeScript client).

## Scope

- TLS is out of scope for this system. TLS termination is handled by reverse proxies.
- Persistence is not part of the design and is out of scope.
- Command API is intended for trusted clients only and requires a configured API key (see `command_api_key`).