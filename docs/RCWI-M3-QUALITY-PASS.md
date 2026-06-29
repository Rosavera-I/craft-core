# RCWI: M3 Quality Pass

> Refined Context Workspace Item for Rosie Codex Bridge delegation
> Created: 2026-06-29
> Target: /home/moak/.openclaw/workspace/projects/craft-core-m1-20260627

## Context

CRAFT M3 implementation is present as an uncommitted multi-crate diff. The goal is to turn the current work into simple, readable, commitable increments with strong docs and honest verification.

The current branch is `feat/extract-example-cartridges`. The workspace contains new M3 crates and changes across:

- `crates/craft-memory`
- `crates/craft-web`
- `crates/craft-bridge`
- `crates/craft-registry`
- `docs/`
- workspace `Cargo.toml` and `Cargo.lock`

## Required Changes

### 1. Review and Prioritize

**Current Issue:** The current M3 diff is large and was produced by several implementation passes.

**Refactor/Add:** Review the diff for correctness, maintainability, risky placeholder code, excessive comments, `unwrap()`/panic paths in non-test code, inconsistent APIs, and docs drift.

**Files to inspect:** All modified and untracked project files.

**Output:** A prioritized fix list grouped by commit-sized increments.

### 2. Implement High-Value Fixes

**Current Issue:** The implementation may contain rough edges from broad parallel work.

**Refactor/Add:** Apply focused fixes where safe:

- Prefer straightforward, readable Rust.
- Avoid broad rewrites unless they remove real risk.
- Remove placeholder comments and speculative claims.
- Use comments only where they clarify non-obvious logic.
- Replace `unwrap()`/`expect()` in library/server paths with typed errors where practical.
- Keep test-only `unwrap()`/`expect()` acceptable when they make assertions clearer.

### 3. Documentation

**Current Issue:** `/docs` needs to reflect what was actually built.

**Refactor/Add:** Update and format docs for the M3 additions:

- Keep `docs/M3-IMPLEMENTATION.md` as the shipping implementation overview.
- Keep `docs/ARCHITECTURE.md` accurate for new crates.
- Ensure `docs/m3-design/*` does not overclaim incomplete functionality.
- Add concise crate-level docs where helpful, but avoid noisy inline comments.

### 4. Verification

**Current Issue:** Full offline test currently fails because `ipnetwork` for `sqlx-core v0.8.6` is missing from the local cargo cache.

**Refactor/Add:** Run all verification that is possible without network. Record blockers exactly.

Expected checks to attempt:

- `cargo fmt --all -- --check`
- `cargo test -p craft-memory --features crypto --offline`
- `cargo test -p craft-web --offline`
- `cargo test -p craft-bridge --offline`
- `cargo test -p craft-registry --offline`
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`

If a command is blocked by missing cached dependencies, record the package and command.

## Result

- Replaced registry CLI `unwrap()` paths for prompts, confirmations, auth token use, and `current_dir` with typed `RegistryError` propagation.
- Removed misleading placeholder/future-tense comments from memory sync, bridge task handling, and registry auth/handlers.
- Removed unused `tower-test` dev dependency from `craft-registry`; the remaining offline blocker is the uncached `ipnetwork` dependency pulled by `sqlx-core`.
- Added focused test/bench clippy allowances where unwraps are assertion/setup code, then cleaned benchmark imports and iteration.
- Updated `README.md`, `docs/M3-IMPLEMENTATION.md`, and `docs/A2A-MCP-BRIDGE.md` to describe shipped behavior, known limitations, verification commands, and commit grouping.

## Acceptance Criteria

- [x] High-priority correctness risks are fixed or documented as follow-up.
- [x] Code is simple and readable, with comments only where justified.
- [x] `/docs` accurately describes shipped M3 work and known limitations.
- [x] Formatting is clean.
- [x] Verification commands are run or blockers are precisely recorded.
- [x] Changes are organized into reasonable commit-sized groups.

## Deliverable

Use the standard RCB output shape:

```markdown
## Result
- concise summary

## Evidence
- concrete refs (files, lines, commands, behavior)

## Risks
- unresolved risks and edge cases

## Next Actions
1. immediate next step
2. optional follow-up

## Confidence
- overall: 0.00-1.00
- claims:
  - "<claim>": 0.00-1.00
```
