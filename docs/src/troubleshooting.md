# Troubleshooting

## `doctor` reports missing tools

Install the missing runtime or toolchain first:
- Rust for Rust generation
- Go for Go generation
- Node and Bun for TypeScript generation
- `mdbook` for documentation builds

Then run:

```bash
nimesvc doctor
```

## Generation fails because a module function cannot be found

Check that:
- the module path in `use` is correct
- the file exists
- the referenced function name exists in that module
- the function signature matches what the generated handler expects

## A cross-service call fails during generation

Check that:
- the target service exists in the same DSL file
- the target route exists
- the referenced path, query, body, and header fields are declared
- `base_url` is valid if you override it

## WebSocket connection works but messages fail

Check that:
- the inbound message type is declared
- custom message kinds use `trigger`
- the payload shape matches the declared type
- required socket headers or auth values are present
- room-bound triggers use a declared room name

## gRPC generation or runtime fails

Check that:
- the `rpc` block appears before `service`
- the target service name exists
- `output` is declared
- your language-specific gRPC dependencies are installed

## Formatting or linting fails

Run:

```bash
nimesvc fmt api.ns
nimesvc lint api.ns
```

Formatting issues are usually safe to auto-fix. Lint warnings often point to unused modules, empty services, or incomplete event wiring.
