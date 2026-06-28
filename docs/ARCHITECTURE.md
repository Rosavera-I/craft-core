# CRAFT Architecture

CRAFT separates the core runtime from individual harness repositories.

## Repositories

- `craft-core`: CLI, manifest model, composition engine, memory interfaces, runners, and developer tooling.
- `craft-<harness>`: one repository per expertise harness.

## Crates

- `craft-cli`: command-line entry point.
- `craft-manifest`: `craft.toml` parsing and validation.
- `craft-core`: harness project loading and future composition APIs.
- `craft-lsp`: stdio language-server protocol adapter for `craft.toml`.
- `craft-memory`: scoped SQLite memory store and JSONL event log.

## Milestone Path

1. Foundation: buildable workspace, CLI, manifest validation.
2. Harness discovery: install/list/info/uninstall harness repos.
3. Memory: SQLite plus JSONL event log with scoped retrieval.
4. Runner: local model adapters and tool bindings.
5. Validation: `tdd-dsl` powered harness tests.
6. Developer experience: `craft.toml` LSP diagnostics, completions, and harness navigation.
