# Histeries

Histeries is an open-source, clean-room, turn-based 4X strategy game built in Rust.

The Phase 0 scaffold establishes the crate boundaries, deterministic-simulation decisions, data-driven ruleset direction, and CI entry points. Gameplay implementation begins in Phase 1. Read `AGENTS.md` for the project contract and `docs/ROADMAP.md` for the current task list.

## Local checks

```text
cargo xtask layers
cargo xtask ci-fast
```

The repository uses jj for version control. The GitHub Actions fast gate runs the pinned Rust toolchain, formatting, clippy, license checks, tests, and platform checks.
