# nimesvc Overview

`nimesvc` is a compact DSL and code generator for API contracts and runtime wiring.

You describe a service once and generate:
- OpenAPI 3.0.3 documents
- HTTP servers in Rust, Go, and TypeScript
- gRPC server output
- WebSocket and event runtime helpers

## What the DSL covers

The DSL lets you describe:
- routes and response contracts
- typed inputs (`path`, `query`, `body`, `headers`)
- shared types and enums
- auth, middleware, rate limits, and env requirements
- RPC methods, WebSocket services, and events
- multi-service projects in one file

## What the generator does

From the same source file, `nimesvc` can:
- generate server code
- generate OpenAPI output
- wire modules into handlers
- prepare events, cross-service calls, and protocol-specific runtime code

## Minimal example

```ns
service API:
    GET "/health":
        response 200
        healthcheck
```

Generate and run:

```bash
nimesvc generate api.ns rust
nimesvc run api.ns rust
```

## Recommended reading order

- [Quick Start](quickstart.md)
- [Features](features.md)
- [DSL](dsl.md)
- [Modules and Calls](modules.md)
- [Types and Validation](types.md)
- [RPC](rpc.md)
- [Events](events.md)
- [WebSocket](websocket.md)
- [CLI](cli.md)
