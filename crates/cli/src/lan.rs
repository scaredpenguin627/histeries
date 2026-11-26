//! Interactive terminal access to a replicated LAN match.
use std::{
    collections::BTreeMap,
    env,
    io::{self, BufRead, IsTerminal, Write},
    net::SocketAddr,
};

use histeries_net::{
    LockstepHost,
    lan::{LanClient, LanError, LanServer, Refusal, SyncOutcome},
};
use histeries_rules_core::PlayerId;
use histeries_sim::Command;
use serde_json::{Value, json};

use crate::{flag_string, flag_u32, interactive_session, validate_flags};

const HELP: &str = "observe: sync and show your seat's view; sync: catch up; end: end your turn; send a JSON command with a stable command ID, or type quit";

pub(super) fn run(args: &[String]) -> Result<(), String> {
    match args.split_first().map(|(action, rest)| (action.as_str(), rest)) {
        Some(("host", rest)) => host(rest),
        Some(("join", rest)) => join(rest),
        _ => Err("usage: histeries lan host [--listen IP:PORT] [--state PATH | --players N --seed N] or lan join --connect IP:PORT --seat ID".into()),
    }
}

pub(super) fn host(args: &[String]) -> Result<(), String> {
    validate_flags(
        args,
        &["--listen", "--players", "--seed", "--state", "--globe"],
        0,
    )?;
    let address = flag_string(args, "--listen")?.unwrap_or_else(|| "127.0.0.1:7878".into());
    let session = interactive_session(args, histeries_sim::seeded_frontier)?;
    let mut tokens = BTreeMap::new();
    for player in session.seat_ids() {
        let key = format!("HISTERIES_LAN_TOKEN_{}", player.index());
        tokens.insert(
            player,
            env::var(&key).map_err(|_| format!("set {key} for this seat"))?,
        );
    }
    let server = LanServer::bind(
        parse_address(&address)?,
        LockstepHost::new(session)?,
        tokens,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        json!({"listening": server.local_addr().map_err(|e| e.to_string())?.to_string()})
    );
    io::stdout().flush().map_err(|e| e.to_string())?;
    server.serve().map_err(|error| error.to_string())
}

fn parse_address(value: &str) -> Result<SocketAddr, String> {
    value
        .parse()
        .map_err(|_| "expected an IP address and port".to_owned())
}

fn join(args: &[String]) -> Result<(), String> {
    validate_flags(args, &["--connect", "--seat"], 0)?;
    let address = flag_string(args, "--connect")?.ok_or("--connect IP:PORT is required")?;
    let seat = PlayerId::new(flag_u32(args, "--seat")?.ok_or("--seat ID is required")?);
    let token = env::var("HISTERIES_LAN_TOKEN")
        .map_err(|_| "set HISTERIES_LAN_TOKEN for the assigned seat")?;
    let mut client =
        LanClient::connect(parse_address(&address)?, seat, token).map_err(|e| e.to_string())?;
    let terminal = io::stdin().is_terminal();
    if terminal {
        eprintln!("{HELP}");
    }
    println!("{}", status(&client, seat));
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let input = line.trim();
        if input == "quit" {
            break;
        }
        if input.is_empty() {
            continue;
        }
        match handle_input(&mut client, seat, input, terminal) {
            Ok(value) => println!("{value}"),
            Err(message) if terminal => eprintln!("{message}"),
            Err(message) => return Err(message),
        }
    }
    if !terminal && client.needs_recovery() {
        return Err("disconnected before synchronizing the pending submission".into());
    }
    Ok(())
}

fn status(client: &LanClient, seat: PlayerId) -> Value {
    let replica = client.replica();
    let position = replica.position();
    json!({
        "seat": seat.index(), "active_seat": replica.active_seat().map(|s| s.index()),
        "sequence": position.sequence, "turn": position.turn, "state_hash": format!("{:016x}", position.hash),
        "winner": replica.session().winner(),
    })
}

fn handle_input(
    client: &mut LanClient,
    seat: PlayerId,
    input: &str,
    terminal: bool,
) -> Result<Value, String> {
    let submission = match input {
        "help" => return Ok(json!({"help": HELP})),
        "sync" | "observe" => {
            let update = match client.sync() {
                Ok(update) => update,
                Err(error) => return request_error(client, error, terminal),
            };
            let mut response = status(client, seat);
            response["sync"] = match update {
                SyncOutcome::Replayed { commands } => json!({"replayed": commands}),
                SyncOutcome::Checkpoint { previous, current } => {
                    json!({"checkpoint_from": previous.sequence, "checkpoint_to": current.sequence})
                }
            };
            if let Some(outcome) = client.submission_outcome() {
                response["recovered_submission"] = json!(outcome);
            }
            if input == "observe" {
                let view = client
                    .replica()
                    .session()
                    .observe(seat)
                    .map_err(|e| e.to_string())?;
                response["observation"] = serde_json::to_value(view).map_err(|e| e.to_string())?;
            }
            return Ok(response);
        }
        "end" => client.submit(Command::TurnEnd { player: seat }),
        json => {
            let mut fields: serde_json::Map<String, Value> = serde_json::from_str(json)
                .map_err(|e| format!("expected a command object ({e}); {HELP}"))?;
            fields
                .entry("player")
                .or_insert_with(|| json!(seat.index()));
            let command =
                serde_json::from_value(Value::Object(fields)).map_err(|e| e.to_string())?;
            client.submit(command)
        }
    };
    match submission {
        Ok(()) => Ok(status(client, seat)),
        Err(error) => request_error(client, error, terminal),
    }
}

fn request_error(client: &LanClient, error: LanError, terminal: bool) -> Result<Value, String> {
    let message = error.to_string();
    if !terminal
        && client.needs_recovery()
        && !matches!(
            error,
            LanError::Refused(
                Refusal::Credentials
                    | Refusal::RecoveryExpired
                    | Refusal::Version { .. }
                    | Refusal::Rejected(_)
            )
        )
    {
        return Ok(json!({"error": message, "recovery_required": true}));
    }
    Err(message)
}
