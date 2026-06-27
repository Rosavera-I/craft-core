# CRAFT Core

Composable Runtime for Agentic Framework Tooling.

CRAFT is a Rust-native foundation for local-model expertise harnesses: reusable packages that bundle prompts, model requirements, memory shape, tools, and validators for a specific capability such as `godot-designer`, `java-programmer`, or `tdd-architect`.

## Current Scope

This repository currently provides:

- A Rust workspace with `craft-cli`, `craft-core`, `craft-manifest`, and `craft-memory`.
- A `craft` CLI with `init`, `doctor`, `version`, harness registry commands, composition, and help output.
- A `craft.toml` manifest parser and validator.
- GitHub harness installation into `~/.craft/harnesses`.
- A persistent SQLite harness registry at `~/.craft/registry.sqlite3`.
- Basic `craft compose` output with last-write-wins conflict warnings.
- A scoped memory API stub that establishes the interface for later persistence.
- CI for format, clippy, and tests.

## Quickstart

```sh
cargo run -p craft-cli -- init
cargo run -p craft-cli -- doctor
cargo run -p craft-cli -- harness list
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

Compose multiple installed harnesses into a generated config:

```sh
craft compose godot-designer roguelike-specialist -o craft.compose.toml
```

The registry uses SQLite. This dependency-light milestone calls the `sqlite3` CLI so the workspace remains buildable offline; the API is shaped so a future `rusqlite` backend can replace it without changing CLI behavior.

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
