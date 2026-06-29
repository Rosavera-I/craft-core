# A2A/MCP Bridge

`craft-bridge` gives CRAFT an interoperability layer for agent ecosystems:

- A2A agent discovery, task lifecycle routes, task status history, and server-sent task events.
- A2A client helpers for agent card discovery and task operations.
- MCP JSON-RPC request/response parsing.
- MCP server metadata, tool, resource, prompt, and transport types.
- CLI helpers for serving protocol endpoints, discovering agents, and proxying harnesses.

## Commands

Run the focused verification suite:

```sh
cargo test -p craft-bridge --offline
```

Start an A2A server:

```sh
cargo run -p craft-cli -- bridge serve --protocol a2a --port 8080
```

Start an MCP server over HTTP:

```sh
cargo run -p craft-cli -- bridge serve --protocol mcp --port 8080
```

Start an MCP server over stdio:

```sh
cargo run -p craft-cli -- bridge serve --protocol mcp --stdio
```

Discover a remote A2A agent:

```sh
cargo run -p craft-cli -- bridge discover http://127.0.0.1:8080
```

## A2A Surface

The A2A server exposes:

| Route | Purpose |
|-------|---------|
| `GET /.well-known/agent.json` | Return the agent card |
| `POST /tasks/send` | Create a task from a user message |
| `GET /tasks/{id}` | Read task status |
| `POST /tasks/{id}/cancel` | Cancel a known task |
| `GET /tasks/{id}/events` | Stream task updates |

Core types include `AgentCard`, `Capabilities`, `Skill`, `Message`, `Part`, `Task`, `TaskStatus`, and `TaskState`.

## MCP Surface

The MCP layer includes JSON-RPC 2.0 parsing and typed protocol structures for:

| Area | Types |
|------|-------|
| Server metadata | `McpServer`, `ServerCapabilities`, `Implementation` |
| Tools | `Tool`, `ToolCapabilities`, `ToolResult`, `CallToolRequest` |
| Resources | `Resource`, `ResourceCapabilities`, `ResourceContent` |
| Prompts | `PromptTemplate`, `PromptCapabilities`, `GetPromptRequest` |
| Transport | `Transport::Stdio`, `Transport::Http` |

The HTTP handlers currently expose listing and read-style endpoints for tools, resources, and prompts. Full MCP request handling over `/mcp` is stubbed with `501 Not Implemented`; stdio JSON-RPC request handling is the primary implemented runtime path.

## Quality Notes

- Production bridge code passes the workspace `unwrap_used` and `expect_used` lint policy.
- Standalone bridge test binaries allow unwrap/expect locally because they are assertion-heavy protocol round-trip tests.
- Integration tests exercise Axum services in-process, avoiding fixed ports and network assumptions.
- The bridge depends on `craft-core`; focused bridge clippy also requires `craft-core` to remain lint-clean.
