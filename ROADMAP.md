# CRAFT Roadmap

> Composable Runtime for Agentic Framework Tooling

## M1 Complete ✅

**Status**: 3 commits ready to push

- `a9538b7` - feat: real craft compose artifact merging
- `6ffb5a7` - feat: persistent memory backend with MemoryStore trait  
- `38e91a5` - Implement craft validate and craft harness test commands with tdd-dsl integration

**Validation**: All 21 tests passing, fmt clean, clippy clean

---

## M2: Hardening & Runtime ✅

### P0: Critical Path

| Feature | Impact | Effort | Description |
|---------|--------|--------|-------------|
| **rusqlite Migration** | High | Medium | ✅ Replaced shell-based SQLite with native rusqlite calls and prepared statements. Eliminates manual quoting risks and improves error propagation. |
| **craft run Command** | High | Medium | ✅ Execute composed harness configurations: `craft run craft.compose.toml --model llama3.1:8b`. Initial CLI runtime bridge implemented for Ollama-compatible local runtimes. |
| **Error Handling Hardening** | High | Low | ✅ Structured core, memory, and CLI errors now expose stable error codes, preserve source errors, and print coded CLI diagnostics. |

### P1: Important

| Feature | Impact | Effort | Description |
|---------|--------|--------|-------------|
| **LSP Integration** | High | High | Language server for craft.toml files: autocomplete, validation, go-to-definition for harness refs. |
| **Harness Version Resolution** | Medium | Medium | Support semver ranges in harness dependencies, version conflict resolution. |
| **Compose Conflict Strategies** | Medium | Low | Add `merge`, `override`, `fail` strategies for artifact conflicts beyond ordered-merge. |
| **Memory Event Streaming** | Medium | Low | Real-time JSONL event streaming for external tools to consume memory changes. |

### P2: Nice to Have

| Feature | Impact | Effort | Description |
|---------|--------|--------|-------------|
| **Harness Marketplace** | High | High | Public harness registry, search, ratings, official JMoak harnesses gallery. |
| **Workspace Support** | Medium | Medium | Multi-harness projects with shared dependencies and cross-references. |
| **TDD DSL Expansion** | Medium | Medium | Richer validator syntax: `check`, `ensure`, `forbid`, `pattern`, `semantic`. |
| **MCP Server Bridge** | Medium | High | Export craft memory/harnesses as MCP servers for external tool consumption. |

---

## M3: Ecosystem (Future)

- **Web Dashboard**: Visual harness composition, memory inspection
- **Distributed Memory**: Sync memory across machines via encrypted channels
- **A2A Protocol Bridge**: Interoperability with other agent frameworks
- **Cloud Harness Hosting**: Private harness registries for teams

---

## Recommended M2 Execution Order

```
1. rusqlite Migration (foundation) ✅
2. Error Handling Hardening (quality) ✅
3. craft run Command (user value) ✅
4. Compose Conflict Strategies (polish)
5. Harness Version Resolution (ecosystem)
6. LSP Integration (developer experience)
```

---

## Technical Debt Notes

- TDD runner detection is MVP-level; needs richer integration
- Memory context assembly uses simple token estimation (~4 chars/token)
- No concurrency controls on SQLite (WAL mode helps but isn't sufficient for multi-process)

---

*Last updated: 2026-06-28*
