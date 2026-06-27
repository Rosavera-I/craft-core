# CRAFT Architecture

CRAFT separates the core runtime from individual harness repositories.

## Repositories

- `craft-core`: CLI, manifest model, composition engine, memory interfaces, runners.
- `craft-<harness>`: one repository per expertise harness.

## Crates

- `craft-cli`: command-line entry point.
- `craft-manifest`: `craft.toml` parsing and validation.
- `craft-core`: harness project loading and future composition APIs.
- `craft-memory`: scoped memory interface and in-memory stub.

## Milestone Path

1. Foundation: buildable workspace, CLI, manifest validation.
2. Harness discovery: install/list/info/uninstall harness repos.
3. Memory: SQLite plus JSONL event log with scoped retrieval.
4. Runner: local model adapters and tool bindings.
5. Validation: `tdd-dsl` powered harness tests.
