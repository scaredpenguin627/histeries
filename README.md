# Histeries

Histeries is an open-source, clean-room, turn-based 4X strategy game built in Rust.

The Phase 0 scaffold establishes the crate boundaries, deterministic-simulation decisions, data-driven ruleset direction, and CI entry points. Gameplay implementation begins in Phase 1. Read `AGENTS.md` for the project contract and `docs/ROADMAP.md` for the current task list.

## Local checks

```text
cargo xtask layers
cargo xtask ci-fast
```

The repository uses jj for version control. The GitHub Actions fast gate runs the pinned Rust toolchain, formatting, clippy, license checks, tests, and platform checks.

## CLI

The headless CLI uses the simulation command registry and versioned binary files.

```text
histeries new --players 2 --width 8 --height 8 --seed 7 --output state.hst
histeries step state.hst --player 1
histeries run state.hst --turns 10
histeries map state.hst
histeries replay session.hrp
histeries editor state.hst set-terrain --player 1 --tile 4 --value plains
```

Use `--replay-output PATH` with `new`, `step`, `run`, or `editor` to write a replay log.
