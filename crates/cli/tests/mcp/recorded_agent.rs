//! Replay external agents' actual requests and compare the responses they used to make decisions.

use std::io::{BufRead, BufReader};

use flate2::read::GzDecoder;
use serde_json::{Value, json};

use super::{rpc, server, with_stdio_requests};

#[test]
fn recorded_agent_science_game() {
    replay_game(
        include_bytes!("../../../../docs/sessions/2026-11-19-mcp-llm-game.jsonl.gz"),
        "20261119",
        87,
        60,
    );
}

#[test]
fn recorded_agent_game_with_scouting_and_capture() {
    replay_game(
        include_bytes!("../../../../docs/sessions/2026-11-19-phase2-mcp-game.jsonl.gz"),
        "20261120",
        132,
        57,
    );
}

fn replay_game(archive: &[u8], seed: &str, exchanges: usize, winning_turn: u32) {
    let mut command = server();
    command
        .args(["--players", "2", "--seed", seed])
        .env_remove("HISTERIES_MCP_SEAT_2_TOKEN")
        .env_remove("HISTERIES_MCP_EDITOR_TOKEN")
        .env_remove("HISTERIES_MCP_SPECTATOR_TOKEN")
        .env("HISTERIES_MCP_TOKEN", "one");
    with_stdio_requests(&mut command, |exchange| {
        let mut history = BufReader::new(GzDecoder::new(archive)).lines();
        let mut terminal = Value::Null;
        for index in 0..exchanges {
            let line = history.next().expect("incomplete agent transcript");
            let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let request = &row["request"];
            let expected = &row["response"];
            assert_eq!(request["id"], index + 1);
            assert_eq!(expected["id"], request["id"]);
            let response = exchange(request);
            assert!(
                response.get("error").is_none(),
                "exchange {}: {response}",
                index + 1
            );
            let result = &response["result"];
            let recorded = &expected["result"];
            let method = request["method"].as_str().unwrap();
            if index == 0 {
                assert_eq!(method, "server/discover");
            }
            if index + 1 == exchanges {
                assert_eq!(method, "tools/call");
                assert_eq!(request["params"]["name"], "replay");
                assert_eq!(request["params"]["arguments"]["turn"], winning_turn);
                assert_eq!(terminal, result["structuredContent"]);
            }
            match method {
                "server/discover" => {
                    assert!(
                        result["supportedVersions"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("2026-07-28"))
                    );
                    for capability in ["tools", "resources"] {
                        assert!(result["capabilities"][capability].is_object());
                    }
                }
                "tools/list" => compare_catalog(result, recorded, "tools", "name", "inputSchema"),
                "resources/list" => {
                    compare_catalog(result, recorded, "resources", "uri", "mimeType")
                }
                "resources/read" => {
                    assert_eq!(request["params"]["uri"], "histeries://ruleset/base");
                    let rules = |result: &Value| -> toml::Value {
                        let resource = &result["contents"][0];
                        assert_eq!(resource["uri"], request["params"]["uri"]);
                        toml::from_str(resource["text"].as_str().unwrap()).unwrap()
                    };
                    assert_eq!(rules(result), rules(recorded));
                }
                "tools/call" => {
                    let name = request["params"]["name"].as_str().unwrap();
                    assert!(matches!(
                        name,
                        "observe" | "list_commands" | "dispatch" | "advance_turn" | "replay"
                    ));
                    assert_eq!(result["isError"] == true, recorded["isError"] == true);
                    let view = &result["structuredContent"];
                    assert_eq!(
                        view,
                        &recorded["structuredContent"],
                        "exchange {} ({name})",
                        index + 1
                    );
                    if name != "list_commands" && result["isError"] != true {
                        assert_eq!(view["seat"], 1);
                    }
                    if name != "replay" && !view["winner"].is_null() {
                        assert_eq!(view["turn"], winning_turn);
                        assert_eq!(view["winner"], json!({"seat":2, "kind":"science"}));
                        terminal = view.clone();
                        terminal.as_object_mut().unwrap().remove("turn_ended");
                    }
                }
                _ => panic!("unexpected method {method} in recorded game"),
            }
        }
        assert!(history.next().is_none(), "unplayed exchanges in transcript");
        assert!(terminal.is_object());
        let ended = exchange(&rpc(
            "tools/call",
            json!({"name":"advance_turn", "arguments":{}}),
        ));
        assert_eq!(ended["result"]["isError"], true, "{ended}");
    });
}

// SDK-generated metadata and human-readable descriptions can evolve independently of the game.
fn compare_catalog(result: &Value, recorded: &Value, catalog: &str, id: &str, field: &str) {
    let entries = result[catalog].as_array().unwrap();
    let expected = recorded[catalog].as_array().unwrap();
    assert_eq!(entries.len(), expected.len());
    for item in expected {
        let entry = entries.iter().find(|entry| entry[id] == item[id]).unwrap();
        assert_eq!(entry[field], item[field]);
    }
}
