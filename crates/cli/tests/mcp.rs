//! Exercise the public MCP transports and a complete two-seat game from client observations.
#![cfg(not(target_family = "wasm"))]

use std::{
    io::{BufRead, BufReader, Write},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    time::Duration,
};

use http_body_util::{BodyExt, Full};
use hyper::{Request, body::Bytes, client::conn::http1};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn server() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_histeries"));
    cmd.arg("mcp")
        .env("HISTERIES_MCP_SEAT_1_TOKEN", "one")
        .env("HISTERIES_MCP_SEAT_2_TOKEN", "two")
        .env("HISTERIES_MCP_SPECTATOR_TOKEN", "watch")
        .env("HISTERIES_MCP_EDITOR_TOKEN", "edit");
    cmd
}

fn rpc(method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{},
        "io.modelcontextprotocol/clientInfo":{"name":"histeries-acceptance","version":"1"}
    });
    json!({"id":1,"jsonrpc":"2.0","method":method,"params":params})
}

#[test]
fn stdio_discovers_and_advances_a_playable_game() {
    check_stdio(server().env("HISTERIES_MCP_TOKEN", "one"), 1, 0);
}

fn check_stdio(command: &mut Command, seat: u32, turn: u32) {
    with_stdio(command, |call| {
        assert!(
            call("server/discover", json!({}))
                .get("capabilities")
                .is_some()
        );
        let observed = call("tools/call", json!({"name":"observe","arguments":{}}));
        assert_eq!(observed["structuredContent"]["seat"], seat);
        assert_eq!(observed["structuredContent"]["turn"], turn);
        assert_eq!(observed["structuredContent"]["turn_ended"], false);
        let next = call("tools/call", json!({"name":"advance_turn","arguments":{}}));
        assert_eq!(next["structuredContent"]["turn"], turn + 1, "{next}");
    });
}

fn with_stdio(command: &mut Command, play: impl FnOnce(&mut dyn FnMut(&str, Value) -> Value)) {
    with_stdio_requests(command, |exchange| {
        play(&mut |method, params| {
            let reply = exchange(&rpc(method, params));
            assert!(reply.get("error").is_none(), "{reply}");
            reply["result"].clone()
        });
    });
}

fn with_stdio_requests(command: &mut Command, play: impl FnOnce(&mut dyn FnMut(&Value) -> Value)) {
    let mut child = Server(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = child.0.stdin.take().unwrap();
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    play(&mut |request| {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], request["id"]);
        reply
    });
}

#[path = "mcp/recorded_agent.rs"]
mod recorded_agent;

