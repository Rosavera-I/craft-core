# CRAFT Web Dashboard

Visual harness composition and memory inspection interface for CRAFT (M3 Pillar 01).

## Overview

The CRAFT Web Dashboard provides a web-based interface for:
- **Harness Palette**: Browse and explore installed harnesses
- **Composition Canvas**: Visually compose harnesses with real-time validation
- **Memory Inspector**: Search and browse memory facts with FTS support
- **Runtime Monitor**: View system status and statistics

## Building

```bash
# Build the web server
cargo build -p craft-web

# Run tests
cargo test -p craft-web
```

## Running

```bash
# Start the server (default: http://127.0.0.1:3000)
cargo run -p craft-web

# With custom port
CRAFT_WEB_PORT=8080 cargo run -p craft-web

# With static file serving for frontend
cargo run -p craft-web -- CRAFT_WEB_STATIC=./ui/dist
```

Environment variables:
- `CRAFT_HOME` - Path to CRAFT home directory (default: `~/.craft`)
- `CRAFT_WEB_PORT` - Server port (default: `3000`)
- `CRAFT_WEB_HOST` - Server host (default: `127.0.0.1`)
- `CRAFT_WEB_STATIC` - Path to static files directory (optional)

## API Endpoints

### Harness Registry

```http
GET    /api/v1/harnesses                     # List all installed harnesses
GET    /api/v1/harnesses/{name}              # Get harness details
GET    /api/v1/harnesses/{name}/versions     # List all versions of a harness
```

### Composition

```http
POST   /api/v1/compose/plan                  # Preview composition without writing
POST   /api/v1/compose                       # Compose harnesses and write output
```

### Memory

```http
GET    /api/v1/memory/search?q=term         # Search memory facts with FTS
GET    /api/v1/memory/facts                  # List memory facts (paginated)
POST   /api/v1/memory/facts                 # Create a new memory fact
GET    /api/v1/memory/scope/{scope}         # Get facts by scope
```

### Runtime

```http
GET    /api/v1/status                       # Get runtime status and stats
```

### WebSocket

```http
WS     /ws/validate                         # Real-time composition validation
```

## WebSocket Protocol

The WebSocket endpoint uses JSON messages for real-time validation:

**Client sends:**
```json
{
  "type": "validate",
  "request": {
    "harness_names": ["harness1", "harness2"],
    "strategy": "ordered-merge"
  }
}
```

**Server responds with progress updates:**
```json
{
  "type": "progress",
  "harness_name": "harness1",
  "status": "validating"
}
```

**Validation result:**
```json
{
  "type": "result",
  "result": {
    "status": "Valid",
    "message": "Harness `harness1` is valid",
    "details": {"name": "harness1"}
  }
}
```

**Completion:**
```json
{
  "type": "complete"
}
```

## Response Format

All API responses use a consistent JSON wrapper:

**Success:**
```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

**Error:**
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "not_found",
    "message": "harness `test` not found"
  }
}
```

## Architecture

```
craft-web/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs           # Entry point and CLI
│   ├── lib.rs            # Public interfaces and types
│   ├── error.rs          # Error types and conversion
│   ├── server.rs         # Axum server setup
│   ├── api/
│   │   ├── mod.rs        # API utilities
│   │   ├── harness.rs    # Harness registry handlers
│   │   ├── composition.rs # Composition handlers
│   │   ├── memory.rs     # Memory service handlers
│   │   └── status.rs     # Runtime status handlers
│   ├── websocket/
│   │   └── mod.rs        # WebSocket handlers
│   └── memory/
│       └── mod.rs        # Memory service helpers
└── tests/
    └── integration.rs    # Integration tests
```

## Design Principles

1. **Production Quality**: Proper error types, structured logging, no shortcuts
2. **Consistent API**: All endpoints return the same JSON wrapper format
3. **CORS Ready**: Configured for local development with any frontend
4. **Real-time**: WebSocket for composition validation with progress updates
5. **Integration Tested**: Comprehensive tests for all endpoints

## Frontend Integration

The API is designed to work with any frontend framework. If you have a `craft-web-ui` crate with a Leptos WASM build, serve the static files:

```bash
# Serve static files from the UI dist folder
cargo run -p craft-web CRAFT_WEB_STATIC=./craft-web-ui/dist
```

The root path (`/`) will serve the static files, and API routes are prefixed with `/api/v1/`.
