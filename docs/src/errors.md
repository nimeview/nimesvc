# Errors and Responses

This chapter explains how errors are represented in generated services and runtime responses.

## HTTP Error Format (Server)
Runtime validation and handler failures are returned as JSON:
```json
{ "error": "message" }
```

### Validation examples
- Missing required field:
```json
{ "error": "Missing body.user" }
```
- Invalid value:
```json
{ "error": "Invalid user.email" }
```
- Invalid path/query:
```json
{ "error": "Invalid path.id" }
```
```json
{ "error": "Invalid query.limit" }
```

## Responses in DSL
```ns
GET "/users/{id}":
    responses:
        200 User
        404 Error
        401 AuthError
    call users.get(path.id)
```

Rules:
- Any `2xx` response is a success.
- Any non‑`2xx` response is returned as an HTTP error payload.
- Use `204` for empty responses.

## Auth Errors
- Missing `Authorization` with `auth bearer` → `401`.
- Missing `x-api-key` with `auth api_key` → `401`.
