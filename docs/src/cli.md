# CLI Reference

This chapter describes the main `nimesvc` commands and when to use them.

## `init`
```bash
nimesvc init
```
Creates a starter `main.ns` file and a local `.nimesvc/` directory.

## `build`
```bash
nimesvc build api.ns --output ./openapi
nimesvc build api.ns --output ./openapi --json
```
Builds OpenAPI 3.0.3 output from the HTTP part of the DSL.
- Default format: YAML
- `--json`: JSON output

## `generate`
Generate HTTP servers:
```bash
nimesvc generate api.ns rust --out ./.nimesvc
nimesvc generate api.ns ts --out ./.nimesvc
nimesvc generate api.ns go --out ./.nimesvc
```

Generate the whole project, including gRPC output for services that declare `rpc`:
```bash
nimesvc generate api.ns --lang rust
```

Generate only gRPC output:
```bash
nimesvc generate api.ns grpc --lang rust --out ./grpc-rust
```

Notes:
- Each service is generated into its own folder.
- If `output` is set in the DSL, `--out` can be omitted.
- gRPC output uses `<out>/<ServiceName>-grpc`.
- Without explicit `grpc`, `generate` also emits gRPC output automatically for services with `rpc`.

## `run`
```bash
nimesvc run api.ns
nimesvc run api.ns rust
nimesvc run api.ns --lang rust
```
Runs the generated services for the file. A positional language or `--lang` can override the language declared in the DSL.
If the project contains `rpc`, `run` also starts gRPC outputs automatically for those services.

Run only gRPC outputs:
```bash
nimesvc run api.ns grpc --lang rust --out ./grpc-rust
```

## `dev`
```bash
nimesvc dev api.ns rust
nimesvc dev api.ns --lang rust
nimesvc dev api.ns grpc --lang rust --out ./grpc-rust
```

`dev` watches the input `.ns` file, local module files referenced through `use`, and common environment files such as `.env` and `.env.local`, then restarts `nimesvc run` when they change.
Without explicit `grpc`, `dev` also starts gRPC outputs automatically for services with `rpc`.

Useful options:
- `--out <dir>` to control the output directory
- `--lang <rust|ts|go>` when the DSL does not declare a language or when you want a unified project-wide language override
- `--no-log` to avoid writing runtime logs
- `--debounce-ms <N>` to control polling frequency

Output model:
- `Dev: ...` lines are watcher status lines
- `[run] ...` lines are live stdout from `nimesvc run` and the generated service
- `[err] ...` lines are live stderr from `nimesvc run` and the generated service

Typical status flow:
- `scanning`: read the project and discover watched files
- `parsed`: the current DSL parses successfully
- `generating` / `generated`: refresh generated output before starting the service
- `watching`: print the current watch set
- `starting` / `running`: launch the generated service and attach live output
- `restart`: detect changed files and restart the service
- `failed`: keep watching after a parse or startup error
- `waiting`: project is currently invalid or the service exited; waiting for the next change

## `stop`
```bash
nimesvc stop api.ns
```
Stops running outputs for the file, including standard service outputs and gRPC outputs.

## `lint`
```bash
nimesvc lint api.ns
```
Runs static checks and project warnings.

Typical lint output includes:
- unused modules
- empty services
- event configuration without `emit` or `subscribe`

## `fmt`
```bash
nimesvc fmt api.ns
nimesvc fmt api.ns --check
```
Formats a DSL file or checks whether it already matches canonical formatting.

## `env`
```bash
nimesvc env api.ns
```

Prints environment requirements discovered from the DSL.

## `doctor`
```bash
nimesvc doctor
```
Checks local toolchains and validates `.ns` files found in the current project tree.

Doctor currently:
- reports missing Rust, Go, Node, and Bun tooling
- parses discovered `.ns` files
- validates module paths and references where possible

## `update`
```bash
nimesvc update
```
Updates the binary from GitHub Releases.
