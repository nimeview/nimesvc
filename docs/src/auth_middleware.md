# Auth and Middleware

Auth and middleware can be declared globally, per service, per route, or per socket.

## Auth
```ns
auth bearer
```
Supported:
- `bearer` (Authorization header)
- `api_key` (x-api-key header)
- `none` (explicitly disable auth for this route)

### Service‑level
```ns
service API:
    auth bearer

    GET "/me":
        responses:
            200 User
        call users.me
```

### Route‑level override
Routes use `auth:` (with colon):
```ns
service API:
    auth bearer

    GET "/admin":
        auth: bearer
        responses:
            200 string
        call admin.dashboard

    GET "/health":
        auth: none
        responses:
            200 string
        call core.health
```

### WebSocket
```ns
service Chat:
    socket Chat "/ws":
        auth bearer
        inbound:
            Join -> chat.on_join
```

## Middleware
```ns
middleware logging
```

### Service‑level
```ns
service API:
    middleware logging
    middleware metrics
```

### Route‑level
Routes use `middleware:` (with colon):
```ns
GET "/users":
    middleware: audit
    responses:
        200 User
    call users.list
```

### WebSocket
```ns
socket Chat "/ws":
    middleware logging
```

## Runtime Behavior
- `auth bearer` checks that the incoming request has an `Authorization` header.
- `auth api_key` checks that the incoming request has an `x-api-key` header.
- Auth is a presence check only; actual token validation should happen in your module logic.
- Middleware runs before handler calls.
- If middleware throws/returns an error, the request is rejected.
- For WebSocket, middleware runs on each inbound message.
