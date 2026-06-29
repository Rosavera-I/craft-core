# M3 Ecosystem Architecture Overview

> Composable Runtime for Agentic Framework Tooling — Ecosystem Milestone

## Vision

M3 transforms CRAFT from a local harness runtime into a distributed, collaborative ecosystem where:
- Teams visually compose and share harnesses through a Web Dashboard
- Memory synchronizes securely across machines and team members
- Agents interoperate with external frameworks via standard protocols
- Private harness registries enable enterprise-grade distribution

## The Four Pillars

```mermaid
flowchart TB
    subgraph M3["M3 Ecosystem Architecture"]
        direction TB
        
        subgraph P1["🖥️ Web Dashboard"]
            W1[Visual Harness Composer]
            W2[Memory Inspector]
            W3[Runtime Monitor]
        end
        
        subgraph P2["🔗 Distributed Memory"]
            D1[Encrypted Sync]
            D2[Conflict Resolution]
            D3[Vector Search]
        end
        
        subgraph P3["🌉 A2A Protocol Bridge"]
            A1[Agent Card Discovery]
            A2[MCP Interop]
            A3[Task Delegation]
        end
        
        subgraph P4["☁️ Cloud Harness Hosting"]
            C1[Private Registries]
            C2[Version Management]
            C3[Team ACL]
        end
    end
    
    subgraph Core["CRAFT Core"]
        CR[craft-core]
        ME[craft-memory]
        CL[craft-cli]
    end
    
    Core --> M3
```

## Pillar Summary

| Pillar | Purpose | Key Challenge |
|--------|---------|---------------|
| **Web Dashboard** | Visual harness composition and system observability | Real-time composition preview with live validation |
| **Distributed Memory** | Secure memory synchronization across devices | Encrypted CRDTs for conflict resolution |
| **A2A Protocol Bridge** | Interoperability with Google A2A, MCP, and other frameworks | Protocol impedance matching |
| **Cloud Harness Hosting** | Private harness registries for teams | Git-backed package management with ACLs |

## Execution Dependencies

```mermaid
flowchart LR
    subgraph Phase1["Phase 1: Foundation"]
        A[Cloud Harness Hosting]
    end
    
    subgraph Phase2["Phase 2: Connectivity"]
        B[Distributed Memory]
        C[A2A Protocol Bridge]
    end
    
    subgraph Phase3["Phase 3: Experience"]
        D[Web Dashboard]
    end
    
    A --> B
    A --> C
    B --> D
    C --> D
```

**Recommended Order:**

1. **Cloud Harness Hosting** — Provides the distribution backbone that other pillars depend on
2. **Distributed Memory** & **A2A Bridge** — Can be built in parallel; both need the hosting foundation
3. **Web Dashboard** — Depends on all other pillars for full functionality

## Cross-Cutting Concerns

### Security
- All external communication uses mTLS or WireGuard
- Memory encryption at rest (AES-256-GCM) and in transit
- Harness signatures for provenance verification

### Observability
- OpenTelemetry tracing across all pillars
- Structured logging compatible with craft-memory event stream
- Prometheus metrics for self-hosted registries

### Configuration
- Unified `craft.toml` extension for M3 features
- Environment-based activation (no compile-time flags)
- Sensible defaults for single-user mode

## Document Index

1. **[01-overview.md](./01-overview.md)** — This document
2. **[02-web-dashboard.md](./02-web-dashboard.md)** — Visual composition architecture
3. **[03-distributed-memory.md](./03-distributed-memory.md)** — Encrypted sync design
4. **[04-a2a-bridge.md](./04-a2a-bridge.md)** — Protocol interoperability
5. **[05-cloud-hosting.md](./05-cloud-hosting.md)** — Private registry architecture
6. **[06-execution-roadmap.md](./06-execution-roadmap.md)** — Implementation sequencing
