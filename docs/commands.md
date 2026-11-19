# Command registry

Every state change uses one registry entry with a stable ID, schema, validator, and test.

| ID | Schema | Validation and test coverage |
| --- | --- | --- |
| `editor.set_feature` | `player:u32,tile:u32,feature:u8` | Requires an existing player and compatible tile terrain. Map editor tests cover validation. |
| `editor.set_marker` | `player:u32,value:i64` | Requires an existing player. Binary decoder and session dispatch tests cover valid and unknown seats. |
| `editor.set_resource` | `player:u32,tile:u32,resource:u8?` | Requires an existing player and non-water, non-mountain terrain. Map editor tests cover validation. |
| `editor.set_river` | `player:u32,tile:u32,river:u8?` | Requires an existing player and compatible terrain. Map editor tests cover validation. |
| `editor.set_terrain` | `player:u32,tile:u32,terrain:u8` | Requires an existing player and a valid tile. Map editor tests cover validation. |
| `city.build_building` | `player:u32,city:u32,building:u8` | Requires an owned city, an unbuilt building, and its district prerequisite. Production and ownership tests cover the command. |
| `city.build_district` | `player:u32,city:u32,kind:u8,tile:u32` | Requires an owned city, a unique traversable tile within graph distance 3, and a valid topology placement. District tests cover placement and adjacency. |
| `city.enqueue_production` | `player:u32,city:u32,item:production` | Appends to an owned city's bounded queue, preserving active progress and reserving district sites. Earlier orders may fulfill a later building's district prerequisite. |
| `city.found` | `player:u32,city:u32,unit:u32` | Consumes an owned settler on a valid unoccupied tile. Founding and seat-scoped observation tests cover the command. |
| `city.set_production` | `player:u32,city:u32,item:production` | Replaces one owned city's queue with a validated unit, building, or district order. Binary round-trip and completion tests cover the command. |
| `city.settle` | `player:u32,unit:u32` | Uses the same founding checks and allocates the lowest available city ID at dispatch. Two-seat founding and binary replay tests cover allocation. |
| `policy.adopt` | `player:u32,slot:u8,policy:string` | Requires an unlocked slot, a completed civic prerequisite, and a non-duplicate policy. Policy and victory tests cover the command. |
| `policy.remove` | `player:u32,slot:u8` | Clears an owned seat's policy slot. Slot validation tests cover the command. |
| `research.civic` | `player:u32,civic:string` | Selects a validated civic whose prerequisites are complete. Progress and binary round-trip tests cover the command. |
| `research.technology` | `player:u32,technology:string` | Selects a validated technology whose prerequisites are complete. Progress and binary round-trip tests cover the command. |
| `turn.end` | `player:u32` | Requires an existing player who has not ended this turn. Emits `TurnEnded` until every seat ends, then advances the checked counter and resets all units. Binary decoder, repeated-end, and multi-seat event tests cover the command. |
| `unit.attack` | `player:u32,attacker:u32,defender:u32` | Requires owned military units within range and resolves deterministic damage. Combat tests cover damage and events. |
| `unit.capture_city` | `player:u32,unit:u32,city:u32` | An owned melee unit with movement captures an adjacent enemy city after the city owner's perimeter defenders have been cleared. Conquest tests cover rejection, victory, save/load and replay. |
| `unit.build_road` | `player:u32,unit:u32` | Requires an owned builder with a remaining charge on traversable terrain. Road and charge-exhaustion tests cover the command. |
| `unit.heal` | `player:u32,unit:u32` | Requires an owned unit outside enemy ZOC. Healing tests cover bounds and safety. |
| `unit.hold` | `player:u32,unit:u32` | Holds an owned unit in place and spends any remaining movement. An exhausted unit can also hold. |
| `unit.move` | `player:u32,unit:u32,destination:u32` | Requires an owned unit, adjacent traversable destination, movement points, and ZOC clearance. |
| `unit.promote` | `player:u32,unit:u32,promotion:u8` | Requires owned military unit experience. Promotion tests cover one-time effects. |
| `unit.spawn` | `player:u32,unit:u32,kind:u8,tile:u32` | Requires an existing player and unoccupied traversable tile. |
| `barbarian.spawn` | `tile:u32,kind:u8` | Spawns a hostile unit on an unoccupied traversable tile. |

The generated registry is exposed by `histeries-sim::command_registry`.

The MCP `list_commands` result includes JSON argument schemas for the connected role. A seat sends the command ID and its arguments to `dispatch`; the server supplies the player ID. The trusted local scenario editor accepts the same named commands with explicit seat IDs in a JSON file. For example, this batch gives seat 1 an archer and a builder on the same empty traversable tile:

```json
[
  {"command":"unit.spawn","player":1,"unit":100,"kind":"archer","tile":12},
  {"command":"unit.spawn","player":1,"unit":101,"kind":"builder","tile":12}
]
```

Run `histeries scenario-editor state.hst apply --commands orders.json --output edited.hst` to apply the batch. All orders must succeed before the output world is written. District and building production arguments use tagged items such as `{"district":{"kind":"campus","tile":9}}` and `{"building":"library"}`.

## CLI entry points

`histeries new` creates a seeded state snapshot, `step` ends one seat turn, `run` ends complete turns, `replay` validates and replays a command log, and `map` prints deterministic terrain glyphs. `editor` and `scenario-editor` route the five `editor.*` commands through `Session::dispatch`.