#[test]
fn stdio_resumes_nonconsecutive_saved_seats_halfway_through_round() {
    use histeries_rules_core::PlayerId;
    use histeries_sim::{Command as Order, Session};
    let mut session = Session::new([9, 27, 73].map(PlayerId::new));
    for player in [9, 73] {
        session
            .dispatch(Order::TurnEnd {
                player: PlayerId::new(player),
            })
            .unwrap();
    }
    let path = std::env::temp_dir().join(format!("histeries-mcp-save-{}.hst", std::process::id()));
    std::fs::write(&path, session.state_snapshot().encode()).unwrap();
    let mut command = server();
    command
        .arg("--state")
        .arg(&path)
        .env("HISTERIES_MCP_TOKEN", "saved-seat")
        .env("HISTERIES_MCP_SEAT_9_TOKEN", "saved-seat");
    check_stdio(&mut command, 9, 1);
    let conflict = command.args(["--seed", "10"]).output().unwrap();
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("omit --players and --seed"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn stdio_bots_contest_a_tile_from_the_same_round_snapshot() {
    use histeries_ai::{GreedyPlayer, Player};
    use histeries_hex::{FlatHex, TileId};
    use histeries_rules_core::{PlayerId, UnitId};
    use histeries_sim::{Command as Order, MapState, Session, UnitKind};

    let seats = [1, 2, 3].map(PlayerId::new);
    let mut session = Session::with_map(
        seats,
        MapState::blank(FlatHex::new(3, 2, false, false).unwrap()),
    );
    // Both warriors want tile 1. The human builder sees the contest from tile 4.
    for (player, kind, tile) in [
        (seats[0], UnitKind::Builder, 4),
        (seats[1], UnitKind::Warrior, 0),
        (seats[2], UnitKind::Warrior, 2),
    ] {
        session
            .dispatch(Order::UnitSpawn {
                player,
                unit: UnitId::new(player.index()),
                kind,
                tile: TileId::new(tile),
            })
            .unwrap();
    }
    let target = TileId::new(1);
    let plan_for = |game: &Session, seat| GreedyPlayer.plan(&game.observe(seat).unwrap()).commands;
    for seat in &seats[1..] {
        assert!(plan_for(&session, *seat).contains(&Order::UnitMove {
            player: *seat,
            unit: UnitId::new(seat.index()),
            destination: target,
        }));
    }
    let mut sequential = session.clone();
    sequential
        .dispatch(Order::TurnEnd { player: seats[0] })
        .unwrap();
    for order in plan_for(&sequential, seats[1]) {
        sequential.dispatch(order).unwrap();
    }
    assert!(
        plan_for(&sequential, seats[2]).contains(&Order::UnitAttack {
            player: seats[2],
            attacker: UnitId::new(3),
            defender: UnitId::new(2),
        })
    );

    let path =
        std::env::temp_dir().join(format!("histeries-mcp-contest-{}.hst", std::process::id()));
    for human_already_ended in [false, true] {
        if human_already_ended {
            session
                .dispatch(Order::TurnEnd { player: seats[0] })
                .unwrap();
        }
        std::fs::write(&path, session.state_snapshot().encode()).unwrap();
        with_stdio(
            server()
                .arg("--state")
                .arg(&path)
                .env("HISTERIES_MCP_TOKEN", "one"),
            |call| {
                let action = if human_already_ended {
                    "observe"
                } else {
                    "advance_turn"
                };
                let live = call("tools/call", json!({"name":action,"arguments":{}}));
                let view = &live["structuredContent"];
                assert_eq!(view["turn"], 1, "{live}");
                assert_eq!(view["turn_ended"], false);
                let units = view["visible_units"].as_array().unwrap();
                for (id, tile) in [(2, 1), (3, 2)] {
                    let unit = units.iter().find(|u| u["id"] == id).unwrap();
                    assert_eq!(unit["tile"], tile);
                    // Seat 3's move became illegal. Replanning would attack seat 2.
                    assert_eq!(unit["hp"], UnitKind::Warrior.max_hp());
                }
                let replay = call(
                    "tools/call",
                    json!({"name":"replay","arguments":{"turn":1}}),
                );
                assert_eq!(
                    replay["structuredContent"]["visible_units"],
                    view["visible_units"]
                );
            },
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn stdio_replays_a_bot_victory_before_the_round_boundary() {
    use histeries_hex::{FlatHex, TileId};
    use histeries_rules_core::{PlayerId, UnitId};
    use histeries_sim::{Command as Order, MapState, Session, UnitKind};
    let seats = [1, 2].map(PlayerId::new);
    let mut session = Session::with_map(
        seats,
        MapState::blank(FlatHex::new(5, 1, false, false).unwrap()),
    );
    for (seat, site) in [(seats[0], 0), (seats[1], 4)] {
        let unit = UnitId::new(seat.index());
        session
            .dispatch(Order::UnitSpawn {
                player: seat,
                unit,
                kind: UnitKind::Settler,
                tile: TileId::new(site),
            })
            .unwrap();
        session
            .dispatch(Order::CitySettle { player: seat, unit })
            .unwrap();
    }
    session
        .dispatch(Order::UnitSpawn {
            player: seats[1],
            unit: UnitId::new(99),
            kind: UnitKind::Warrior,
            tile: TileId::new(1),
        })
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "histeries-mcp-domination-{}.hst",
        std::process::id()
    ));
    std::fs::write(&path, session.state_snapshot().encode()).unwrap();
    with_stdio(
        server()
            .arg("--state")
            .arg(&path)
            .env("HISTERIES_MCP_TOKEN", "one"),
        |call| {
            let observed = call("tools/call", json!({"name":"observe","arguments":{}}));
            assert!(observed["structuredContent"]["winner"].is_null());
            let next = call("tools/call", json!({"name":"advance_turn","arguments":{}}));
            let end = &next["structuredContent"];
            assert_eq!(end["turn"], 0);
            assert_eq!(end["winner"], json!({"seat":2,"kind":"domination"}));
            let replay = call(
                "tools/call",
                json!({"name":"replay","arguments":{"turn":0}}),
            );
            assert_eq!(replay["structuredContent"]["winner"], end["winner"]);
            assert_eq!(replay["structuredContent"]["cities"], end["cities"]);
        },
    );
    std::fs::remove_file(path).unwrap();
}

struct Client(SocketAddr);

impl Client {
    async fn request(&self, token: &str, method: &str, params: Value) -> (u16, Value) {
        let socket = tokio::net::TcpStream::connect(self.0).await.unwrap();
        let (mut sender, connection) = http1::handshake(TokioIo::new(socket)).await.unwrap();
        tokio::spawn(connection);
        let mut req = Request::post("/mcp")
            .header("host", self.0.to_string())
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", method)
            .header("authorization", format!("Bearer {token}"));
        if let Some(name) = params
            .get("name")
            .or(params.get("uri"))
            .and_then(Value::as_str)
        {
            req = req.header("Mcp-Name", name);
        }
        let req = req
            .body(Full::new(Bytes::from(rpc(method, params).to_string())))
            .unwrap();
        let response = sender.send_request(req).await.unwrap();
        let status = response.status().as_u16();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
        )
    }

    async fn tool(&self, token: &str, name: &str, arguments: Value) -> Value {
        let (status, reply) = self
            .request(
                token,
                "tools/call",
                json!({"name":name,"arguments":arguments}),
            )
            .await;
        assert_eq!(status, 200, "{reply}");
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    async fn value(&self, token: &str, name: &str, args: Value) -> Value {
        let result = self.tool(token, name, args).await;
        assert_ne!(result["isError"], true, "{result}");
        result["structuredContent"].clone()
    }
}

#[test]
fn http_seat_permissions_history_and_two_player_science_victory() {
    let mut child = Server(
        server()
            .args(["--listen", "127.0.0.1:0"])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    BufReader::new(child.0.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let address = line
        .trim()
        .strip_prefix("Histeries MCP listening at http://")
        .and_then(|s| s.strip_suffix("/mcp"))
        .expect(&line)
        .parse()
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(90), play(&Client(address)))
            .await
            .expect("game exceeded time limit");
    });
}

async fn play(client: &Client) {
    assert_eq!(
        client.request("wrong", "tools/list", json!({})).await.0,
        401
    );
    let (_, tools) = client.request("watch", "tools/list", json!({})).await;
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 3);
    let (_, denied) = client
        .request(
            "watch",
            "tools/call",
            json!({"name":"advance_turn","arguments":{}}),
        )
        .await;
    assert!(denied.get("error").is_some());
    let before = client.value("one", "observe", json!({})).await;
    let other = client.value("two", "observe", json!({})).await;
    assert_ne!(before["visible_tiles"], other["visible_tiles"]);
    assert_eq!(before["cities"].as_array().unwrap().len(), 1);
    assert_eq!(
        client
            .tool(
                "one",
                "dispatch",
                json!({"id":"editor.set_marker","args":{"value":8}})
            )
            .await["isError"],
        true
    );
    assert_eq!(
        client
            .tool(
                "one",
                "dispatch",
                json!({"id":"turn.end","args":{"player":2}})
            )
            .await["isError"],
        true
    );
    let foreign_unit = &other["units"][0]["id"];
    assert_eq!(
        client
            .tool(
                "one",
                "dispatch",
                json!({"id":"unit.hold","args":{"unit":foreign_unit}})
            )
            .await["isError"],
        true
    );
    client
        .value(
            "edit",
            "dispatch",
            json!({"id":"editor.set_marker","args":{"value":8}}),
        )
        .await;
    assert_eq!(
        client.value("two", "observe", json!({})).await["own_marker"],
        0
    );
    assert_eq!(
        client.value("one", "replay", json!({"turn":0})).await["own_marker"],
        0
    );
    let (_, res) = client
        .request(
            "one",
            "resources/read",
            json!({"uri":"histeries://ruleset/base"}),
        )
        .await;
    let rules: toml::Value = toml::from_str(
        res["result"]["contents"][0]["text"]
            .as_str()
            .expect("ruleset resource"),
    )
    .unwrap();
    let techs = rules["technologies"].as_array().unwrap();
    let mut view = before;
    assert_eq!(view["progress"]["age"]["id"], "founding");
    let orbital_policy =
        json!({"id":"policy.adopt","args":{"slot":0,"policy":"policy_orbital_research"}});
    assert_eq!(
        client.tool("one", "dispatch", orbital_policy.clone()).await["isError"],
        true
    );
    let mut observed_ages = Vec::new();
    for _ in 0..500 {
        let progress = &view["progress"];
        let age = progress["age"]["id"].as_str().unwrap();
        if observed_ages.last().map(String::as_str) != Some(age) {
            observed_ages.push(age.to_owned());
            if age == "orbital" {
                assert!(
                    progress["age"]["unlocks"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("orbital_access"))
                );
                client
                    .value("one", "dispatch", orbital_policy.clone())
                    .await;
            }
        }
        if progress["current_technology"].is_null() {
            let completed = progress["technologies"].as_array().unwrap();
            let next = techs
                .iter()
                .find(|t| {
                    let id = t["id"].as_str().unwrap();
                    !completed.iter().any(|known| known == id)
                        && t["prerequisites"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|p| completed.iter().any(|known| known == p.as_str().unwrap()))
                })
                .expect("an available technology");
            client.value("one","dispatch",json!({"id":"research.technology","args":{"technology":next["id"].as_str().unwrap()}})).await;
        }
        client.value("one", "advance_turn", json!({})).await;
        let ending = client.value("two", "advance_turn", json!({})).await;
        view = client.value("one", "observe", json!({})).await;
        if !view["winner"].is_null() {
            assert_eq!(view["winner"], json!({"seat":1,"kind":"science"}));
            assert_eq!(view["winner"], ending["winner"]);
            assert_eq!(
                view["progress"]["technologies"].as_array().unwrap().len(),
                techs.len()
            );
            let replay = client
                .value("two", "replay", json!({"turn":view["turn"]}))
                .await;
            assert_eq!(replay["winner"], view["winner"]);
            assert_eq!(replay["progress"]["technologies"], json!([]));
            assert_eq!(replay["progress"]["age"]["id"], "founding");
            assert_eq!(observed_ages, ["founding", "industrial", "orbital"]);
            assert_eq!(
                client.tool("one", "replay", json!({"turn":0})).await["isError"],
                true
            );
            return;
        }
    }
    panic!("game failed to finish in 500 rounds");
}
