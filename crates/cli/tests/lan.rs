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
