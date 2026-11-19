//! Compare independently specified outcomes with the simulation's public seat observations.
use histeries_hex::FlatHex;
use histeries_rules_core::PlayerId;
use histeries_sim::{Command, MapState, Session};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    version: u8,
    provenance: String,
    source: String,
    players: u32,
    width: u32,
    height: u32,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    #[serde(default)]
    commands: Vec<Command>,
    #[serde(default)]
    rounds: u32,
    checks: Vec<Check>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    seat: u32,
    pointer: String,
    expected: Value,
    tolerance: Option<u64>,
}

impl Check {
    fn compare(&self, view: &Value) -> Result<Value, String> {
        if !self.pointer.starts_with('/') {
            return Err("an observation check requires a JSON Pointer starting with /".into());
        }
        let actual = view.pointer(&self.pointer);
        let passed = match self.tolerance {
            None => actual == Some(&self.expected),
            Some(limit) => {
                let target = self
                    .expected
                    .as_i64()
                    .ok_or("numeric tolerance requires an integer expectation")?;
                actual
                    .and_then(Value::as_i64)
                    .is_some_and(|n| target.abs_diff(n) <= limit)
            }
        };
        Ok(
            json!({"seat":self.seat,"pointer":self.pointer,"expected":self.expected,
            "actual":actual,"present":actual.is_some(),"tolerance":self.tolerance,"passed":passed}),
        )
    }
}

fn measure(fixture: Fixture, provenance: &str) -> Result<Vec<Value>, String> {
    if fixture.version != 1 || fixture.provenance != provenance || fixture.source.trim().is_empty()
    {
        return Err("invalid fixture version, provenance or source".into());
    }
    if !(2..=8).contains(&fixture.players) || fixture.steps.is_empty() {
        return Err("expected 2–8 players and at least one step".into());
    }
    let board =
        FlatHex::new(fixture.width, fixture.height, false, false).map_err(|e| e.to_string())?;
    let seats: Vec<_> = (1..=fixture.players).map(PlayerId::new).collect();
    let mut session = Session::with_map(seats.iter().copied(), MapState::blank(board));
    let mut measurements = Vec::new();
    for (index, step) in fixture.steps.into_iter().enumerate() {
        if step.checks.is_empty() {
            return Err(format!("step {} has no expectations", index + 1));
        }
        for command in step.commands {
            session
                .dispatch(command)
                .map_err(|e| format!("step {}: {e}", index + 1))?;
        }
        for _ in 0..step.rounds {
            for &player in &seats {
                if !session.has_ended_turn(player) {
                    session
                        .dispatch(Command::TurnEnd { player })
                        .map_err(|e| e.to_string())?;
                }
            }
            drop(session.drain_events());
            drop(session.take_replay_segment());
        }
        for check in step.checks {
            let observation = session
                .observe(PlayerId::new(check.seat))
                .map_err(|e| e.to_string())?;
            let mut row =
                check.compare(&serde_json::to_value(observation).map_err(|e| e.to_string())?)?;
            row["step"] = json!(index + 1);
            row["turn"] = json!(session.turn());
            measurements.push(row);
        }
    }
    Ok(measurements)
}

pub(super) fn run(root: &Path) -> Result<(), String> {
    let mut fixtures = Vec::new();
    let mut total = 0;
    let mut failed = 0;
    for (directory, provenance) in [("designed", "designed"), ("oracle", "observed")] {
        let entries =
            fs::read_dir(root.join("fixtures").join(directory)).map_err(|e| e.to_string())?;
        let mut paths = entries
            .map(|e| e.map(|d| d.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        paths.retain(|p| p.extension().is_some_and(|x| x == "json"));
        paths.sort();
        if directory == "designed" && paths.is_empty() {
            return Err("designed reference suite is empty".into());
        }
        for path in paths {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let fixture: Fixture =
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("{name}: {e}"))?;
            let source = fixture.source.clone();
            let checks = measure(fixture, provenance).map_err(|e| format!("{name}: {e}"))?;
            total += checks.len();
            failed += checks.iter().filter(|r| r["passed"] == false).count();
            fixtures
                .push(json!({"name":name,"provenance":provenance,"source":source,"checks":checks}));
        }
    }
    let directory = root.join("target/gates");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let report = json!({"version":1,"checks":total,"failed":failed,"fixtures":fixtures});
    fs::write(
        directory.join("parity.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "parity: {} of {total} checks passed (target/gates/parity.json)",
        total - failed
    );
    if failed > 0 {
        return Err(format!(
            "{failed} fixture checks differed from their references"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_distinguishes_missing_null_and_inclusive_integer_tolerance() {
        let view = json!({"n":10,"unselected":null});
        let check = |p: &str, expected: Value, tolerance| {
            Check {
                seat: 1,
                pointer: p.into(),
                expected,
                tolerance,
            }
            .compare(&view)
            .unwrap()["passed"]
                .clone()
        };
        assert_eq!(check("/unselected", Value::Null, None), true);
        assert_eq!(check("/missing", Value::Null, None), false);
        assert_eq!(check("/n", json!(12), Some(2)), true);
        assert_eq!(check("/n", json!(13), Some(2)), false);
        assert_eq!(check("/unselected", json!(0), Some(0)), false);
    }
}
