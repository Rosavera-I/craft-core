# CRAFT Architecture

CRAFT separates the core runtime from individual harness repositories.

## System Map

```mermaid
flowchart LR
    User[Developer] --> CLI[craft-cli]
    CLI --> Core[craft-core]
    CLI --> LSP[craft-lsp]
    CLI --> Memory[craft-memory]
    Core --> Manifest[craft-manifest]
    Core --> Registry[(registry.sqlite3)]
    Core --> Harnesses[~/.craft/harnesses]
    Harnesses --> CartridgeA[craft-godot-designer]
    Harnesses --> CartridgeB[craft-tdd-architect]
    Harnesses --> CartridgeC[craft-rust-maintainer]
    Core --> Compose[craft.compose.toml]
    Compose --> Runtime[Local runtime: ollama-compatible]
    Memory --> MemoryDb[(memory.sqlite3)]
    Memory --> Logs[JSONL event logs]
    LSP --> Editor[Editor diagnostics and completions]
```

The CLI is the integration point. The library crates keep manifest parsing, harness loading, memory persistence, and LSP behavior separate so each surface can mature without turning the command binary into the architecture.

## Repositories

- `craft-core`: CLI, manifest model, composition engine, memory interfaces, runners, and developer tooling.
- `craft-<harness>`: one repository per expertise harness.

## Crates

- `craft-cli`: command-line entry point.
- `craft-manifest`: `craft.toml` parsing and validation.
- `craft-core`: harness project loading and future composition APIs.
- `craft-lsp`: stdio language-server protocol adapter for `craft.toml`.
- `craft-memory`: scoped SQLite memory store and JSONL event log.

## Install, Compose, Run

```mermaid
sequenceDiagram
    actor Dev as Developer
    participant CLI as craft-cli
    participant Core as craft-core
    participant Git as GitHub
    participant Reg as registry.sqlite3
    participant Runtime as Local runtime

    Dev->>CLI: craft harness install github:owner/repo[@ref]
    CLI->>Core: parse GithubSource
    Core->>Git: git clone --depth 1
    Core->>Core: load and validate craft.toml
    Core->>Reg: upsert installed harness
    Dev->>CLI: craft compose a b --plan
    CLI->>Core: plan_composition(names)
    Core->>Reg: resolve installed harnesses
    Core-->>CLI: ordered merge plan and warnings
    Dev->>CLI: craft compose a b -o craft.compose.toml
    CLI->>Core: compose_harnesses(names, output)
    Core->>Reg: resolve installed harnesses
    Core->>Core: merge prompt, memory, tools, validators
    Core-->>CLI: write compose file
    Dev->>CLI: craft run craft.compose.toml --model llama3.1:8b
    CLI->>Runtime: ollama run model prompt
```

`craft compose --plan` is the safe inspection path for cartridge stacks. It performs the same registry and manifest resolution as `craft compose`, reports the `ordered-merge` strategy, lists artifact paths per harness, and returns duplicate-harness warnings without writing `craft.compose.toml`.

## Composition Contract

```mermaid
flowchart TB
    subgraph Inputs[Installed Harnesses]
        H1[Harness A<br/>craft.toml]
        H2[Harness B<br/>craft.toml]
        H3[Harness C<br/>craft.toml]
    end

    H1 --> PromptMerge[prompts.system<br/>concatenate in CLI order]
    H2 --> PromptMerge
    H3 --> PromptMerge

    H1 --> MemoryMerge[memory.schemas<br/>namespace by harness name]
    H2 --> MemoryMerge
    H3 --> MemoryMerge

    H1 --> ToolMerge[tools.mcp<br/>namespace by harness name]
    H2 --> ToolMerge
    H3 --> ToolMerge

    H1 --> ValidatorMerge[validators.tdd<br/>namespace by harness name]
    H2 --> ValidatorMerge
    H3 --> ValidatorMerge

    PromptMerge --> Output[craft.compose.toml]
    MemoryMerge --> Output
    ToolMerge --> Output
    ValidatorMerge --> Output
```

Composition deliberately treats prompt order as meaningful and keeps non-prompt artifacts namespaced. That keeps source ownership visible for runners and avoids silently flattening memory schemas, MCP bindings, or validators from multiple cartridges.

## Memory Flow

```mermaid
flowchart LR
    CLI[craft memory] --> API[craft-memory API]
    API --> Validate[scope and key validation]
    Validate --> Tx[SQLite transaction]
    Tx --> Facts[(facts + facts_fts)]
    Tx --> Events[(events)]
    Tx --> Audit[(audit_log)]
    Events --> Jsonl[logs/events-YYYY-MM-DD.jsonl]
    Facts --> Recall[recall/search/context]
    Audit --> Review[operational audit trail]
```

Facts are stored in SQLite, indexed through FTS, and mirrored by replayable events. `memory context` assembles scoped facts in deterministic priority order so future runners can request bounded context without knowing the storage layout.

## Milestone Path

1. Foundation: buildable workspace, CLI, manifest validation.
2. Harness discovery: install/list/info/uninstall harness repos.
3. Memory: SQLite plus JSONL event log with scoped retrieval.
4. Runner: local model adapters and tool bindings.
5. Validation: `tdd-dsl` powered harness tests.
6. Developer experience: `craft.toml` LSP diagnostics, completions, and harness navigation.
