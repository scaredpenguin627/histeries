//! Headless command-line entry points for Histeries.

#![forbid(unsafe_code)]

#[cfg(all(
    feature = "heap-profile",
    any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
mod memgate;
mod selfplay;
mod tournament;

use std::{env, fs};

use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Command, Feature, MapState, ResourceKind, RiverDirection, Session, SimulationState, Terrain,
    generate_map,
};

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("histeries: {error}");
        std::process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage());
    };
    match command {
        "new" => command_new(&args[1..]),
        "step" => command_step(&args[1..]),
        "run" => command_run(&args[1..]),
        "replay" => command_replay(&args[1..]),
        "tournament" => tournament::run(&args[1..]),
        "selfplay" => selfplay::run(&args[1..]),
        #[cfg(all(
            feature = "heap-profile",
            any(
                target_os = "windows",
                target_os = "macos",
                all(target_os = "linux", not(target_env = "ohos"))
            )
        ))]
        "memgate" => memgate::run(&args[1..]),
        "map" => command_map(&args[1..]),
        "editor" | "scenario-editor" => command_editor(&args[1..]),
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(format!("unknown command {command}\n{}", usage())),
    }
}

fn command_new(args: &[String]) -> Result<(), String> {
    validate_flags(
        args,
        &[
            "--players",
            "--width",
            "--height",
            "--seed",
            "--output",
            "--replay-output",
        ],
        0,
    )?;
    let players = flag_u32(args, "--players")?.unwrap_or(2);
    let width = flag_u32(args, "--width")?.unwrap_or(8);
    let height = flag_u32(args, "--height")?.unwrap_or(8);
    let seed = flag_u64(args, "--seed")?.unwrap_or(0);
    let output = flag_string(args, "--output")?.unwrap_or_else(|| "state.hst".to_owned());
    let replay_output = flag_string(args, "--replay-output")?;
    if players == 0 || players > 8 {
        return Err("--players must be between 1 and 8".to_owned());
    }
    let topology = FlatHex::new(width, height, false, false).map_err(|error| error.to_string())?;
    let map = generate_map(topology, seed_bytes(seed));
    let seats = (1..=players).map(PlayerId::new);
    let session = Session::with_map(seats, map);
    write_state(&output, &session.state_snapshot())?;
    write_replay(replay_output.as_deref(), &session)?;
    println!(
        "turn=0 hash={} output={output}",
        session.state_hash().value()
    );
    Ok(())
}

fn command_step(args: &[String]) -> Result<(), String> {
    validate_flags(args, &["--player", "--output", "--replay-output"], 1)?;
    let input = positional(args, 0, "state path")?;
    let output = flag_string(args, "--output")?.unwrap_or_else(|| input.clone());
    let replay_output = flag_string(args, "--replay-output")?;
    let state = read_state(&input)?;
    let player = flag_u32(args, "--player")?
        .unwrap_or_else(|| state.players().next().map(PlayerId::index).unwrap_or(1));
    let mut session = Session::from_state(state);
    session
        .dispatch(Command::TurnEnd {
            player: PlayerId::new(player),
        })
        .map_err(|error| error.to_string())?;
    let snapshot = session.state_snapshot();
    write_state(&output, &snapshot)?;
    write_replay(replay_output.as_deref(), &session)?;
    println!(
        "turn={} hash={} output={output}",
        snapshot.turn(),
        session.state_hash().value()
    );
    Ok(())
}

fn command_run(args: &[String]) -> Result<(), String> {
    validate_flags(args, &["--turns", "--output", "--replay-output"], 1)?;
    let input = positional(args, 0, "state path")?;
    let output = flag_string(args, "--output")?.unwrap_or_else(|| input.clone());
    let replay_output = flag_string(args, "--replay-output")?;
    let turns = flag_u32(args, "--turns")?.unwrap_or(1);
    let mut session = Session::from_state(read_state(&input)?);
    let players: Vec<_> = session.state_snapshot().players().collect();
    for _ in 0..turns {
        for player in &players {
            if session
                .state_snapshot()
                .ended_players()
                .any(|ended| ended == *player)
            {
                continue;
            }
            session
                .dispatch(Command::TurnEnd { player: *player })
                .map_err(|error| error.to_string())?;
        }
        release_round_history(&mut session, replay_output.is_some());
    }
    let snapshot = session.state_snapshot();
    write_state(&output, &snapshot)?;
    write_replay(replay_output.as_deref(), &session)?;
    println!(
        "turn={} hash={} output={output}",
        snapshot.turn(),
        session.state_hash().value()
    );
    Ok(())
}

