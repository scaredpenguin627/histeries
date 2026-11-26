# Phase 5 — Playable geodesic matches

P5-03 connects the sphere to seeded game creation, seat observations, desktop and browser controls, scenario editing, saves, replay and network replication. The bookmark is `task/phase5-globe-match`. Codex authored the changes and the maintainer selected independent Sol 6.2 subagents at maximum reasoning effort. [Match rules](../../specs/globe-matches.md) · [Map encoding](../adr/0025-geodesic-matches.md)

On Windows GNU, a frequency-3 game started on a real TCP loopback host with seed 7. Both seats planned from immutable observations of the same round boundary. Running with worker limits of 1 and 8 produced identical traces. Each client verified its replica independently. Seat 2 won a science victory after 42 completed rounds and 494 accepted commands. The final hash was `8886fd3ca80ba118`. Each round agreed with a freshly loaded save and with a full command-log replay. A client also disconnected and rejoined after the fifth round. The hosted fast workflow records the full round trace from each native platform and compares the three artifacts.

The CLI also generated an eight-player frontier on the largest supported sphere (frequency 32, seed 7, 10,242 cells). Its initial save occupied 83,034 bytes. Windows Chrome 158 on Intel Gen9 WebGPU opened a frequency-3 globe, selected visible desert tile 24 from pentagonal city tile 0, and rotated the planet. It opened the planar atlas and restored a 1,103-byte globe save at round 1.

The browser save matched the independently generated native reference byte for byte (SHA-256 `3974f3c6ce99b5ac1784889a198f8f1e3a96433472e0b9b2b6815d509c459a26`). Native solo and LAN clients rendered the globe at 1,500 × 1,000. After seat 1 ended its turn, seat 2 saw its own city on tile 63. The default LAN launch and explicit `--3d` launch produced identical captures.

Independent review prompted the explicit worker-count comparison. It also led to a test of a frequency-1 frontier where greedy placement used a garrison needed by another player. A matching fallback finds the three disjoint capital/garrison pairs for seed 10 and still reports an error for an impossible roster.
