# Late-game ages

Bookmark `task/phase5-late-ages` implements the per-seat age and policy rules in ADR-0026. The base campaign runs from Founding through Industrial to Orbital. A player can see their current age and available policies in the Realm panel or through MCP. The sim derives the same age from a restored seat's completed research.

The command tests exercise independent seats, invalid early policy adoption, science and culture carry-over, save/load and replay. The MCP acceptance game discovers the rules, researches through both transitions and adopts the orbital policy. [Spec](../../specs/ages.md) · [Scorecard](../scorecard.md)

The local suite passed 251 tests and 34 fixture checks. Extending the ruleset initially made the historical greedy guest win one round early. Restricting the reference controllers to their original policy pool restored both recorded matches. All 219 archived MCP exchanges retain their original observations and outcomes, including science victories in rounds 60 and 57. The current layered planner can choose the new policy after reaching its required age.

The seed-7 globe replay checks that a layered player actually adopts the Orbital research policy. Separate paired tournaments ran V2 against greedy and V1, using both seat assignments on 32 seeds for each opponent. The scorecard records the outcomes and significance tests.

Sol 6.2 (max) approved `a5fdf0ce3cd2` after the final 40.412-second local fast gate. The bookmark was pushed as `task/phase5-late-ages`. GitHub passed five fast jobs, five nightly shards and twelve weekly jobs. PR #16 merged at `1a1d1d3c06d9`. [PR #16](https://github.com/scaredpenguin627/histeries/pull/16) · [Fast](https://github.com/scaredpenguin627/histeries/actions/runs/45893502412) · [Nightly](https://github.com/scaredpenguin627/histeries/actions/runs/45893512780) · [Weekly](https://github.com/scaredpenguin627/histeries/actions/runs/45893516319)
