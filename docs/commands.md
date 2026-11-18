# Command registry

Every state change uses one registry entry with a stable ID, schema, validator, and test.

| ID | Schema | Validation and test coverage |
| --- | --- | --- |
| `editor.set_marker` | `player:u32,value:i64` | Requires an existing player. Binary decoder and session dispatch tests cover valid and unknown seats. |
| `turn.end` | `player:u32` | Requires an existing player and advances the checked turn counter. Binary decoder and event tests cover the command. |

The generated registry is exposed by `histeries-sim::command_registry`.
