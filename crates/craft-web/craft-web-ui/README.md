# craft-web-ui

Leptos WASM frontend for the CRAFT Web Dashboard.

## Overview

The UI provides:
- **Harness Palette**: Draggable list of installed harnesses
- **Composition Canvas**: Visual node editor for harness composition
- **Memory Inspector**: Full-text search and browsing of memory facts
- **Runtime Monitor**: Live system status and statistics

## Building

```bash
# Install wasm-pack (if not already installed)
cargo install wasm-pack

# Build the WASM module
wasm-pack build --target web

# Or with release optimizations
wasm-pack build --target web --release
```

## Development

```bash
# Use trunk for development server
cargo install trunk

# Start development server
trunk serve --open
```

## Project Structure

```
craft-web-ui/
├── Cargo.toml
├── index.html
├── README.md
└── src/
    ├── lib.rs          # Entry point and routing
    ├── api.rs          # API client for backend communication
    ├── components/     # Reusable UI components
    │   ├── mod.rs
    │   ├── layout.rs
    │   ├── navigation.rs
    │   ├── harness_palette.rs
    │   ├── composition_canvas.rs
    │   ├── memory_inspector.rs
    │   └── runtime_monitor.rs
    └── pages/          # Page-level components
        ├── mod.rs
        ├── home.rs
        ├── harness.rs
        ├── compose.rs
        └── memory.rs
```

## Features

### API Communication
- REST API integration via `web_sys::fetch`
- WebSocket for real-time validation (TODO)
- Automatic JSON serialization/deserialization

### State Management
- Leptos reactive signals
- Context-based dependency injection
- Scoped state per component

### Styling
- CSS-in-Rust with scoped styles per component
- Dark theme matching CRAFT aesthetic
- Responsive layout patterns

## Integration

To serve with the CRAFT backend:

```bash
# Build the frontend
cd crates/craft-web/craft-web-ui
wasm-pack build --target web

# Copy built assets to static directory
cp -r pkg ../../craft-web/static/

# Start the server with static file serving
cd ../../craft-web
cargo run -- CRAFT_WEB_STATIC=./static
```

## API Endpoints Used

- `GET /api/v1/harnesses` - List all harnesses
- `GET /api/v1/harnesses/{name}` - Get harness details
- `POST /api/v1/compose/plan` - Preview composition
- `GET /api/v1/memory/search?q=...` - Search memory facts
- `GET /api/v1/status` - Runtime status

## Browser Support

- Modern browsers with WASM support
- Chrome/Edge 80+, Firefox 78+, Safari 14+
- Requires ES6 module support
