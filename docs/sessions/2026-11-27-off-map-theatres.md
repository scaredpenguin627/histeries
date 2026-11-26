# Orbital Survey

Bookmark `task/phase5-off-map-theatres` adds a second board to a running campaign. After unlocking Orbital access, a seat can launch at the entry, explore one edge per round and return to recall. The desktop and browser clients share the chart and order controls. CLI, LAN and MCP use the same typed orders. A seat keeps its chart when it returns to the surface or reopens the save. [Specification](../../specs/theatres.md) · [ADR-0027](../adr/0027-off-map-theatres.md)

The `orbital_opening` example creates a playable opening by issuing research and turn orders on the seed-7 globe. The browser acceptance test opens this save, launches an expedition, advances a round, visits a second site, returns to the surface and saves. After reloading it checks the expedition's position and survey history. The native reference repeats the orders independently. The LAN acceptance test exercises expeditions through two replicas and reconstructs the whole campaign from its command log. [Scorecard](../scorecard.md)

The parallel HarmonyOS dependency check and the remaining device acceptance conditions are recorded in the [graphics preflight](2026-11-27-ohos-graphics.md).

On Windows GNU, the fast gate passed 255 tests, 34 fixture checks and 1,200 round hashes in 463.266 seconds. Chrome on Intel Gen9 reopened the expedition at site 1 on round 293 with two visited sites. The 1,361-byte browser save matched the independently played native save byte for byte (SHA-256 `6f036c0959d7d2d4f0a084de6a31365186d76a55d5b302a796c61e2628950eba`). The LAN trace checks both player-facing observations on each trusted replica. The canonical replica itself contains all seats' state under ADR-0022.

The submission bookmark is `task/phase5-off-map-theatres`. Hosted three-platform traces, Linux WebGPU save parity and the nightly and weekly runs are pending for this submission.
