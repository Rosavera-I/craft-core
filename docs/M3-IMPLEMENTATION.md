# M3 Implementation Notes

M3 adds the first production-shaped surfaces around CRAFT Core: a web dashboard API, encrypted distributed memory primitives, A2A/MCP protocol bridging, and the start of a cloud harness registry.

This document tracks what exists in code, how to run it, and what still needs hardening before calling the full milestone complete.

## Status

| Pillar | Crate | Current State | Verification |
|--------|-------|---------------|--------------|
| Web dashboard | `craft-web` | Axum API for harnesses, composition, memory, runtime status, and validation websocket | Workspace tests and clippy pass offline |
| Distributed memory | `craft-memory` | SQLite memory plus optional encrypted sync primitives, CRDTs, Merkle diffing, peer state | Workspace tests and clippy pass offline |
| A2A/MCP bridge | `craft-bridge` | A2A agent card/task server, A2A client types, MCP JSON-RPC server/types, CLI helpers | Workspace tests and clippy pass offline |
| Cloud registry | `craft-registry` | PostgreSQL schema, auth, server handlers, storage/git modules, CLI command surface | Workspace tests and clippy pass offline |

## Web Dashboard

`craft-web` exposes local CRAFT state over a JSON API.

```sh
cargo run -p craft-web
CRAFT_WEB_PORT=8080 cargo run -p craft-web
```

Environment:

| Variable | Default | Purpose |
|----------|---------|---------|
| `CRAFT_HOME` | `~/.craft` | CRAFT registry and memory root |
| `CRAFT_WEB_HOST` | `127.0.0.1` | Bind host |
| `CRAFT_WEB_PORT` | `3000` | Bind port |
| `CRAFT_WEB_STATIC` | unset | Optional static frontend directory |

Core endpoints:

| Endpoint | Purpose |
|----------|---------|
| `GET /api/v1/harnesses` | List installed harnesses |
| `GET /api/v1/harnesses/{name}` | Show installed harness details |
| `POST /api/v1/compose/plan` | Preview harness composition |
| `POST /api/v1/compose` | Write a composed harness config |
| `GET /api/v1/memory/search?q=...` | Search memory facts |
| `GET /api/v1/memory/facts` | List memory facts |
| `POST /api/v1/memory/facts` | Create a memory fact |
| `GET /api/v1/status` | Runtime status summary |
| `WS /ws/validate` | Real-time composition validation |

All REST responses use the shared `ApiResponse<T>` wrapper with `success`, `data`, and `error` fields.

## Distributed Memory

Distributed memory is feature-gated behind `craft-memory/crypto`.

```sh
cargo test -p craft-memory --features crypto --offline
```

Implemented pieces:

- AES-256-GCM symmetric encryption with random nonces.
- Real X25519 key exchange via `x25519-dalek`.
- Noise-style handshake state for deriving a shared sync secret.
- Last-write-wins registers for scalar fact conflicts.
- OR-Set and vector clock CRDT utilities.
- Merkle root/diff utilities for deciding whether peers need sync.
- Peer config/state tracking and sync reports.

The current network sync engine is still a local protocol skeleton. It validates peers, tracks checkpoints, and exposes the merge/diff primitives, but it does not yet open peer TCP connections or exchange batches over the wire.

## A2A/MCP Bridge

`craft-bridge` provides interoperability types and servers for agent protocols.

```sh
cargo test -p craft-bridge --offline
```

Implemented pieces:

- A2A agent cards, messages, task state, task artifacts, and auth metadata.
- Axum A2A server routes for agent discovery, task send/get/cancel, and task event streams.
- A2A client discovery and task calls.
- MCP JSON-RPC request/response parsing.
- MCP server capabilities, tools, resources, prompts, and transport config.
- CLI helper commands for starting A2A/MCP surfaces and inspecting remote A2A agents.

The integration tests exercise the Axum service in-process instead of binding fixed TCP ports, so they run in restricted sandboxes and regular development shells.

## Cloud Registry

`craft-registry` is the broadest M3 surface. It includes:

- Axum server routing for auth, users, orgs, teams, harnesses, versions, downloads, and tokens.
- PostgreSQL migrations for users, organizations, memberships, harnesses, versions, access tokens, and audit logs.
- JWT auth and hashed access tokens.
- Local package storage and Git integration modules.
- CLI shape for login, publish, install, search, org/team, and token management.

Registry integration tests require a PostgreSQL database. They skip cleanly unless `TEST_DATABASE_URL` is set, so normal offline workspace tests can run in restricted sandboxes while still supporting real database coverage in CI or a prepared local dev shell.

## Quality Pass Notes

The M3 cleanup pass keeps implementation claims tied to verified behavior:

- `craft-memory` has real X25519 shared-secret derivation and AES-256-GCM payload encryption, but peer TCP transport and batch exchange remain future hardening work.
- `craft-web` is currently verified as a backend API and websocket surface. The Leptos UI crate is present but not wired into a production static asset build.
- `craft-bridge` exposes protocol-compatible A2A/MCP surfaces and testable in-process handlers. Harness proxy execution is still a CLI/API integration task.
- `craft-registry` now compiles, tests, and passes library clippy offline. Database-backed integration behavior still needs a run with `TEST_DATABASE_URL` pointed at a disposable PostgreSQL database.

## Verification Commands

The following commands currently pass in the restricted workspace:

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --offline -- -D warnings
```

`cargo test --workspace --offline` includes `craft-registry` integration test binaries, but those database tests return early unless `TEST_DATABASE_URL` is set.

## Suggested Commit Split

Keep M3 reviewable by landing the work in dependency order:

1. `craft-memory` distributed sync primitives: `crates/craft-memory/Cargo.toml`, `src/crypto.rs`, `src/crdt/`, `src/sync/`, `tests/sync_integration.rs`, and `benches/sync_benchmark.rs`.
2. `craft-web` dashboard API: `crates/craft-web/`, workspace `Cargo.toml`, and dashboard-specific README/docs updates.
3. `craft-bridge` A2A/MCP interop: `crates/craft-bridge/`, `docs/A2A-MCP-BRIDGE.md`, and M3 docs entries for bridge behavior.
4. `craft-registry` cloud registry scaffold: `crates/craft-registry/`, registry migrations, and cloud-hosting docs. Include the version resolver fix, clean auth/client compilation, and `TEST_DATABASE_URL`-gated integration tests.
5. Cross-cutting docs and workspace polish: `README.md`, `docs/ARCHITECTURE.md`, `docs/M3-IMPLEMENTATION.md`, and design-doc updates that explain the milestone as shipped.

## Next Hardening Pass

1. Run `TEST_DATABASE_URL=postgres://... cargo test -p craft-registry --test integration_tests` against a disposable PostgreSQL database.
2. Split the M3 diff into reviewable commits by pillar.
3. Replace the distributed sync skeleton with a real peer transport and batch exchange.
4. Add deeper auth and persistence tests around registry handlers.
5. Wire `craft-web-ui` to a built static asset pipeline or keep the backend-only dashboard API explicit.
