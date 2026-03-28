# nimesvc

Minimal DSL for describing HTTP APIs and producing servers in Rust, TypeScript, and Go.

## Why
- Compact DSL focused on API contracts and runtime behavior.
- Strong typing across generated services and contracts.
- Multi‑service projects in a single DSL file.
- Built‑in support for HTTP, RPC (gRPC), WebSocket, and Events.

## Features
- HTTP routes with typed inputs (`path`, `query`, `body`, `headers`).
- RPC (gRPC) methods with typed input/output.
- WebSocket services with inbound/outbound message contracts.
- Events with Redis Streams or in‑memory fallback.
- Codegen for servers (Rust/TS/Go).
- OpenAPI 3.0.3 output (YAML/JSON).
- Tooling: `init`, `build`, `generate`, `run`, `stop`, `lint`, `fmt`, `env`, `doctor`, `update`, `dev`.

## Install

### macOS / Linux
```bash
curl -fsSL https://raw.githubusercontent.com/nimeview/nimesvc/main/scripts/install.sh | bash
```

### Windows (PowerShell)
```powershell
iwr https://raw.githubusercontent.com/nimeview/nimesvc/main/scripts/install.ps1 -UseBasicParsing | iex
```

Update:
```bash
nimesvc update
```

## Quick Start

Init:
```bash
nimesvc init
```

and

Run:
```bash
nimesvc run main.ns
```

Dev loop:
```bash
nimesvc dev main.ns
```

## Example DSL
```ns
output "./server"
use core "./modules/core.rs"

auth bearer
middleware logging

service API:
    env DATABASE_URL

    GET "/hello":
        responses:
            200 string
        call core.hello

    POST "/users":
        input:
            body:
                user: User
        responses:
            201 User
        call core.create_user(body.user)
```

## Multi‑Service Output
If the file contains multiple services, each service is generated into its own folder:
```
./server/Auth/
./server/Data/
```

## Documentation
- English book: `docs/src/index.md`

Serve docs with mdBook:
```bash
mdbook serve ./docs
```

## CLI
```bash
nimesvc init
nimesvc build api.ns --output ./openapi
nimesvc generate api.ns rust --out ./.nimesvc
nimesvc generate api.ns --lang rust
nimesvc dev api.ns rust
nimesvc run api.ns
nimesvc stop api.ns
nimesvc env api.ns
nimesvc lint api.ns
nimesvc fmt api.ns
nimesvc doctor
nimesvc update
```

## Generation Output
Default server output: `.nimesvc/<ServiceName>` or `output/<ServiceName>` if `output` is set in the DSL.
See `docs/src/output_structure.md` for details.

## Known Limitations
- The HTTP pipeline has the broadest runtime and test coverage.
- gRPC and WebSocket are implemented and tested, but still have less real-world validation than HTTP.
- Some advanced runtime scenarios still need broader long-running usage coverage.
- GitHub Actions and release packaging are still being hardened.[.github](../NimeScript/.github)

## License
Apache License 2.0
