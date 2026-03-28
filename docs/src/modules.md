# Modules and Calls

Modules connect the DSL to real code in Rust/Go/TypeScript. You declare modules with `use` and
call functions with `call`. This is the main bridge between the DSL and your business logic.

## `use`
```ns
use db "./modules/db.rs"
use helpers "./modules/helpers.ts" as h
use serde_json "1.0"
```

Rules:
- Path modules are copied into the generated service folder.
- Dependency modules are added to runtime dependencies.
- `as <alias>` changes the module name used in `call`.
- Paths are resolved relative to the `.ns` file.

## Dependency vs Path Modules
Path modules:
```ns
use db "./modules/db.rs"
use auth "./modules/auth/"
```
- Copies a file or directory into `generated_service/modules/`.
- Best for your own code and helper utilities.

Dependency modules:
```ns
use serde_json "1.0"
use axum "0.7"
```
- Adds a runtime dependency (Cargo/Go module/npm package).
- Best for third‑party libraries.

## Scope
```ns
use my_codegen "./codegen.rs" scope compile
use runtime "./runtime.rs" scope runtime
```
- `compile` → available only during generation.
- `runtime` → available only in the generated server.
- default is `runtime`.

## `call`
```ns
call db.create_user(body.user)
call db.get_user(path.id)
call analytics.track(event=body.event)
```

Arguments can be positional or named. Values come from:
- `path.<name>`
- `query.<name>`
- `body.<name>`
- `headers.<name>`
- `input.<name>` in RPC handlers

## HTTP Example
```ns
use db "./modules/db.rs"

service API:
    POST "/users":
        input:
            body:
                user: User
        responses:
            201 User
        call db.create_user(body.user)
```

## RPC Example
```ns
use auth "./modules/auth.rs"

rpc Auth.Login:
    input:
        username: string
        password: string
    output Token
    call auth.login(input.username, input.password)
```

## WebSocket Example
```ns
use chat "./modules/chat.rs"

service Chat:
    socket Chat "/ws":
        inbound:
            Join -> chat.on_join
            MessageIn -> chat.on_message
        outbound:
            MessageOut -> chat.send_message
```

## Events Example
```ns
use audit "./modules/audit.rs"

event UserCreated:
    payload User

service API:
    emit UserCreated
    subscribe UserCreated

    GET "/users/{id}":
        responses:
            200 User
        call audit.read_user(path.id)
```

## Module Files
Path-based modules are copied into the generated service under:
```
<service>/modules/
```

The generator expects the referenced functions to exist in those files.
If a module path is missing or a referenced function cannot be resolved, generation fails with an error.

## Writing Modules (Rust/Go/TypeScript)
Modules are just normal source files. The generator calls your functions from the
generated handlers. You can use any library you want — just add it via `use <crate> "<version>"`
for Rust, `use <module> "<version>"` for TS, or `use <module> "<version>"` for Go.

### Rust modules
File: `modules/db.rs`
```rust
use crate::types::{User, ErrorResponse};

pub async fn get_user(id: String) -> Result<User, ErrorResponse> {
    // ...
}
```
Notes:
- Use `pub` functions. Async is allowed and recommended.
- You can import generated types via `crate::types::*`.
- Read env with `std::env::var("NAME")`.

### Go modules
File: `modules/db.go`
```go
package modules

import "os"

func GetUser(id string) (User, error) {
    _ = os.Getenv("DATABASE_URL")
    // ...
    return User{}, nil
}
```
Notes:
- Use exported functions (`FuncName`).
- Generated types are available in the same package (usually `modules`).
- Read env with `os.Getenv("NAME")`.

### TypeScript modules
File: `modules/db.ts`
```ts
import type { User } from "../types";

export async function getUser(id: string): Promise<User> {
  const url = process.env.DATABASE_URL;
  // ...
  return { id, name: "demo" };
}
```
Notes:
- Use named exports.
- Types are in `types.ts`.
- Read env with `process.env.NAME`.

## Environment Variables
Declare required env vars in DSL:
```ns
service API:
    env DATABASE_URL
    env JWT_SECRET="dev_secret"
```

At runtime:
- Rust: `std::env::var("DATABASE_URL")`
- Go: `os.Getenv("DATABASE_URL")`
- TS: `process.env.DATABASE_URL`

If a default is provided, the generator sets it at startup.

## Header Mapping
Header names declared in DSL are normalized at runtime:
- DSL uses `_` instead of `-` (e.g. `X_Request_Id`).
- At runtime, headers are lowercased and `-` is used.
- Access in `call` uses the DSL name: `headers.x_request_id`.

Example:
```ns
headers:
    X_Request_Id: string
```
In `call`, use:
```ns
call audit.track(headers.x_request_id)
```

## Cross‑Service Calls
Cross‑service calls are compiled into HTTP requests using the target service `base_url` if set,
otherwise the service `address` + `port` is used (or `127.0.0.1:3000` by default).

### Syntax
```ns
call Users.db.get_user(path.id)
```

### Requirements
- The `ServiceName` in `call ServiceName.module.func` must exist in the same DSL file.
- The target service may define `base_url` to override `address` + `port`.

Example:
```ns
service Users:
    config:
        base_url: "http://users:8081"

    GET "/users/{id}":
        responses:
            200 User
        call db.get_user(path.id)

service API:
    GET "/profile/{id}":
        responses:
            200 User
        call Users.db.get_user(path.id)
```

### Behavior
- The compiler generates an HTTP client call to `base_url + route_path` (or the derived base).
- Query/path/body/headers are mapped using the same DSL input definitions.
- Response handling follows the target route `responses` contract.
- If the target route cannot be resolved, generation fails.

### Errors
- Calling a non‑existing service → compile error.
- Runtime HTTP failures are converted into typed client errors.
