//! JSON tools use stable command IDs, registry field names and seat-filtered observations.

use histeries_rules_core::PlayerId;
use histeries_sim::{BuildingKind, Command, DistrictKind, Observation, command_registry};
use serde_json::{Map, Value, json};

fn privileged(id: &str) -> bool {
    id.starts_with("editor.") || matches!(id, "barbarian.spawn" | "unit.spawn" | "city.found")
}

fn alternatives(values: impl IntoIterator<Item = &'static str>) -> Value {
    json!({"type":"string", "enum":values.into_iter().collect::<Vec<_>>()})
}

fn uint(max: u64) -> Value {
    json!({"type":"integer","minimum":0,"maximum":max})
}

fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let fields: Map<String, Value> = fields
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect();
    let required: Vec<_> = fields.keys().cloned().collect();
    json!({"type":"object","properties":fields,"required":required,"additionalProperties":false})
}

fn unit_kinds() -> Value {
    alternatives(["warrior", "archer", "settler", "builder"])
}

fn districts() -> Value {
    alternatives(
        DistrictKind::ALL
            .into_iter()
            .filter(|d| *d != DistrictKind::CityCenter)
            .map(DistrictKind::ruleset_id),
    )
}

fn buildings() -> Value {
    alternatives(BuildingKind::ALL.into_iter().map(BuildingKind::ruleset_id))
}

fn production_schema() -> Value {
    json!({"oneOf":[object([("unit",unit_kinds())]),object([("building",buildings())]),
        object([("district",object([("kind",districts()),("tile",uint(u32::MAX.into()))]))])]})
}

fn field_schema(id: &str, field: &str, kind: &str) -> Value {
    match (field, kind) {
        (_, "u32") => uint(u32::MAX.into()),
        (_, "i64") => json!({"type":"integer","minimum":i64::MIN,"maximum":i64::MAX}),
        (_, "string") => json!({"type":"string"}),
        (_, "production") => production_schema(),
        ("kind", _) if id == "city.build_district" => districts(),
        ("kind", _) if id == "barbarian.spawn" => alternatives(["barbarian"]),
        ("kind", _) => unit_kinds(),
        ("building", _) => buildings(),
        ("promotion", _) => alternatives(["shock", "drill", "mobility"]),
        ("terrain", _) => alternatives([
            "plains",
            "grassland",
            "desert",
            "tundra",
            "coast",
            "ocean",
            "hill",
            "mountain",
        ]),
        ("feature", _) => alternatives(["forest", "jungle", "marsh", "none"]),
        ("river", _) => {
            json!({"oneOf":[{"type":"null"},alternatives(["north_east","east","south_east","south_west","west","north_west"])]})
        }
        ("resource", _) => {
            json!({"oneOf":[{"type":"null"},alternatives(["wheat","stone","iron","gold"])]})
        }
        (_, "u8") => uint(u8::MAX.into()),
        _ => unreachable!("unmapped command registry field {id}.{field} ({kind})"),
    }
}

/// Parameters describe the object accepted by dispatch; the connection supplies its seat.
pub(crate) fn commands(editor: bool) -> Value {
    Value::Array(
        command_registry()
            .specs()
            .iter()
            .filter(|spec| editor || !privileged(spec.id))
            .map(|spec| {
                let parameters = object(
                    spec.schema
                        .split(',')
                        .map(|field| field.split_once(':').expect("typed command field"))
                        .filter(|(field, _)| *field != "player")
                        .map(|(field, ty)| (field, field_schema(spec.id, field, ty))),
                );
                json!({"id":spec.id,"inputSchema":parameters})
            })
            .collect(),
    )
}

pub(crate) fn decode(
    id: &str,
    args: Value,
    seat: PlayerId,
    editor: bool,
) -> Result<Command, String> {
    let spec = command_registry()
        .find(id)
        .ok_or_else(|| "unknown command".to_string())?;
    if privileged(id) && !editor {
        return Err("command requires editor authority".into());
    }
    let Value::Object(mut fields) = args else {
        return Err("arguments must be an object".into());
    };
    if fields.contains_key("player") || fields.contains_key("command") {
        return Err("reserved command or player field".into());
    }
    for field in spec
        .schema
        .split(',')
        .filter_map(|s| s.split_once(':').map(|(k, _)| k))
    {
        if field == "player" {
            fields.insert("player".into(), json!(seat.index()));
        } else if !fields.contains_key(field) {
            return Err(format!("missing argument {field}"));
        }
    }
    fields.insert("command".into(), json!(id));
    serde_json::from_value(fields.into()).map_err(|e| e.to_string())
}

pub(crate) fn observation(view: &Observation) -> Value {
    serde_json::to_value(view).expect("seat observation contains canonical JSON values")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_discovery_covers_every_command_and_only_authorized_actions() {
        let all = commands(true);
        assert_eq!(
            all.as_array().unwrap().len(),
            command_registry().specs().len()
        );
        for entry in commands(false).as_array().unwrap() {
            assert!(!privileged(entry["id"].as_str().unwrap()));
            assert!(entry["inputSchema"]["properties"].get("player").is_none());
        }
        let seat = PlayerId::new(7);
        let command = decode(
            "city.set_production",
            json!({"city":4,"item":{"district":{"kind":"campus","tile":9}}}),
            seat,
            false,
        )
        .unwrap();
        assert_eq!(command.id(), "city.set_production");
        assert_eq!(serde_json::to_value(command).unwrap()["player"], 7);
        for (id, args) in [
            ("turn.end", json!({"player":5})),
            ("unit.move", json!({"unit":2,"destination":1,"extra":0})),
            ("editor.set_marker", json!({"value":9})),
        ] {
            assert!(decode(id, args, seat, false).is_err());
        }
    }
}
