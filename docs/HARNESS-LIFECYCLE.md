# Harness Lifecycle

CRAFT harnesses are independent repositories that move through a repeatable lifecycle: publish, install, validate, compose, run, and learn. Keeping each cartridge separate lets domain expertise evolve without coupling it to the core runtime release cadence.

## Lifecycle Map

```mermaid
flowchart LR
    Author[Harness author] --> Repo[Public craft-* repository]
    Repo --> Install[craft harness install]
    Install --> Registry[(registry.sqlite3)]
    Registry --> Validate[craft validate / craft harness test]
    Validate --> Compose[craft compose]
    Compose --> Run[craft run]
    Run --> Memory[craft memory record]
    Memory --> Refine[Prompt, schema, validator updates]
    Refine --> Repo
```

## Artifact Contract

```mermaid
flowchart TB
    Manifest[craft.toml] --> Prompt[prompts/system.md]
    Manifest --> Memory[memory/schema.toml]
    Manifest --> Tools[tools/mcp.toml]
    Manifest --> Validators[validators/checks.tdd]
    Prompt --> Compose[Composed prompt stack]
    Memory --> ComposeMemory[Namespaced memory schemas]
    Tools --> ComposeTools[Namespaced MCP bindings]
    Validators --> Test[Harness validator checks]
    Compose --> Runtime[Local model runtime]
    ComposeMemory --> Runtime
    ComposeTools --> Runtime
    Test --> Ready[Ready for use]
```

Every harness repository should stay small and inspectable:

```text
craft.toml
README.md
prompts/system.md
memory/schema.toml
tools/mcp.toml
validators/checks.tdd
docs/
```

## Current Starter Repositories

- [`Rosavera-I/craft-tdd-architect`](https://github.com/Rosavera-I/craft-tdd-architect): test-first behavior contracts, edge cases, fixture strategy, and red-green-refactor loops.
- [`Rosavera-I/craft-godot-designer`](https://github.com/Rosavera-I/craft-godot-designer): Godot 4 scene ownership, signal contracts, resources, tuning, and playtest feedback.
- [`Rosavera-I/craft-rust-maintainer`](https://github.com/Rosavera-I/craft-rust-maintainer): Rust review, small patches, API compatibility, verification commands, and release hygiene.

## Release Checklist

```mermaid
flowchart TD
    Change[Harness change] --> Manifest[Manifest still valid]
    Manifest --> Docs[README and docs explain workflow]
    Docs --> Prompt[Prompt gives domain-specific behavior]
    Prompt --> Schema[Memory schema captures reusable context]
    Schema --> Validators[Validators enforce output shape]
    Validators --> Validate[craft validate passes]
    Validate --> Compose[compose smoke test passes]
    Compose --> Tag[Commit, push, tag when stable]
```

Use this checklist for cartridge updates before promoting a new tag or recommending a harness in the core README.
