//! Exercise a host and two independently replicated players through the public CLI.
#![cfg(not(target_family = "wasm"))]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::SocketAddr,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use histeries_ai::{Player, PlayerPlan, ThreeLayerPlayer, plan_seats};
use histeries_hex::TileId;
use histeries_rules_core::PlayerId;
use histeries_sim::{Command as Order, Observation, Session};
use serde_json::{Value, json};

const DEADLINE: Duration = Duration::from_secs(15);

struct Process {
    child: Child,
    stdout: Receiver<String>,
}

impl Process {
    fn spawn(args: &[&str], vars: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_histeries"))
            .args(args)
            .envs(vars.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pipe = child.stdout.take().unwrap();
        let (send, stdout) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, stdout }
    }

    fn join(address: &str, seat: &str, token: &str) -> Self {
        Self::spawn(
            &["lan", "join", "--connect", address, "--seat", seat],
            &[("HISTERIES_LAN_TOKEN", token)],
        )
    }

    fn mcp(address: &str, token: &str) -> Self {
        Self::spawn(
            &["mcp", "--connect", address, "--seat", "1"],
            &[("HISTERIES_LAN_TOKEN", token)],
        )
    }

    fn read(&self) -> Value {
        let line = self
            .stdout
            .recv_timeout(DEADLINE)
            .expect("timely CLI reply");
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("invalid CLI reply {line:?}: {error}"))
    }

    fn send(&mut self, line: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{line}").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, input: &str) -> Value {
        self.send(input);
        self.read()
    }

    fn rpc(&mut self, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": {"name":"lan-acceptance","version":"1"}
        });
        let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let reply = self.request(&request.to_string());
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], request["id"]);
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.rpc("tools/call", json!({"name":name,"arguments":arguments}));
        assert!(
            !result.to_string().contains("state_hash"),
            "{name}: {result}"
        );
        result
    }

    fn value(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.tool(name, arguments);
        assert_ne!(result["isError"], true, "{name}: {result}");
        result["structuredContent"].clone()
    }

    fn exit(&mut self) -> (ExitStatus, String) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut stderr = String::new();
                self.child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                return (status, stderr);
            }
            assert!(Instant::now() < deadline, "CLI process failed to exit");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn assert_turn(left: &Value, right: &Value, turn: u32, active: u32) {
    for status in [left, right] {
        assert_eq!(status["turn"], turn, "{status}");
        assert_eq!(status["active_seat"], active, "{status}");
    }
}

fn assert_replica_position(left: &Value, right: &Value, sequence: u64, turn: u32, active: u32) {
    assert_turn(left, right, turn, active);
    assert_eq!(left["sequence"], sequence);
    assert_eq!(right["sequence"], sequence);
    let hash = left["state_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 16);
    u64::from_str_radix(hash, 16).unwrap();
    assert_eq!(left["state_hash"], right["state_hash"]);
}

fn host() -> Process {
    Process::spawn(
        &[
            "serve",
            "--listen",
            "127.0.0.1:0",
            "--players",
            "2",
            "--seed",
            "7",
        ],
        &[
            ("HISTERIES_LAN_TOKEN_1", "one"),
            ("HISTERIES_LAN_TOKEN_2", "two"),
        ],
    )
}

#[test]
fn stdio_mcp_can_start_and_replay_a_spherical_game() {
    let mut mcp = Process::spawn(
        &["mcp", "--globe", "3", "--seed", "7"],
        &[
            ("HISTERIES_MCP_SEAT_1_TOKEN", "test-seat"),
            ("HISTERIES_MCP_TOKEN", "test-seat"),
        ],
    );
    let initial = mcp.value("observe", json!({}));
    assert_eq!(initial["globe_frequency"], 3);
    let tiles = initial["visible_tiles"].as_array().unwrap();
    assert!(!tiles.is_empty() && tiles.len() < 92);
    let next = mcp.value("advance_turn", json!({}));
    assert_eq!(next["turn"], 1);
    assert_eq!(next["globe_frequency"], 3);
    let prior = mcp.value("replay", json!({"turn":0}));
    assert_eq!(prior["visible_tiles"], initial["visible_tiles"]);
}

#[test]
fn cli_globe_save_atlas_and_replay_use_the_same_tiles() {
    use histeries_sim::{ReplayLog, Session};
    let folder = std::env::temp_dir().join(format!("histeries-cli-globe-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_histeries"))
            .current_dir(&folder)
            .args(args)
            .output()
            .unwrap()
    };
    let new = run(&[
        "new",
        "--globe",
        "3",
        "--seed",
        "7",
        "--output",
        "world.hst",
    ]);
    assert!(
        new.status.success(),
        "{}",
        String::from_utf8_lossy(&new.stderr)
    );
    let atlas = run(&["map", "world.hst"]);
    assert!(atlas.status.success());
    let ascii = String::from_utf8(atlas.stdout).unwrap();
    let mut rows = ascii.lines();
    assert_eq!(rows.next(), Some("Geodesic atlas, frequency 3, 92 cells"));
    assert_eq!(
        rows.flat_map(str::chars)
            .filter(|c| !c.is_whitespace())
            .count(),
        92
    );
    let step = run(&[
        "step",
        "world.hst",
        "--player",
        "1",
        "--output",
        "step.hst",
        "--replay-output",
        "world.hrp",
    ]);
    assert!(
        step.status.success(),
        "{}",
        String::from_utf8_lossy(&step.stderr)
    );
    assert!(run(&["replay", "world.hrp"]).status.success());
    let state = Session::from_save_bytes(&std::fs::read(folder.join("step.hst")).unwrap()).unwrap();
    let replay = ReplayLog::decode(&std::fs::read(folder.join("world.hrp")).unwrap()).unwrap();
    assert_eq!(state.state_hash(), replay.replay().unwrap().state_hash());
    let bad = run(&["new", "--globe", "3", "--width", "8", "--output", "bad.hst"]);
    assert!(!bad.status.success() && !folder.join("bad.hst").exists());
    for name in ["world.hst", "step.hst", "world.hrp"] {
        std::fs::remove_file(folder.join(name)).unwrap();
    }
    std::fs::remove_dir(folder).unwrap();
}

