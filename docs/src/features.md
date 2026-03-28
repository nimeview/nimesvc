# Features

`nimesvc` focuses on describing APIs with minimal syntax while generating fully typed servers and runtime output.

## Current Status
- HTTP DSL, OpenAPI generation, Rust/TypeScript/Go servers, events, and CLI workflows are implemented.
- gRPC and WebSocket generation/runtime are implemented and covered by smoke and end-to-end tests.
- Ongoing work is focused on release hardening, CI expansion, and packaging consistency.

## Core Protocols
- HTTP routes with typed inputs (`path`, `query`, `body`, `headers`).
- RPC (gRPC) service methods with typed input/output.
- WebSocket services with inbound/outbound message contracts.
- Events with broker support (Redis Streams) or in‑memory fallback.

## Types and Validation
- Primitive, array, map, object, union, oneof, nullable.
- Enums (string or numeric).
- Validation constraints (min/max/regex/len/etc).

## Generators
- Servers: Rust, TypeScript, Go.
- OpenAPI 3.0.3 output (YAML/JSON).

## Runtime Behavior
- Automatic parsing and validation.
- Typed error handling and runtime error variants.
- Auth and middleware hooks with baseline generated behavior.
- Event consumers/producers for memory and Redis-backed modes.
- WebSocket rooms and triggers.

## Current limits
- HTTP is the most mature runtime path.
- gRPC is supported, but has less runtime coverage than HTTP.
- WebSocket support is available, but still has less production-style validation than HTTP.

## Tooling
- `init`, `build`, `generate`, `run`, `stop`, `lint`, `fmt`, `env`, `doctor`, `update`.

## Example Flow
```ns
type User:
    id: string
    name: string

service API:
    GET "/users/{id}":
        input:
            path:
                id: string
        responses:
            200 User
        call users.get(path.id)
```