const REPLAY_SEGMENT_TURNS: u32 = 25;

fn release_round_history(session: &mut Session, keep_replay: bool) {
    drop(session.drain_events());
    if !keep_replay && session.turn().is_multiple_of(REPLAY_SEGMENT_TURNS) {
        drop(session.take_replay_segment());
    }
}

fn command_replay(args: &[String]) -> Result<(), String> {
    validate_flags(args, &[], 1)?;
    let input = positional(args, 0, "replay path")?;
    let bytes = fs::read(&input).map_err(|error| format!("read {input}: {error}"))?;
    let replay = histeries_sim::ReplayLog::decode(&bytes).map_err(|error| error.to_string())?;
    let state = replay.replay().map_err(|error| error.to_string())?;
    println!(
        "turn={} hash={} commands={}",
        state.turn(),
        state.state_hash().value(),
        replay.commands().len()
    );
    Ok(())
}

fn command_map(args: &[String]) -> Result<(), String> {
    validate_flags(args, &[], 1)?;
    let input = positional(args, 0, "state path")?;
    let state = read_state(&input)?;
    let map = state.map().ok_or_else(|| "state has no map".to_owned())?;
    print!("{}", map_ascii(map));
    Ok(())
}

fn command_editor(args: &[String]) -> Result<(), String> {
    validate_flags(
        args,
        &[
            "--output",
            "--replay-output",
            "--player",
            "--tile",
            "--value",
        ],
        2,
    )?;
    let input = positional(args, 0, "state path")?;
    let action = positional(args, 1, "editor action")?;
    let output = flag_string(args, "--output")?.unwrap_or_else(|| input.clone());
    let replay_output = flag_string(args, "--replay-output")?;
    let player = PlayerId::new(flag_u32(args, "--player")?.ok_or("--player is required")?);
    let command = match action.as_str() {
        "set-marker" => Command::EditorSetMarker {
            player,
            value: flag_string(args, "--value")?
                .ok_or("--value is required")?
                .parse()
                .map_err(|_| "--value must be an integer".to_owned())?,
        },
        "set-terrain" => Command::EditorSetTerrain {
            player,
            tile: required_tile(args)?,
            terrain: parse_terrain(&flag_string(args, "--value")?.ok_or("--value is required")?)?,
        },
        "set-feature" => Command::EditorSetFeature {
            player,
            tile: required_tile(args)?,
            feature: parse_feature(&flag_string(args, "--value")?.ok_or("--value is required")?)?,
        },
        "set-river" => Command::EditorSetRiver {
            player,
            tile: required_tile(args)?,
            river: parse_optional_river(&flag_string(args, "--value")?.ok_or("--value is required")?)?,
        },
        "set-resource" => Command::EditorSetResource {
            player,
            tile: required_tile(args)?,
            resource: parse_optional_resource(&flag_string(args, "--value")?.ok_or("--value is required")?)?,
        },
        _ => return Err("editor action must be set-marker, set-terrain, set-feature, set-river, or set-resource".to_owned()),
    };
    let mut session = Session::from_state(read_state(&input)?);
    session
        .dispatch(command)
        .map_err(|error| error.to_string())?;
    let snapshot = session.state_snapshot();
    write_state(&output, &snapshot)?;
    write_replay(replay_output.as_deref(), &session)?;
    println!(
        "turn={} hash={} output={output}",
        snapshot.turn(),
        session.state_hash().value()
    );
    Ok(())
}

fn read_state(path: &str) -> Result<SimulationState, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
    SimulationState::decode(&bytes).map_err(|error| format!("decode {path}: {error}"))
}

fn write_state(path: &str, state: &SimulationState) -> Result<(), String> {
    fs::write(path, state.encode()).map_err(|error| format!("write {path}: {error}"))
}

fn write_replay(path: Option<&str>, session: &Session) -> Result<(), String> {
    if let Some(path) = path {
        fs::write(path, session.replay().encode())
            .map_err(|error| format!("write {path}: {error}"))?;
    }
    Ok(())
}

fn map_ascii(map: &MapState) -> String {
    let width = map.topology().width() as usize;
    let mut output = String::new();
    for (index, (_, tile)) in map.tiles().enumerate() {
        if index > 0 && index % width == 0 {
            output.push('\n');
        }
        output.push(terrain_glyph(tile.terrain));
    }
    output.push('\n');
    output
}

