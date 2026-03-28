# RPC (gRPC)

RPC services are declared in the DSL and generated as gRPC servers. Each RPC method is bound to a service by name.

## DSL
```ns
rpc Auth.Login:
    input:
        username: string
        password: string
    headers:
        x_request_id?: string
    output Token
    auth bearer
    middleware logging
    rate_limit 100/min
    call auth.login(input.username, input.password)
```

Rules:
- `rpc <Service>.<Method>:` must appear before `service` blocks.
- `<Service>` must match an existing service name.
- `output` is required.
- `call` is required and can only reference `input.*` or `headers.*`.

## Modules in RPC
```ns
use auth "./modules/auth.rs"

rpc Auth.Login:
    input:
        username: string
        password: string
    output Token
    call auth.login(input.username, input.password)
```
The referenced module path is copied into the generated service under `modules/`.
If the module function does not exist or the path is wrong, generation fails and you need to add the implementation yourself.

## Module Examples (Logic)
### Rust
```rust
use crate::types::{Token, AuthError};
use jsonwebtoken::{encode, EncodingKey, Header};

pub async fn login(username: String, password: String) -> Result<Token, AuthError> {
    if username.is_empty() || password.is_empty() {
        return Err(AuthError { message: "invalid credentials".into() });
    }
    let token = encode(
        &Header::default(),
        &serde_json::json!({ "sub": username }),
        &EncodingKey::from_secret(b"dev"),
    ).map_err(|_| AuthError { message: "token failed".into() })?;
    Ok(Token { access: token })
}
```

### Go
```go
package modules

import "errors"

func Login(username, password string) (Token, error) {
	if username == "" || password == "" {
		return Token{}, errors.New("invalid credentials")
	}
	return Token{Access: "dev-token"}, nil
}
```

### TypeScript
```ts
import type { Token, AuthError } from "../types";

export async function login(username: string, password: string): Promise<Token> {
  if (!username || !password) {
    throw { message: "invalid credentials" } as AuthError;
  }
  return { access: "dev-token" };
}
```

## Service Config (gRPC)
```ns
service Auth:
    grpc_config:
        address: "127.0.0.1"
        port: 50051
        max_message_size: 4mb
        tls "./cert.pem" "./key.pem"
```

`grpc_config` is per service and controls the gRPC listener. If omitted, generators fall back to their defaults
(see generated `main.*`).

## How RPC Calls Work
- RPC inputs are validated against the `input` block.
- The generated handler calls `call module.func(...)` and returns the result as the gRPC response.
- Headers defined in `headers:` are available as typed inputs in `call`.

## Generation
Generate only gRPC output:
```bash
nimesvc generate api.ns grpc --lang rust
nimesvc generate api.ns grpc --lang go
nimesvc generate api.ns grpc --lang ts
```

Generate the whole project and include gRPC automatically for RPC services:
```bash
nimesvc generate api.ns --lang rust
```

Run only gRPC output:

```bash
nimesvc run api.ns grpc --lang rust
```

Run the whole project and let `nimesvc` start gRPC automatically where `rpc` is declared:
```bash
nimesvc run api.ns
```

Notes:
- RPCs are generated into the service chosen by `service` name and language.
- Auth/middleware/rate_limit can be applied per RPC method.
