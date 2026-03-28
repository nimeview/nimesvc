# WebSocket

WebSocket services are defined with inbound/outbound message contracts. Each message is routed to a handler.

## DSL
```ns
service Chat:
    socket Chat "/ws":
        auth bearer
        middleware logging
        rate_limit 100/min
        headers:
            authorization: string

        inbound:
            Join -> chat.on_join
            MessageIn -> chat.on_message

        outbound:
            MessageOut -> chat.send_message
            UserJoined -> chat.user_joined
```

## Modules in WebSocket Handlers
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
The referenced module path is copied into the generated service under `modules/`.
If the handler function does not exist, generation fails and you need to add the implementation in your module.

## Module Examples (Logic)
### Rust
```rust
use crate::types::{MessageOut};

pub async fn on_join(ctx: ChatSocketContext, _payload: serde_json::Value) {
    ctx.send_message_out(MessageOut { text: Some("welcome".into()) }).await;
}

pub async fn on_message(ctx: ChatSocketContext, payload: serde_json::Value) {
    let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
    ctx.send_message_out(MessageOut { text: Some(format!("echo: {}", text)) }).await;
}
```

### Go
```go
package modules

import "encoding/json"

func OnJoin(ctx ChatSocketContext, _ json.RawMessage) {
	ctx.SendMessageOut(MessageOut{Text: "welcome"})
}

func OnMessage(ctx ChatSocketContext, payload json.RawMessage) {
	var body struct{ Text string `json:"text"` }
	_ = json.Unmarshal(payload, &body)
	ctx.SendMessageOut(MessageOut{Text: "echo: " + body.Text})
}
```

### TypeScript
```ts
import type { MessageOut } from "../types";

export async function onJoin(ctx: ChatSocketContext, _payload: any) {
  await ctx.sendMessageOut({ text: "welcome" } as MessageOut);
}

export async function onMessage(ctx: ChatSocketContext, payload: { text?: string }) {
  await ctx.sendMessageOut({ text: `echo: ${payload.text ?? ""}` } as MessageOut);
}
```

## Inbound vs Outbound
- **Inbound**: client → server messages. These trigger handler calls.
- **Outbound**: server → client messages. These define what the server can emit and generate helpers on the socket context.

## Rooms / Topics
```ns
socket Chat "/ws":
    room "chat"
    room "notifications"
```

Rooms are validated against the declared list. `topic` is an alias for `room`.

## Custom Triggers
If a message is not reserved, it must be declared as a trigger with a payload type.

```ns
socket Chat "/ws":
    room "chat"

    trigger SendMessage:
        room "chat"
        payload ChatMessage

    inbound:
        SendMessage -> chat.send_message
```

Rules:
- Each inbound/outbound message must include `-> module.func`.
- Custom messages must be declared using `trigger <Name>:`.
- `trigger` requires `payload <Type>`.
- `trigger room "..."` is optional, but if set it must exist in `room` list.

## Reserved Events
Reserved events are built‑in and do not require a `trigger` declaration:
- `Join`, `Exit`, `MessageIn`, `MessageOut`, `Typing`, `Ping`, `Pong`, `Auth`, `Subscribe`, `Unsubscribe`,
  `RoomJoin`, `RoomLeave`, `Ack`, `Receipt`, `UserJoined`, `UserLeft`, `Error`, `ServerNotice`.

### Reserved Payloads
Reserved events use predefined payload shapes:

- `Join`: `{}`
- `Exit`: `{}`
- `MessageIn`: `{ text?: string }`
- `MessageOut`: `{ text?: string }`
- `Typing`: `{ active?: bool }`
- `Ping`: `{}`
- `Pong`: `{}`
- `Auth`: `{ token?: string }`
- `Subscribe`: `{ topic?: string }`
- `Unsubscribe`: `{ topic?: string }`
- `RoomJoin`: `{ room?: string }`
- `RoomLeave`: `{ room?: string }`
- `Ack`: `{ id?: string }`
- `Receipt`: `{ id?: string }`
- `UserJoined`: `{ user_id?: string }`
- `UserLeft`: `{ user_id?: string }`
- `Error`: `{ message?: string }`
- `ServerNotice`: `{ message?: string }`

Reserved payload types are generated into `types.*` automatically.
If you need a strict payload shape, define a custom trigger instead.

## Handler Signature
Generated handlers receive:
- `ctx` — socket context (headers, send helpers, room helpers).
- `payload` — message body (`frame.data`).

Example (TypeScript):
```ts
export async function on_message(ctx: ChatSocketContext, payload: { text?: string }) {
  // ...
}
```

## Runtime Notes
- `Join` handler is called automatically on connection.
- `Exit` handler is called automatically on disconnect.
- `Ping` without a handler auto‑responds with `Pong`.