fn terrain_glyph(terrain: Terrain) -> char {
    match terrain {
        Terrain::Plains => '.',
        Terrain::Grassland => 'g',
        Terrain::Desert => 'd',
        Terrain::Tundra => 't',
        Terrain::Coast => ',',
        Terrain::Ocean => '~',
        Terrain::Hill => 'h',
        Terrain::Mountain => '^',
    }
}

fn parse_terrain(value: &str) -> Result<Terrain, String> {
    match value {
        "plains" => Ok(Terrain::Plains),
        "grassland" => Ok(Terrain::Grassland),
        "desert" => Ok(Terrain::Desert),
        "tundra" => Ok(Terrain::Tundra),
        "coast" => Ok(Terrain::Coast),
        "ocean" => Ok(Terrain::Ocean),
        "hill" => Ok(Terrain::Hill),
        "mountain" => Ok(Terrain::Mountain),
        _ => Err(format!("unknown terrain {value}")),
    }
}

fn parse_feature(value: &str) -> Result<Feature, String> {
    match value {
        "none" => Ok(Feature::None),
        "forest" => Ok(Feature::Forest),
        "jungle" => Ok(Feature::Jungle),
        "marsh" => Ok(Feature::Marsh),
        _ => Err(format!("unknown feature {value}")),
    }
}

fn parse_optional_river(value: &str) -> Result<Option<RiverDirection>, String> {
    if value == "none" {
        return Ok(None);
    }
    let river = match value {
        "northeast" => RiverDirection::NorthEast,
        "east" => RiverDirection::East,
        "southeast" => RiverDirection::SouthEast,
        "southwest" => RiverDirection::SouthWest,
        "west" => RiverDirection::West,
        "northwest" => RiverDirection::NorthWest,
        _ => return Err(format!("unknown river {value}")),
    };
    Ok(Some(river))
}

fn parse_optional_resource(value: &str) -> Result<Option<ResourceKind>, String> {
    if value == "none" {
        return Ok(None);
    }
    let resource = match value {
        "wheat" => ResourceKind::Wheat,
        "stone" => ResourceKind::Stone,
        "iron" => ResourceKind::Iron,
        "gold" => ResourceKind::Gold,
        _ => return Err(format!("unknown resource {value}")),
    };
    Ok(Some(resource))
}

fn positional(args: &[String], index: usize, name: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing {name}"))
}

fn validate_flags(
    args: &[String],
    allowed: &[&str],
    positional_limit: usize,
) -> Result<(), String> {
    let mut positional = 0;
    let mut index = 0;
    while index < args.len() {
        if args[index].starts_with("--") {
            if !allowed.contains(&args[index].as_str()) {
                return Err(format!("unknown flag {}", args[index]));
            }
            if index + 1 >= args.len() || args[index + 1].starts_with("--") {
                return Err(format!("{} requires a value", args[index]));
            }
            index += 2;
        } else {
            positional += 1;
            if positional > positional_limit {
                return Err(format!("unexpected positional argument {}", args[index]));
            }
            index += 1;
        }
    }
    Ok(())
}

fn required_tile(args: &[String]) -> Result<TileId, String> {
    Ok(TileId::new(
        flag_u32(args, "--tile")?.ok_or("--tile is required")?,
    ))
}

fn flag_string(args: &[String], name: &str) -> Result<Option<String>, String> {
    let Some(index) = args.iter().position(|value| value == name) else {
        return Ok(None);
    };
    args.get(index + 1)
        .cloned()
        .map(Some)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn flag_u32(args: &[String], name: &str) -> Result<Option<u32>, String> {
    flag_string(args, name)?.map_or(Ok(None), |value| {
        value
            .parse()
            .map(Some)
            .map_err(|_| format!("{name} must be an unsigned integer"))
    })
}

fn flag_u64(args: &[String], name: &str) -> Result<Option<u64>, String> {
    flag_string(args, name)?.map_or(Ok(None), |value| {
        value
            .parse()
            .map(Some)
            .map_err(|_| format!("{name} must be an unsigned integer"))
    })
}

fn seed_bytes(seed: u64) -> [u8; 32] {
    let lane = seed.to_le_bytes();
    let mut bytes = [0_u8; 32];
    for index in 0..4 {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&lane);
    }
    bytes
}

