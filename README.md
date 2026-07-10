# CRAFT Core

Composable Runtime for Agentic Framework Tooling.

CRAFT is a Rust-native foundation for local-model expertise harnesses: reusable packages that bundle prompts, model requirements, memory shape, tools, and validators for a specific capability such as `godot-designer`, `java-programmer`, or `tdd-architect`.

## Current Scope

This repository currently provides:

- A Rust workspace with `craft-cli`, `craft-core`, `craft-lsp`, `craft-manifest`, `craft-memory`, `craft-web`, `craft-bridge`, and `craft-registry`.
- A `craft` CLI with `init`, `doctor`, `version`, harness registry commands, composition, and help output.
- A `craft.toml` manifest parser and validator.
- GitHub harness installation into `~/.craft/harnesses`.
- A persistent SQLite harness registry at `~/.craft/registry.sqlite3`.
- `craft compose` output plus `craft compose-plan` dry runs that merge prompt, memory, MCP tool, and validator artifacts with ordered conflict warnings.
- `craft validate` and `craft harness test <name>` for manifest checks plus `tdd-dsl` validator execution.
- A minimal `craft lsp` stdio language server for `craft.toml` diagnostics and manifest completions.
- A persistent scoped memory store backed by `~/.craft/memory.sqlite3` plus JSONL event logs.
- Optional encrypted distributed-memory primitives in `craft-memory` behind the `crypto` feature.
- A local `craft-web` dashboard API for harness browsing, visual composition flows, memory inspection, runtime status, and validation websockets.
- A `craft-bridge` crate for A2A agent discovery/task routes and MCP JSON-RPC/tool/resource/prompt surfaces.
- A draft `craft-registry` cloud harness registry with PostgreSQL schema, Axum handlers, JWT/access-token auth, package storage, and CLI scaffolding.
- Installable starter cartridges published as separate `JMoak/craft-*` repositories.
- CI for format, clippy, and tests.

## Quickstart

```sh
cargo run -p craft-cli -- init
cargo run -p craft-cli -- doctor
cargo run -p craft-cli -- harness list
cargo run -p craft-cli -- run craft.compose.toml --model llama3.1:8b
cargo run -p craft-cli -- lsp
cargo run -p craft-web
cargo test
```

`craft init` creates:

```text
.craft/
craft.toml
prompts/system.md
memory/schema.toml
tools/mcp.toml
validators/checks.tdd
```

## Harness Registry

Install a harness from GitHub:

```sh
craft harness install github:Rosavera-I/craft-godot-designer
craft harness install github:Rosavera-I/craft-godot-designer@v0.1.0
```

Inspect and remove installed harnesses:

```sh
craft harness list
craft harness info godot-designer
craft harness uninstall godot-designer
```

Validate a local harness project:

```sh
craft validate
craft validate path/to/harness
```

Run validators for an installed harness:

```sh
craft harness test godot-designer
```

### Registry packages

After authenticating with `craft login`, publish the current validated harness and
install a package from the configured registry:

```sh
craft harness publish --org acme
craft harness install acme/godot-designer       # highest non-yanked semver
craft harness install acme/godot-designer@1.2.3 # exact version
craft harness install acme/godot-designer@^1.2  # highest compatible version
```

`craft publish` and `craft install` remain short aliases. A successful registry
install verifies the server-provided SHA-256 before extraction, records the local
installation in `CRAFT_HOME`, and atomically writes or updates `craft.lock` in the
current project. The lockfile stores the resolved exact version, registry source,
and archive checksum so ranges remain reproducible.

Validation first checks `craft.toml` and referenced artifacts. If `validators/checks.tdd` contains non-comment checks, CRAFT runs `tdd-dsl` from `PATH`; if that binary is not available, it tries `python -m tdd_dsl`.

Compose multiple installed harnesses into a generated config:

```sh
craft compose-plan godot-designer roguelike-specialist
craft compose godot-designer roguelike-specialist --plan
craft compose godot-designer roguelike-specialist -o craft.compose.toml
```

`compose-plan` previews the ordered merge without writing a compose file. It lists each harness source, artifact paths, merge strategy, and warnings such as duplicate harness entries. `craft compose --plan` remains available as the compatibility spelling.

