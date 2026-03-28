# Quick Start

This guide gets you from an empty folder to a running HTTP service.

## 1. Init
```bash
nimesvc init
```

This declares one service with one health route.

## 2. Generate a server
```bash
nimesvc generate main.ns rust
```

By default, output goes to `.nimesvc/<ServiceName>`.

## 3. Run it
```bash
nimesvc run main.ns
```

For an edit-and-restart loop, use:

```bash
nimesvc dev main.ns
```

## 4. Call the route
```bash
curl http://127.0.0.1:8080/health
```

You should get `200 OK`.

## 5. Add a module-backed route

When you want real business logic, add a module file and call it from the DSL:

```ns
use core "./modules/core.rs"

service API:
    GET "/hello":
        responses:
            200 string
        call core.hello
```

Create `./modules/core.rs`:

```rust
pub async fn hello() -> Result<String, axum::http::StatusCode> {
    Ok("hello".to_string())
}
```

Then regenerate:

```bash
nimesvc generate main.ns rust
```

## Next steps
- Add shared types and validation rules in [Types and Validation](types.md).
- Add auth or middleware in [Auth and Middleware](auth_middleware.md).
- See more complete snippets in [Examples](examples.md).