#[derive(Debug, Eq, PartialEq)]
struct GlobeTrace {
    rounds: Vec<GlobeRound>,
    expedition_orders: Vec<(u32, Order)>,
    winner: (histeries_rules_core::PlayerId, histeries_sim::VictoryKind),
    accepted: usize,
    terminal_hash: histeries_sim::StateHash,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
struct GlobeRound {
    turn: u32,
    hash: String,
    seat_ages: [String; 2],
    seat_surveys: [Option<SurveyTrace>; 2],
}

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
struct SurveyTrace {
    expedition: Option<TileId>,
    visited: Vec<TileId>,
    charted: Vec<TileId>,
    visible: Vec<TileId>,
}

fn globe_round(session: &Session) -> GlobeRound {
    let views = [1, 2].map(|seat| session.observe(PlayerId::new(seat)).unwrap());
    GlobeRound {
        turn: session.turn(),
        hash: format!("{:016x}", session.state_hash().value()),
        seat_ages: views
            .each_ref()
            .map(|v| v.progress().age.as_ref().unwrap().id.clone()),
        seat_surveys: views.each_ref().map(|v| {
            let orbit = v.theatres().iter().find(|board| board.id == "orbit")?;
            Some(SurveyTrace {
                expedition: orbit.expedition,
                visited: orbit
                    .tiles
                    .iter()
                    .filter(|t| t.visited)
                    .map(|t| t.tile)
                    .collect(),
                charted: orbit.tiles.iter().map(|t| t.tile).collect(),
                visible: orbit
                    .tiles
                    .iter()
                    .filter(|t| t.visible)
                    .map(|t| t.tile)
                    .collect(),
            })
        }),
    }
}

// The LAN player chooses all four expedition orders from its own current chart.
#[derive(Default)]
struct SurveyPlayer(ThreeLayerPlayer);

impl Player for SurveyPlayer {
    fn plan(&mut self, view: &Observation) -> PlayerPlan {
        let mut plan = self.0.plan(view);
        if let Some(order) = survey_order(view) {
            plan.commands.insert(0, order);
        }
        plan
    }
}

fn survey_order(view: &Observation) -> Option<Order> {
    let board = view.theatres().iter().find(|board| board.id == "orbit")?;
    let player = view.seat();
    let theatre = board.id.clone();
    if board.can_launch && board.tiles.iter().all(|t| !t.visited) {
        return Some(Order::TheatreLaunch {
            player,
            theatre,
            city: view.cities()[0].id,
        });
    }
    if !board.can_move {
        return None;
    }
    if board.expedition != Some(board.entry) {
        return Some(Order::TheatreMove {
            player,
            theatre,
            tile: board.entry,
        });
    }
    if board
        .tiles
        .iter()
        .any(|t| t.visited && t.tile != board.entry)
    {
        assert!(board.can_recall);
        return Some(Order::TheatreRecall { player, theatre });
    }
    let entry = board.tiles.iter().find(|t| t.tile == board.entry).unwrap();
    Some(Order::TheatreMove {
        player,
        theatre,
        tile: entry.neighbors[0],
    })
}

fn play_geodesic_lan_game(workers: usize) -> GlobeTrace {
    use histeries_net::lan::{LanClient, LanError, Refusal};
    use histeries_sim::ReplayLog;

    let host = Process::spawn(
        &[
            "serve",
            "--listen",
            "127.0.0.1:0",
            "--players",
            "2",
            "--seed",
            "7",
            "--globe",
            "3",
        ],
        &[
            ("HISTERIES_LAN_TOKEN_1", "one"),
            ("HISTERIES_LAN_TOKEN_2", "two"),
        ],
    );
    let address: SocketAddr = host.read()["listening"].as_str().unwrap().parse().unwrap();
    let mut peers = [
        LanClient::connect(address, PlayerId::new(1), "one".into()).unwrap(),
        LanClient::connect(address, PlayerId::new(2), "two".into()).unwrap(),
    ];
    let mut reference =
        Session::from_save_bytes(&peers[0].replica().session().save_bytes()).unwrap();
    let mut surface = ThreeLayerPlayer::default();
    let mut survey = SurveyPlayer::default();
    let mut boundaries = Vec::new();
    let mut expedition_orders = Vec::new();
    let mut reconnected = false;
    while reference.winner().is_none() {
        let round = reference.turn();
        assert!(round < 500, "globe game failed to finish");
        // Both seats plan from their own view of the same round boundary.
        let planners: [&mut (dyn Player + Send); 2] = [&mut surface, &mut survey];
        let mut inputs: Vec<_> = peers
            .iter()
            .zip(planners)
            .map(|(peer, planner)| {
                assert_eq!(
                    peer.replica().position().hash,
                    reference.state_hash().value()
                );
                let visible = peer.replica().session().observe(peer.seat()).unwrap();
                assert_eq!(visible.board().globe_frequency(), Some(3));
                (planner, visible)
            })
            .collect();
        let plans = plan_seats(&mut inputs, workers);
        for (peer, plan) in peers.iter_mut().zip(plans) {
            if reference.winner().is_some() {
                break;
            }
            peer.sync().unwrap();
            assert_eq!(
                peer.replica().position().hash,
                reference.state_hash().value()
            );
            for order in plan.commands {
                let surface_seat = PlayerId::new(1);
                let other_view = order
                    .id()
                    .starts_with("theatre.")
                    .then(|| reference.observe(surface_seat).unwrap());
                match peer.submit(order.clone()) {
                    Ok(()) => {
                        reference.dispatch(order.clone()).unwrap();
                        if let Some(before) = other_view {
                            expedition_orders.push((round, order));
                            assert_eq!(reference.observe(surface_seat).unwrap(), before);
                        }
                        assert_eq!(
                            peer.replica().position().hash,
                            reference.state_hash().value()
                        );
                    }
                    Err(LanError::Refused(Refusal::Rejected(_)))
                        if other_view.is_none() && !matches!(order, Order::TurnEnd { .. }) =>
                    {
                        assert!(reference.clone().dispatch(order).is_err());
                    }
                    Err(error) => panic!("globe submission failed: {error}"),
                }
                if reference.winner().is_some() {
                    break;
                }
            }
        }
        if reference.turn() != round {
            assert_eq!(reference.turn(), round + 1);
            boundaries.push(globe_round(&reference));
        } else {
            assert!(reference.winner().is_some(), "incomplete nonterminal round");
        }
        let loaded = Session::from_save_bytes(&reference.save_bytes()).unwrap();
        assert_eq!(loaded.state_hash(), reference.state_hash());
        for peer in &mut peers {
            peer.sync().unwrap();
            assert_eq!(
                peer.replica().position().hash,
                reference.state_hash().value()
            );
            assert_eq!(peer.replica().session().winner(), reference.winner());
            let observed = reference.observe(peer.seat()).unwrap();
            assert_eq!(loaded.observe(peer.seat()).unwrap(), observed);
            assert_eq!(
                peer.replica().session().observe(peer.seat()).unwrap(),
                observed
            );
        }
        if !reconnected && reference.turn() >= 5 {
            peers[1] = LanClient::connect(address, PlayerId::new(2), "two".into()).unwrap();
            reconnected = true;
        }
    }
    assert!(reconnected && !boundaries.is_empty());
    assert_eq!(
        expedition_orders
            .iter()
            .map(|(_, order)| order.id())
            .collect::<Vec<_>>(),
        [
            "theatre.launch",
            "theatre.move",
            "theatre.move",
            "theatre.recall"
        ]
    );
    let terminal = globe_round(&reference);
    let surveyed = terminal.seat_surveys[1].as_ref().unwrap();
    assert!(surveyed.expedition.is_none() && surveyed.visible.is_empty());
    assert_eq!(surveyed.visited.len(), 2);
    assert!(surveyed.charted.len() > surveyed.visited.len());
    let log = ReplayLog::decode(&reference.replay().encode()).unwrap();
    assert!(log.commands().iter().any(|order| matches!(
        order,
        Order::PolicyAdopt { policy, .. } if policy == "policy_orbital_research"
    )));
    let mut playback = log.playback_session();
    let mut reconstructed = Vec::new();
    for order in log.commands() {
        let before = playback.turn();
        playback.dispatch(order.clone()).unwrap();
        if playback.turn() != before {
            reconstructed.push(globe_round(&playback));
        }
    }
    assert_eq!(reconstructed, boundaries);
    assert_eq!(playback.save_bytes(), reference.save_bytes());
    let result = GlobeTrace {
        rounds: boundaries,
        expedition_orders,
        winner: reference.winner().unwrap(),
        accepted: log.commands().len(),
        terminal_hash: reference.state_hash(),
    };
    println!(
        "geodesic_game seed=7 frequency=3 workers={workers} rounds={} accepted={} winner={:?} hash={:016x}",
        result.rounds.len(),
        result.accepted,
        result.winner,
        result.terminal_hash.value()
    );
    result
}

#[test]
fn a_geodesic_lan_game_reaches_victory_and_replays_every_round() {
    let serial = play_geodesic_lan_game(1);
    assert_eq!(serial, play_geodesic_lan_game(8));
    if let Some(path) = std::env::var_os("HISTERIES_GLOBE_TRACE") {
        let mut trace = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        for round in &serial.rounds {
            writeln!(trace, "{}", serde_json::to_string(round).unwrap()).unwrap();
        }
        writeln!(trace, "{}", json!({"winner":serial.winner,"accepted":serial.accepted,"hash":format!("{:016x}",serial.terminal_hash.value()),"expedition_orders":serial.expedition_orders})).unwrap();
        trace.flush().unwrap();
    }
}

#[test]
fn two_cli_players_verify_orders_and_turn_handoffs_over_tcp() {
    let host = host();
    let listening = host.read();
    let address = listening["listening"].as_str().unwrap();
    let bound: SocketAddr = address.parse().unwrap();
    assert!(bound.ip().is_loopback());
    assert_ne!(bound.port(), 0);

    let mut denied = Process::join(address, "2", "one");
    let (exit, reason) = denied.exit();
    assert!(
        !exit.success() && reason.contains("credentials"),
        "{exit}: {reason}"
    );

    let mut forged = Process::join(address, "1", "one");
    forged.read();
    forged.send(r#"{"command":"turn.end","player":2}"#);
    let (exit, reason) = forged.exit();
    assert!(
        !exit.success() && reason.contains("another seat"),
        "{exit}: {reason}"
    );

    let mut alice = Process::join(address, "1", "one");
    let mut bob = Process::join(address, "2", "two");
    let a = alice.read();
    let b = bob.read();
    assert_eq!(a["seat"], 1);
    assert_eq!(b["seat"], 2);
    assert_replica_position(&a, &b, 0, 0, 1);

    let research = alice.request(r#"{"command":"research.technology","technology":"tech_01"}"#);
    let remote = bob.request("observe");
    assert_replica_position(&research, &remote, 1, 0, 1);
    assert_eq!(remote["sync"]["replayed"], 1);
    assert_eq!(remote["observation"]["seat"], 2);
    assert_eq!(
        remote["observation"]["progress"]["current_technology"],
        Value::Null
    );
    let own = alice.request("observe");
    assert_eq!(own["observation"]["seat"], 1);
    assert_eq!(
        own["observation"]["progress"]["current_technology"],
        "tech_01"
    );

    let handoff = alice.request("end");
    let active = bob.request("sync");
    assert_replica_position(&handoff, &active, 2, 0, 2);
    let next = bob.request("end");
    let verified = alice.request("sync");
    assert_replica_position(&next, &verified, 3, 1, 1);
    assert_eq!(verified["sync"]["replayed"], 1);
    assert_ne!(next["state_hash"], research["state_hash"]);

    for player in [&mut alice, &mut bob] {
        player.send("quit");
        let (exit, stderr) = player.exit();
        assert!(exit.success(), "{exit}: {stderr}");
    }
}

#[test]
fn mcp_and_terminal_share_verified_turns_and_seat_scoped_history() {
    let host = host();
    let listening = host.read();
    let address = listening["listening"].as_str().unwrap();
    let mut bob = Process::join(address, "2", "two");
    bob.read();

    let (exit, reason) = Process::mcp(address, "two").exit();
    assert!(
        !exit.success() && reason.contains("credentials"),
        "{exit}: {reason}"
    );

    let mut alice = Process::mcp(address, "one");
    let info = alice.rpc("server/discover", json!({}));
    assert!(info["capabilities"].get("tools").is_some());
    assert!(info["capabilities"].get("resources").is_some());
    let list = alice.rpc("tools/list", json!({}));
    let tools = list["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 5);
    for name in [
        "observe",
        "list_commands",
        "dispatch",
        "advance_turn",
        "replay",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}");
    }
    let listing = alice.rpc("resources/list", json!({}));
    let resources = listing["resources"].as_array().unwrap();
    for uri in [
        "histeries://docs/commands",
        "histeries://docs/mcp",
        "histeries://ruleset/base",
    ] {
        assert!(resources.iter().any(|r| r["uri"] == uri), "{uri}");
        let resource = alice.rpc("resources/read", json!({"uri":uri}));
        assert_eq!(resource["contents"][0]["uri"], uri);
        assert!(!resource["contents"][0]["text"].as_str().unwrap().is_empty());
    }
    let listing = alice.value("list_commands", json!({}));
    let commands = listing["commands"].as_array().unwrap();
    assert!(commands.iter().any(|c| c["id"] == "research.technology"));
    assert!(!commands.iter().any(|c| c["id"] == "editor.set_marker"));

    let opening = alice.value("observe", json!({}));
    assert!(opening.get("recovered_submission").is_none());
    let other = bob.request("observe");
    assert_turn(&opening, &other, 0, 1);
    assert!(opening.get("sequence").is_none());
    assert_eq!(opening["seat"], 1);
    assert_ne!(
        opening["visible_tiles"],
        other["observation"]["visible_tiles"]
    );
    for arguments in [
        json!({"id":"editor.set_marker","args":{"value":8}}),
        json!({"id":"turn.end","args":{"player":2}}),
    ] {
        assert_eq!(alice.tool("dispatch", arguments)["isError"], true);
    }
    assert_eq!(alice.value("observe", json!({})), opening);
    let research = alice.value(
        "dispatch",
        json!({"id":"research.technology","args":{"technology":"tech_01"}}),
    );
    assert_eq!(research["progress"]["current_technology"], "tech_01");
    let other = bob.request("observe");
    assert_turn(&research, &other, 0, 1);
    assert!(other["observation"]["progress"]["current_technology"].is_null());
    let handoff = alice.value("advance_turn", json!({}));
    assert_turn(&handoff, &bob.request("sync"), 0, 2);
    assert_eq!(handoff["turn_ended"], true);
    let waiting = alice.tool("advance_turn", json!({}));
    assert_eq!(waiting["isError"], true);

    // An opponent's private choices must be invisible both during catch-up and after
    // enough hidden orders to require a checkpoint instead of individual commits.
    let choose_research = r#"{"command":"research.technology","technology":"tech_01"}"#;
    bob.request(choose_research);
    assert_eq!(alice.tool("advance_turn", json!({})), waiting);
    assert_eq!(alice.value("observe", json!({})), handoff);
    for _ in 0..129 {
        bob.request(choose_research);
    }
    assert_eq!(alice.value("observe", json!({})), handoff);

    let next = bob.request("end");
    let stale = alice.tool("advance_turn", json!({}));
    assert_eq!(stale["isError"], true);
    assert_eq!(stale, waiting);
    assert!(
        !stale
            .to_string()
            .contains(next["state_hash"].as_str().unwrap())
    );
    assert_turn(&alice.value("observe", json!({})), &next, 1, 1);
    let handoff = alice.value("advance_turn", json!({}));
    assert_turn(&handoff, &bob.request("sync"), 1, 2);
    drop(alice);

    // A fresh replica receives only round 1's opening and suffix. Earlier views come from the host.
    let mut rejoined = Process::mcp(address, "one");
    assert_eq!(rejoined.value("observe", json!({})), handoff);
    let replay = rejoined.value("replay", json!({"turn":0}));
    let mut expected_view = opening;
    for field in ["active_seat", "turn_ended"] {
        expected_view.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(replay, expected_view);
    assert_eq!(rejoined.value("observe", json!({})), handoff);
}
