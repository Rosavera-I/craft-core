# CRAFT Cartridge Starters

Starter cartridges should prove that CRAFT harnesses are useful as installable expertise packages, not just manifest fixtures. Build them small, validate them with `craft validate`, and compose them together as soon as two exist.

## Recommended Dogfood Order

1. `craft-tdd-architect`
2. `craft-godot-designer`
3. `craft-rust-maintainer`
4. `craft-codebase-cartographer`
5. `craft-security-reviewer`

## `craft-tdd-architect`

Core idea: turn rough feature intent into executable test contracts and review loops.

- Repo: `JMoak/craft-tdd-architect`.
- Prompt: red-green-refactor framing, crisp acceptance criteria, and edge-case enumeration.
- Memory schema: project testing conventions, fixture locations, known flaky tests.
- Tools: filesystem plus test runner command bindings.
- Validators: emits at least one concrete test file path, names expected failing behavior first, avoids implementation-first output.
- Acceptance: `craft validate` passes; `craft compose tdd-architect -o craft.compose.toml`; `craft run ... --prompt "Add tests for manifest validation"` produces a test plan with runnable command suggestions.

## `craft-godot-designer`

Core idea: Godot 4 gameplay and systems design harness aligned with Jordan's indie game work.

- Repo: `JMoak/craft-godot-designer`.
- Prompt: Godot 4.4, signal-driven architecture, data-driven resources, scene ergonomics.
- Memory schema: project autoloads, node conventions, physics constraints, input maps.
- Tools: filesystem, ripgrep, optional Godot headless validation command.
- Validators: rejects Unity-style lifecycle advice, requires scene/script ownership notes, requires test or manual verification plan.
- Acceptance: validates locally and can review a scene/system prompt without losing Godot-specific constraints.

## `craft-rust-maintainer`

Core idea: Rust implementation partner for small CLI/workspace crates like CRAFT itself.

- Repo: `JMoak/craft-rust-maintainer`.
- Prompt: std-first, explicit error types, narrow tests, fmt/test/clippy gates.
- Memory schema: crate boundaries, lint policy, dependency policy, release target.
- Tools: cargo, rustfmt, clippy, filesystem.
- Validators: requires verification commands and refuses unchecked dependency additions.
- Acceptance: dogfood against CRAFT by generating a scoped implementation brief for one small command.

## `craft-codebase-cartographer`

Core idea: quickly map unfamiliar repos into architecture, workflows, risks, and next edits.

- Repo: `JMoak/craft-codebase-cartographer`.
- Prompt: read-before-changing, identify entrypoints, summarize contracts and tests.
- Memory schema: repo map, commands, ownership boundaries, open risks.
- Tools: filesystem, ripgrep, git.
- Validators: requires file references and separates evidence from inference.
- Acceptance: produces a useful CRAFT architecture map with concrete file references.

## `craft-security-reviewer`

Core idea: focused security review cartridge for local code and agent tooling.

- Repo: `JMoak/craft-security-reviewer`.
- Prompt: secrets, command execution, injection, path traversal, auth boundaries, privacy leaks.
- Memory schema: trust tiers, sensitive paths, external surfaces, known mitigations.
- Tools: ripgrep, dependency inspection, git diff.
- Validators: findings-first output, severity labels, no vague advice without a concrete sink.
- Acceptance: reviews CRAFT registry/runtime paths and identifies only evidence-backed issues.

## First Build

Start with `craft-tdd-architect` because it strengthens every later cartridge and gives CRAFT a self-improving validation loop. Keep each repo tiny:

```text
craft.toml
prompts/system.md
memory/schema.toml
tools/mcp.toml
validators/checks.tdd
examples/
```

Then build `craft-godot-designer` as the first Jordan-domain proof cartridge.
