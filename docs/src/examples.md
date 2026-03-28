# Examples

This chapter shows small focused snippets. For larger real projects, see the `examples/` directory.

## 1. Minimal HTTP service
```ns
service API:
    config:
        address: "127.0.0.1"
        port: 8080

    GET "/health":
        response 200
        healthcheck
```

## 2. Typed route inputs and responses
```ns
type User:
    id: string
    name: string

type Error:
    message: string

service API:
    GET "/users/{id}":
        input:
            path:
                id: string
        responses:
            200 User
            404 Error
        call users.get(path.id)
```

## 3. Auth and middleware
```ns
service API:
    auth bearer
    middleware logging

    GET "/profile":
        responses:
            200 User
        call users.profile
```

## 4. Event producer and consumer
```ns
event UserCreated:
    payload User

type User:
    id: string
    email: string

service API:
    events_config:
        broker: redis
        url: "redis://127.0.0.1:6379"
        group: "api"
        consumer: "api-1"

    emit UserCreated
    subscribe UserCreated
```

## 5. RPC (gRPC)
```ns
rpc Auth.Login:
    input:
        username: string
        password: string
    output Token
    call auth.login(input.username, input.password)
```

## 6. WebSocket rooms and triggers
```ns
type ChatMessage:
    user_id: string
    text: string

service Chat:
    socket Chat "/ws":
        room "chat"

        trigger SendMessage:
            room "chat"
            payload ChatMessage

        inbound:
            Join -> chat.on_join
            SendMessage -> chat.send_message

        outbound:
            MessageOut -> chat.send_message
```

## Repository examples

- `examples/minimal.ns` — minimal HTTP service
- `examples/auth_data.ns` — auth and data services
- `examples/events_redis.ns` — Redis-backed events
- `examples/chat_socket.ns` — WebSocket flow
