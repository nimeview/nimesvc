# Events

Events provide an async contract for communication between services. The DSL declares event payload types,
while services choose which events they emit or subscribe to.

## DSL
```ns
event UserCreated:
    payload User

event AuditLogged:
    payload Audit

service API:
    emit UserCreated
    subscribe AuditLogged
```

Rules:
- `event` declarations must appear before `service` blocks.
- `payload` must reference a valid type.

## Modules with Events
```ns
use audit "./modules/audit.rs"

event UserCreated:
    payload User

service API:
    emit UserCreated
    subscribe UserCreated

    POST "/users":
        input:
            body:
                user: User
        responses:
            201 User
        call audit.create_user(body.user)
```
Events and handlers can live in the same module or in separate ones.

## Module Examples (Logic)
### Rust
```rust
use crate::types::{Audit};

pub async fn create_user(user: User) -> Result<User, ErrorResponse> {
    // business logic + emit event via generated helper
    emit_user_created(user.clone()).await;
    Ok(user)
}

pub async fn on_user_created(event: Audit) {
    println!("audit: {}", event.message);
}
```

### Go
```go
package modules

func CreateUser(user User) (User, error) {
	EmitUserCreated(user)
	return user, nil
}

func OnUserCreated(event Audit) {
	// process audit
}
```

### TypeScript
```ts
import type { User, Audit } from "../types";

export async function createUser(user: User): Promise<User> {
  await emitUserCreated(user);
  return user;
}

export async function onUserCreated(event: Audit) {
  console.log("audit", event.message);
}
```

## Broker (Redis Streams)
```ns
service API:
    events_config:
        broker: redis
        url: "redis://127.0.0.1:6379"
        group: "api"
        consumer: "api-1"
        stream_prefix: "api"
```

## In‑Memory vs Redis
- If `events_config` is **missing**, generators use an in‑memory event bus.
- If `events_config.broker = redis`, generators use Redis Streams.
- In‑memory mode is great for local development but does not persist events.

## Generated API
For each event `UserCreated`, generators provide:
- `emitUserCreated(payload)`
- `onUserCreated(handler)`

If the service subscribes to events and the broker is Redis, a consumer loop is generated:
- `startEventConsumers()`

## Runtime Behavior
- `emit` publishes the event payload.
- `subscribe` registers handlers in memory and starts consumers when configured.
- Redis Streams: consumer group is created on first run.
- Stream name is `{stream_prefix}.{EventName}` (defaults to service name).
- If Redis is not available, consumers keep retrying (server keeps running).
