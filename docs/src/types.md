# Types and Validation

This chapter explains how NimeScript types are declared, composed, and validated at runtime.

## Primitive Types
- `string`
- `int`
- `float`
- `bool`

## Named Types
```ns
type User:
    id: int
    name: string
```
Named types are referenced by name in routes, rpc, and events.

## Optional Fields
```ns
type User:
    name?: string
```
Optional fields are omitted from validation when absent.

## Arrays
```ns
type Payload:
    ids: array<int>
```
Use `array` or `array<T>`. `array` defaults to `array<any>`.

## Map
```ns
type Metrics:
    tags: map<string>
```
`map<T>` is a dictionary with `string` keys and values of type `T`.

## Object Literals
```ns
type User:
    meta: { age: int, tags?: array<string> }
```
Inline object literals are useful for small nested schemas.

## Union / OneOf / Nullable
```ns
type UserId:
    value: union<string, int>

type Payment:
    method: oneof<Card, Cash>

type Profile:
    bio: nullable<string>
```

- `union<A,B>` allows any listed types.
- `oneof<A,B>` enforces exactly one variant in validation.
- `nullable<T>` allows `null` in addition to `T`.

## Enum
```ns
enum Status:
    Active
    Disabled

enum Code:
    Ok = 1
    Fail = 2
```
Enums can be string‑like or numeric.

## Versioned Names
```ns
type User@2:
    id: int
    email: string
```
Use `Name@<version>` for explicit versioned contracts. When `version` is set at the top level,
unversioned names automatically inherit it.

## Validation Rules
```ns
type User:
    name: string(min_len=2, max_len=50, regex="^[a-z]+$")
    email: string(email)
    age: int(min=0, max=150)
    ids: array<int>(min_items=1, max_items=10)
    tag: string(len=3)
```

Supported rules:
- `min`, `max`
- `min_len`, `max_len`, `len`
- `min_items`, `max_items`
- `regex`
- `format="email" | "uuid"`

Extra `key=value` pairs are preserved as `x-constraints` in OpenAPI.

## Runtime Behavior
- Route `path/query/body/headers` values are parsed into typed structs.
- Validation errors return `400` with a structured JSON payload (see Errors).
- RPC `input/headers` are validated the same way.
- Event payloads are validated on publish and consume.
