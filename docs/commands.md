# Command registry

Every state change uses one registry entry with a stable ID, schema, validator, and test.

| ID | Schema | Validation and test coverage |
| --- | --- | --- |
| `editor.set_feature` | `player:u32,tile:u32,feature:u8` | Requires an existing player and compatible tile terrain. Map editor tests cover validation. |
| `editor.set_marker` | `player:u32,value:i64` | Requires an existing player. Binary decoder and session dispatch tests cover valid and unknown seats. |
| `editor.set_resource` | `player:u32,tile:u32,resource:u8?` | Requires an existing player and non-water, non-mountain terrain. Map editor tests cover validation. |
| `editor.set_river` | `player:u32,tile:u32,river:u8?` | Requires an existing player and compatible terrain. Map editor tests cover validation. |
| `editor.set_terrain` | `player:u32,tile:u32,terrain:u8` | Requires an existing player and a valid tile. Map editor tests cover validation. |
| `turn.end` | `player:u32` | Requires an existing player and advances the checked turn counter. Binary decoder and event tests cover the command. |

The generated registry is exposed by `histeries-sim::command_registry`.


