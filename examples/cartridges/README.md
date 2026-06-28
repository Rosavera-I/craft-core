# Starter Cartridges

The CRAFT starter cartridges now live in their own standalone repositories for independent versioning and installation:

| Cartridge | Repository | Description |
|-----------|-----------|-------------|
| godot-designer | [Rosavera-I/craft-godot-designer](https://github.com/Rosavera-I/craft-godot-designer) | Godot 4 gameplay and scene review |
| tdd-architect | [Rosavera-I/craft-tdd-architect](https://github.com/Rosavera-I/craft-tdd-architect) | Feature intent → executable behavior contracts |
| rust-maintainer | [Rosavera-I/craft-rust-maintainer](https://github.com/Rosavera-I/craft-rust-maintainer) | Rust review, maintenance, and release hygiene |

Install via CRAFT CLI:
```bash
craft harness install github:Rosavera-I/craft-godot-designer
craft harness install github:Rosavera-I/craft-rust-maintainer  
craft harness install github:Rosavera-I/craft-tdd-architect
```

These will move to `JMoak/craft-*` after repository transfer.
