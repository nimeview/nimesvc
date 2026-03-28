# DSL

This chapter describes the DSL structure and how the compiler interprets it.

## File Structure

A file is parsed from top to bottom. The recommended order is:
- Top‑level directives (`output`, `use`, `auth`, `middleware`, `version`).
- Type/enum/event/rpc declarations (must appear before the first `service`).
- One or more `service` blocks.

Example:
```ns
output "./server"
use db "./modules/db.rs"
use runtime redis "0.23"
use compile sqlx
version 1

event UserCreated:
    payload User

rpc Auth.Login:
    input:
        username: string
        password: string
    output Token
    call auth.login(input.username, input.password)

service API:
    GET "/health":
        auth: none
        responses:
            200 string
        call core.health
```

### Module Example (HTTP + Logic)
```ns
use users "./modules/users.rs"

service API:
    GET "/users/{id}":
        responses:
            200 User
        call users.get_user(path.id)
```

Rust module:
```rust
use crate::types::{User, ErrorResponse};

pub async fn get_user(id: String) -> Result<User, ErrorResponse> {
    Ok(User { id, name: "demo".into() })
}
```

## Top‑Level Directives
```ns
output "./server"
use db "./modules/db.rs"
use serde_json "1.0"
auth bearer
middleware logging
version 1
```

- `output` — base generation directory.
- `use` — module path or dependency available to services.
- `auth`, `middleware` — defaults for all services (can be overridden per route or socket; use `auth: none` to disable).
- `version` — default version for `type/enum/event/rpc/socket` names and references.

## Service
```ns
service API:
    GET "/users":
        responses:
            200 User
        call db.list
```

A file can contain multiple services. Each service is generated into its own folder.

### Output Layout (Multiple Services)
When the file contains multiple services, each service gets a separate output directory:

- If `output` is not set, output goes into `.nimesvc/<ServiceName>`.
- If `output "./server"` is set, output goes into `./server/<ServiceName>`.

Example:
```ns
output "./server"

service Auth:
    GET "/health":
        responses:
            200 string
        call core.health

service Data:
    GET "/records":
        responses:
            200 Record
        call data.list
```

Generated layout:
```
./server/Auth/
./server/Data/
```

### Service Language
```ns
service Auth go:
    GET "/health":
        responses:
            200 string
        call core.health
```
Supported: `rs/rust`, `ts/typescript`, `go/golang`.

### Service Config
```ns
service API:
    config:
        address: "0.0.0.0"
        port: 8080
        base_url: "http://users:8081"
```

Short form:
```ns
service API:
    address "0.0.0.0"
    port 8080
```

- `address` / `port` — bind address for the HTTP server.
- `base_url` — optional override for cross‑service `call ServiceName.module.func`.

### env
```ns
service API:
    env DATABASE_URL
    env REDIS_URL
    env APP_MODE="dev"
```
The generator adds runtime checks for required env vars. If a default is provided (`env NAME="value"`),
the server will set the env var on startup if it's missing.

### headers (service‑level)
```ns
service API:
    headers:
        X_Request_Id: string
```
Service headers are inherited by all routes in the service.

Header naming rules:
- Header names must be identifiers (letters, numbers, `_`).
- Use `_` instead of `-` in the DSL.
- At runtime, underscores are converted to hyphens and names are lowercased.

Example:
```ns
headers:
    X_Request_Id: string
```
At runtime this maps to `x-request-id`.

### rate_limit (service‑level)
```ns
service API:
    rate_limit 100/min
```
Per‑route limits override service‑level limits.

## Routes (HTTP)
```ns
GET "/users/{id}":
    input:
        path:
            id: int
        query:
            limit?: int
    responses:
        200 User
        404 Error
    call db.get_user(path.id, query.limit)
```

### input
```ns
input:
    path:
        id: int
    query:
        q?: string
    body:
        user: User
```
`path`, `query`, `body`, and `headers` are parsed and validated into typed structs.

### headers (route‑level)
```ns
GET "/users":
    headers:
        X_Custom: string
    responses:
        200 User
    call db.list
```
Header names must be identifiers (use `_` instead of `-`).

### responses
```ns
responses:
    200 User
    404 Error
    401 AuthError
```
Rules:
- Any `2xx` is considered a success.
- Non‑`2xx` are treated as typed errors by generated server helpers.
- Use `204` for empty responses.

### healthcheck
```ns
GET "/health":
    healthcheck
```
Marks a route as a health check. Generators may apply special behavior for these endpoints.

## call
```ns
call module.func
call module.func(body)
call module.func(path.id, query.q, body.user)
call module.func(user=body.user, id=path.id)
call ServiceName.module.func
async module.func(path.id)
```
`call` is synchronous by default. Use `async` to await an async module function.

Notes:
- Arguments come from `path`, `query`, `body`, and `headers`.
- Cross‑service calls use the target service `base_url` if set, otherwise they default to the service
  `address` + `port` (or `127.0.0.1:3000` if neither is set).
- RPC calls use `input.*` and `headers.*` (see RPC chapter).

## Types, Enums, Versions
```ns
type User:
    id: int
    name?: string
    status: Status
    meta: { age: int, tags?: array<string> }

enum Status:
    Active
    Disabled

// Versioned names
// User@2, Status@2, event UserCreated@2, rpc Auth.Login@2
```

Versioned references use `Name@<number>` and can be mixed with default `version`.

## RPC, Events, WebSocket
These are covered in dedicated chapters:
- [RPC (gRPC)](rpc.md)
- [Events](events.md)
- [WebSocket](websocket.md)
