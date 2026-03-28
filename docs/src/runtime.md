# Runtime Behavior

This chapter explains how generated servers behave at runtime and what guarantees you get.

## Request Parsing
- `path`, `query`, `body`, `headers` are parsed into typed structs.
- Optional fields are allowed to be absent.
- Parsing failures return `400` with a structured JSON error.

## Validation
- Validation rules are enforced before handlers run.
- Errors include the field and constraint that failed.
- For nested objects, error paths use dot notation (e.g. `user.email`).

## Rate Limiting
- `rate_limit` can be applied at service, route, socket, or RPC method level.
- HTTP: when the limit is exceeded, the server returns `429`.
- WebSocket: when the limit is exceeded, the server sends an `Error` frame and ignores the message.

## Auth & Middleware
- `auth` and `middleware` can be set at service, route, or socket levels.
- Generated HTTP auth middleware returns `401` when required credentials are missing.
- Generated custom middleware hooks run before handler calls and can reject the request.
- Generated socket middleware validates inbound message kinds before handler dispatch.

## Errors
- HTTP: non‑`2xx` responses are serialized to JSON and mapped into client errors.
- RPC: runtime errors map to gRPC status codes (see generated server).
- WebSocket: errors are sent as `Error` frames.

## Events
- Memory mode dispatches to in-process handlers.
- Redis mode publishes serialized payloads to streams.
- `subscribe` creates consumer loops and dispatches payloads to registered handlers.
- Redis consumer groups are created lazily on first run.

## WebSocket
- Inbound messages are parsed into `{ type, data, room? }`.
- `Join` handler is triggered on connect; `Exit` on disconnect.
- `Ping` without a handler auto‑responds with `Pong`.
- Room helpers are generated when `room`/`topic` are declared.

## RPC
- `input` and `headers` are validated like HTTP inputs.
- `call` is invoked with typed values and its result is returned to the gRPC client.

## Runtime Coverage Notes
- The HTTP runtime path currently has the strongest generator and test coverage.
- Events support both in-memory and Redis-backed flows.
- gRPC and WebSocket runtimes are implemented and covered by smoke and end-to-end tests, but still have less long-running usage coverage than HTTP.
