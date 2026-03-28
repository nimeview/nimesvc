# Generation Output Structure

This chapter explains what the generator creates on disk for each service.

## Root layout
If `output` is not set, files are generated under:
```
.nimesvc/<ServiceName>/
```

If `output "./server"` is set, files are generated under:
```
./server/<ServiceName>/
```

## Rust
Typical layout:
```
<out>/<ServiceName>/
  Cargo.toml
  rpc.proto
  src/
    main.rs
    types.rs
    middleware.rs
    events.rs
    remote_calls.rs
    modules/
      ...
```

What each file does:
- `main.rs` — server bootstrap (routes, middleware, listeners).
- `types.rs` — generated DSL types and enums.
- `middleware.rs` — generated HTTP middleware/auth helpers when needed.
- `modules/` — copied local modules used by the service.
- `events.rs` — generated event helpers when events exist.
- `remote_calls.rs` — generated inter-service HTTP helpers when needed.
- `rpc.proto` — generated when RPC methods exist.

## TypeScript
Typical layout:
```
<out>/<ServiceName>/
  package.json
  tsconfig.json
  install.sh
  rpc.proto
  src/
    index.ts
    types.ts
    middleware.ts
    events.ts
    remote_calls.ts
    modules/
      ...
```

What each file does:
- `index.ts` — server bootstrap and route wiring.
- `types.ts` — generated DSL types and parsing helpers.
- `middleware.ts` — generated middleware/auth helpers when needed.
- `modules/` — copied local modules used by the service.
- `events.ts` — generated event helpers when events exist.
- `remote_calls.ts` — generated inter-service HTTP helpers when needed.
- `rpc.proto` — generated when RPC methods exist.

## Go
Typical layout:
```
<out>/<ServiceName>/
  go.mod
  main.go
  middleware.go
  events.go
  remote_calls.go
  rpc.proto
  types/
    types.go
  modules/
    <module>/
      ...
```

What each file does:
- `main.go` — server bootstrap (routes, middleware, listeners).
- `types/types.go` — generated DSL types and enums.
- `middleware.go` — generated middleware/auth helpers when needed.
- `modules/` — copied local modules used by the service.
- `events.go` — generated event helpers when events exist.
- `remote_calls.go` — generated inter-service HTTP helpers when needed.
- `rpc.proto` — generated when RPC methods exist.

## Notes
- Generated files are meant to be disposable build output.
- Keep business logic in source modules referenced by `use`.
- `types.*`, `middleware.*`, `remote_calls.*`, and event helper files are regenerated output.