fn usage() -> String {
    "usage: histeries <new|step|run|replay|tournament|selfplay|map|editor> ...\n  new [--players N] [--width N] [--height N] [--seed N] [--output PATH] [--replay-output PATH]\n  step STATE [--player N] [--output PATH] [--replay-output PATH]\n  run STATE [--turns N] [--output PATH] [--replay-output PATH]\n  replay REPLAY\n  tournament [--opponent greedy|random] [--seed N] [--seeds COUNT] [--turns LIMIT] [--workers N]\n  selfplay [--seed N] [--players N] [--workers N] [--turns LIMIT] [--games COUNT] [--size small|large|huge] [--mode game|stress] [--checkpoint-dir PATH [--stop-after-round N]]\n  map STATE\n  editor STATE ACTION --player N --tile N --value VALUE [--replay-output PATH]".to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn replay_output_preserves_archives_across_housekeeping_boundaries() {
        let seat = super::PlayerId::new(1);
        let mut archive = super::Session::new([seat]);
        let mut live = archive.clone();
        for _ in 0..=super::REPLAY_SEGMENT_TURNS {
            for (session, keep_replay) in [(&mut archive, true), (&mut live, false)] {
                session
                    .dispatch(super::Command::TurnEnd { player: seat })
                    .unwrap();
                super::release_round_history(session, keep_replay);
                assert!(session.events().is_empty());
            }
        }
        assert_eq!(archive.state_hash(), live.state_hash());
        assert_eq!(live.replay().commands().len(), 1);
        assert_eq!(
            archive.replay().commands().len(),
            super::REPLAY_SEGMENT_TURNS as usize + 1
        );
        assert_eq!(
            archive.replay().replay().unwrap().state_hash(),
            live.state_hash()
        );
    }

    use std::{env, fs, path::PathBuf};

    use super::{map_ascii, parse_feature, parse_terrain, run, seed_bytes, terrain_glyph};
    use histeries_hex::FlatHex;
    use histeries_sim::{Feature, MapState, Terrain};

    fn temp_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("histeries-cli-{name}-{}.bin", std::process::id()))
    }

    #[test]
    fn parser_and_seed_outputs_are_stable() {
        assert_eq!(parse_terrain("mountain"), Ok(Terrain::Mountain));
        assert_eq!(parse_feature("forest"), Ok(Feature::Forest));
        assert_eq!(terrain_glyph(Terrain::Ocean), '~');
        assert_eq!(seed_bytes(7)[..8], 7_u64.to_le_bytes());
        assert_eq!(seed_bytes(7)[8..16], 7_u64.to_le_bytes());
    }

    #[test]
    fn cli_commands_persist_state_replay_map_and_editor_changes() {
        let state = temp_path("state");
        let replay = temp_path("replay");
        let state_text = state.to_string_lossy().into_owned();
        let replay_text = replay.to_string_lossy().into_owned();
        run(vec![
            "new".to_owned(),
            "--players".to_owned(),
            "2".to_owned(),
            "--width".to_owned(),
            "3".to_owned(),
            "--height".to_owned(),
            "3".to_owned(),
            "--seed".to_owned(),
            "4".to_owned(),
            "--output".to_owned(),
            state_text.clone(),
            "--replay-output".to_owned(),
            replay_text.clone(),
        ])
        .unwrap();
        run(vec![
            "step".to_owned(),
            state_text.clone(),
            "--player".to_owned(),
            "1".to_owned(),
        ])
        .unwrap();
        run(vec![
            "run".to_owned(),
            state_text.clone(),
            "--turns".to_owned(),
            "1".to_owned(),
            "--replay-output".to_owned(),
            replay_text.clone(),
        ])
        .unwrap();
        run(vec!["replay".to_owned(), replay_text.clone()]).unwrap();
        run(vec!["map".to_owned(), state_text.clone()]).unwrap();
        for (action, value, tile) in [
            ("set-marker", "42", None),
            ("set-terrain", "plains", Some("0")),
            ("set-feature", "forest", Some("0")),
            ("set-river", "east", Some("0")),
            ("set-resource", "wheat", Some("0")),
        ] {
            let mut args = vec![
                "editor".to_owned(),
                state_text.clone(),
                action.to_owned(),
                "--player".to_owned(),
                "1".to_owned(),
                "--value".to_owned(),
                value.to_owned(),
            ];
            if let Some(tile) = tile {
                args.extend(["--tile".to_owned(), tile.to_owned()]);
            }
            run(args).unwrap();
        }
        assert!(
            map_ascii(&MapState::blank(FlatHex::new(2, 2, false, false).unwrap())).contains("..")
        );
        assert!(run(vec!["new".to_owned(), "--widht".to_owned(), "2".to_owned()]).is_err());
        assert!(
            run(vec![
                "map".to_owned(),
                state_text.clone(),
                "extra".to_owned()
            ])
            .is_err()
        );
        let _ = fs::remove_file(state);
        let _ = fs::remove_file(replay);
    }
}
