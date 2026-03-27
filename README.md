# syncpond

A real-time room-based containerized key/value sync platform (Rust server + TypeScript client).

## Scope

- TLS is out of scope for this system. TLS termination is handled by reverse proxies.
- Persistence is not part of the design and is out of scope.
- Command API is intended for trusted clients only and requires a configured API key (see `command_api_key`).

## Command wire protocol

The command API is a line-oriented text protocol over TCP. The first line of each connection must be the API key verbatim followed by `\n`. After auth, commands are newline-delimited; arguments may be quoted with `"..."` to preserve spaces and special characters. JSON values are passed as remaining payload after initial command tokens.

Supported commands:

- `ROOM.CREATE` -> `OK <room_id>`
- `ROOM.DELETE <room_id>` -> `OK` / `ERROR room_not_found`
- `ROOM.LIST` -> `OK [<id>,...]`
- `ROOM.INFO <room_id>` -> `OK { ... }`
- `SET <room_id> <container> <key> <json>` -> `OK` / `ERROR ...`
- `DEL <room_id> <container> <key>` -> `OK` / `ERROR ...`
- `GET <room_id> <container> <key>` -> `OK <json> <version>` / `ERROR tombstone` / `ERROR not_found`
- `VERSION <room_id>` -> `OK <version>`
- `SET.JWTKEY <jwt_secret>`
- `TX.BEGIN <room_id>`
- `TX.END <room_id>`
- `TX.ABORT <room_id>`
- `TOKEN.GEN <room_id> [containers...]`

Malformed commands return `ERROR <message>`.

## WebSocket auth and update flow

`ws` connects over TCP and completes handshake with a single JSON auth message:

```json
{"type":"auth","jwt":"<token>","last_seen_counter":<opt>}
```

Server responds with:

```json
{"type":"auth_ok","room_counter":<n>,"state":{...}}
```

Updates are broadcast as events in JSON, including container/key/value semantics.

## Configuration defaults

| Key | Default | Description |
| --- | ------- | ----------- |
| `ws_addr` | `127.0.0.1:8080` | WebSocket listen address |
| `command_addr` | `127.0.0.1:9090` | Command TCP listen address |
| `health_addr` | `127.0.0.1:7070` | Health check listen address |
| `command_api_key` | (required) | Command API pre-shared secret |
| `jwt_key` | (optional) | HMAC key for JWT tokens |
| `jwt_issuer` | (optional) | JWT `iss` required value |
| `jwt_audience` | (optional) | JWT `aud` required value |
| `jwt_ttl_seconds` | `3600` | JWT time to live seconds |
| `require_tls` | `false` | Require external TLS termination path |
| `health_bind_loopback_only` | `true` | Bind health to loopback only |
| `command_rate_limit` | `120` | command requests per window |
| `command_rate_window_secs` | `60` | seconds window |
| `ws_auth_rate_limit` | `10` | ws auth attempts per window |
| `ws_update_rate_limit` | `240` | ws updates per client per window |
| `ws_room_rate_limit` | `1000` | ws updates per room per window |

## Security posture

- enforce API key on command socket
- reject untrusted JWT and expired claims
- health endpoint loopback by default
- unbounded WS updates protected by per-client and per-room rate limiting
- command parser checks max command line length to prevent large payload DoS

## Docker image build and deploy

A convenience script is available to build and deploy the server image to Docker Hub under `paleglyph/syncpond`.

```bash
# default: image name paleglyph/syncpond and tag from git describe (or latest)
./scripts/build-and-push-syncpond-server.sh

# explicit image name + tag
./scripts/build-and-push-syncpond-server.sh --image-name paleglyph/syncpond --tag v1.0.0

# build only without pushing
./scripts/build-and-push-syncpond-server.sh --no-push
```

## Docker Compose example (with secrets and env)

Create a `.env` file at repo root:

```env
SYNCPOND_CONFIG=/etc/syncpond/config.yaml
# ensure command api key is protected and not in version control
COMMAND_API_KEY=supersecretcommandkey
JWT_KEY=supersecretjwtkey
```

Create a `config.yaml` on disk for the container (or use Docker secrets with target path `/etc/syncpond/config.yaml`):

```yaml
command_api_key: "${COMMAND_API_KEY}"
ws_addr: "0.0.0.0:8080"
command_addr: "0.0.0.0:9090"
health_addr: "0.0.0.0:7070"
jwt_key: "${JWT_KEY}"
jwt_ttl_seconds: 3600
require_tls: false
health_bind_loopback_only: false
```

Add:

```yaml
version: '3.8'
services:
  syncpond-server:
    image: paleglyph/syncpond:latest
    build:
      context: ./syncpond-server
      dockerfile: Dockerfile
    ports:
      - "8080:8080"   # websocket api
      - "9090:9090"   # command api
      - "7070:7070"   # health
    env_file: .env
    environment:
      SYNCPOND_CONFIG: /etc/syncpond/config.yaml
    volumes:
      - ./config.yaml:/etc/syncpond/config.yaml:ro
    restart: unless-stopped

  # optional reverse proxy (example nginx): provide TLS and public domain routing
  # nginx:
  #   image: nginx:stable
  #   ports:
  #     - "80:80"
  #     - "443:443"
  #   volumes:
  #     - ./nginx.conf:/etc/nginx/nginx.conf:ro

# Use Docker secrets in prod via secrets: with `command_api_key` and `jwt_key` as secure files.
```

Notes:

- `command_api_key` is sensitive: keep it out of Git and use Docker secrets for production.
- `ws_addr`, `command_addr`, `health_addr` are exposed on container; in prod, use proxying and firewall rules.
- `require_tls` remains false here since TLS should be handled by external proxy.