The generated `craft.compose.toml` includes harness metadata plus merged artifacts:

```toml
[compose]
strategy = "ordered-merge"
harnesses = ["godot-designer", "roguelike-specialist"]

[prompts]
system = "# Harness: godot-designer\n\n...\n\n# Harness: roguelike-specialist\n\n...\n"

[memory.schemas]
"godot-designer" = "[facts]\n...\n"
"roguelike-specialist" = "[facts]\n...\n"

[tools.mcp]
"godot-designer" = "[[server]]\n...\n"

[validators.tdd]
"godot-designer" = "check ...\n"
```

Prompts are concatenated in command order. Memory schemas, MCP bindings, and TDD validators are namespaced by harness name so a runner can consume each artifact without losing source ownership.

The registry uses SQLite through native `rusqlite` calls and prepared statements. Runtime errors include stable CLI error codes such as `error[io]`, `error[sqlite]`, and `error[manifest]` while preserving the underlying source error for library consumers.

Run a composed harness against a local runtime:

```sh
craft run craft.compose.toml --model llama3.1:8b --prompt "Review this Godot scene"
craft run craft.compose.toml --model qwen2.5:7b --runtime ollama
```

`craft run` reads the merged `[prompts].system` value from `craft.compose.toml` and passes it to `ollama run <model>` by default. Use `--runtime` to point at another Ollama-compatible local runtime binary.

## Language Server

Start the `craft.toml` language server over stdio:

```sh
craft lsp
```

The current server supports LSP `initialize`, `shutdown`, manifest diagnostics for opened or saved documents, completion labels for known CRAFT sections and fields, and a null-safe definition response. The protocol adapter lives in `craft-lsp` so editor integration can grow without bloating the CLI binary.

## Starter Cartridges

Starter harness cartridges live in their own repositories so they can be installed, versioned, validated, and evolved independently from CRAFT Core:

- [`craft-godot-designer`](https://github.com/Rosavera-I/craft-godot-designer) for Godot 4 gameplay and scene review.
- [`craft-tdd-architect`](https://github.com/Rosavera-I/craft-tdd-architect) for turning feature intent into executable behavior contracts.
- [`craft-rust-maintainer`](https://github.com/Rosavera-I/craft-rust-maintainer) for Rust review, maintenance, and release hygiene.

Each cartridge repo includes a `craft.toml`, system prompt, memory schema, MCP tool config template, and TDD validator file so it can be installed and composed like any other harness.

See `docs/HARNESS-LIFECYCLE.md` for the public harness lifecycle, artifact contract, and release checklist.
See `docs/CARTRIDGE-STARTERS.md` for the five-cartridge dogfood plan and split-out order.
See `docs/STACK-DOGFOOD.md` for the current cross-cartridge improvement loop and release-readiness flow.
See `docs/M3-IMPLEMENTATION.md` for the web dashboard, distributed memory, A2A/MCP bridge, and cloud registry implementation notes.

## Memory Store

Record and recall scoped facts:

```sh
craft memory log project language rust
craft memory recall project language
```

The longer flag-based commands remain available for search-style recall and inspection:

```sh
craft memory record --scope project --key language --value rust
craft memory recall --scope project --query rust
craft memory inspect --scope project
craft memory search --query rust --scope project
```

Memory persists in SQLite under `CRAFT_HOME` or `~/.craft`, and each recorded fact also appends a replayable `fact.recorded` event to the database and JSONL log stream.

## Manifest Shape

```toml
[harness]
name = "godot-designer"
version = "0.1.0"
description = "Godot 4 expertise harness"
authors = ["JMoak"]

[model]
min_context = 4096
recommended = ["llama3.1:8b", "qwen2.5:7b"]

[prompts]
system = "prompts/system.md"

[memory]
schema = "memory/schema.toml"

[tools]
mcp = "tools/mcp.toml"

[validators]
tdd = "validators/checks.tdd"
```

## Design Notes

The first pass is dependency-light so the workspace can build in restricted/offline environments. The crate boundaries intentionally preserve the path to add `clap`, `serde`, `toml`, `semver`, and richer diagnostics once CI/network constraints are in place.
