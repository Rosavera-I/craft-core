# CRAFT Core

Composable Runtime for Agentic Framework Tooling.

CRAFT is a Rust-native foundation for local-model expertise harnesses: reusable packages that bundle prompts, model requirements, memory shape, tools, and validators for a specific capability such as `godot-designer`, `java-programmer`, or `tdd-architect`.

## Current Scope

This repository currently provides:

- A Rust workspace with `craft-cli`, `craft-core`, `craft-lsp`, `craft-manifest`, and `craft-memory`.
- A `craft` CLI with `init`, `doctor`, `version`, harness registry commands, composition, and help output.
- A `craft.toml` manifest parser and validator.
- GitHub harness installation into `~/.craft/harnesses`.
- A persistent SQLite harness registry at `~/.craft/registry.sqlite3`.
- `craft compose` output that merges prompt, memory, MCP tool, and validator artifacts with ordered conflict warnings.
- `craft validate` and `craft harness test <name>` for manifest checks plus `tdd-dsl` validator execution.
- A minimal `craft lsp` stdio language server for `craft.toml` diagnostics and manifest completions.
- A persistent scoped memory store backed by `~/.craft/memory.sqlite3` plus JSONL event logs.
- Installable starter cartridges published as separate `JMoak/craft-*` repositories.
- CI for format, clippy, and tests.

## Quickstart

```sh
cargo run -p craft-cli -- init
cargo run -p craft-cli -- doctor
cargo run -p craft-cli -- harness list
cargo run -p craft-cli -- run craft.compose.toml --model llama3.1:8b
cargo run -p craft-cli -- lsp
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
craft harness install github:JMoak/craft-godot-designer
craft harness install github:JMoak/craft-godot-designer@v0.1.0
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

Validation first checks `craft.toml` and referenced artifacts. If `validators/checks.tdd` contains non-comment checks, CRAFT runs `tdd-dsl` from `PATH`; if that binary is not available, it tries `python -m tdd_dsl`.

Compose multiple installed harnesses into a generated config:

```sh
craft compose godot-designer roguelike-specialist -o craft.compose.toml
```

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

- `JMoak/craft-godot-designer` for Godot 4 gameplay and scene review.
- `JMoak/craft-tdd-architect` for turning feature intent into executable behavior contracts.
- `JMoak/craft-rust-maintainer` for Rust review, maintenance, and release hygiene.

Each cartridge repo includes a `craft.toml`, system prompt, memory schema, MCP tool placeholder, and TDD validator file so it can be installed and composed like any other harness.

See `docs/CARTRIDGE-STARTERS.md` for the five-cartridge dogfood plan and split-out order.

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
