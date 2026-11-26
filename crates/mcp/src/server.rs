use std::{
    collections::BTreeMap,
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use histeries_net::lan::LanClient;
use histeries_rules_core::PlayerId;
use histeries_sim::Session;
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::*,
    service::RequestContext,
    transport::{
        stdio,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use serde_json::{Value, json};
use tower::ServiceExt as TowerServiceExt;

use crate::{
    game::{Game, MatchTools, Role, seat_lan_error},
    wire,
};

type ResourceSpec = (&'static str, &'static str, &'static str, &'static str);

const EDITOR_PACKAGE_DOCS: ResourceSpec = (
    "histeries://docs/editor-packages",
    "Editor package ABI",
    "text/markdown",
    include_str!("../../../docs/editor-packages.md"),
);

const RESOURCES: [ResourceSpec; 3] = [
    (
        "histeries://ruleset/base",
        "Base ruleset",
        "application/toml",
        include_str!("../../../data/rulesets/base/base.toml"),
    ),
    (
        "histeries://docs/commands",
        "Simulation command registry",
        "text/markdown",
        include_str!("../../../docs/commands.md"),
    ),
    (
        "histeries://docs/mcp",
        "Playing through MCP",
        "text/markdown",
        include_str!("../../../docs/mcp.md"),
    ),
];

#[derive(Clone)]
struct Handler {
    game: Arc<Mutex<dyn MatchTools>>,
    role: Role,
}

fn tool(
    name: &'static str,
    description: &'static str,
    properties: Value,
    required: &[&str],
) -> Tool {
    let schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    Tool::new(
        name,
        description,
        schema.as_object().expect("static object").clone(),
    )
}

impl Handler {
    fn resources(&self) -> impl Iterator<Item = ResourceSpec> {
        let editor_docs = matches!(self.role, Role::Editor(_)).then_some(EDITOR_PACKAGE_DOCS);
        RESOURCES.into_iter().chain(editor_docs)
    }

    fn tools(&self) -> Vec<Tool> {
        let mut tools = vec![
            tool(
                "list_commands",
                "List available simulation command IDs and argument schemas.",
                json!({}),
                &[],
            ),
            tool(
                "observe",
                "Read the current board, progression and winner visible to this credential.",
                json!({}),
                &[],
            ),
            tool(
                "replay",
                "Replay a completed round and read the resulting observation, including opening round 0. The last 32 rounds are retained.",
                json!({"turn":{"type":"integer","minimum":0,"maximum":u32::MAX}}),
                &["turn"],
            ),
        ];
        if self.role != Role::Spectator {
            tools.extend([
                tool("dispatch","Validate and apply one command as your seat. See list_commands for each args object.",json!({"id":{"type":"string"},"args":{"type":"object"}}),&["id","args"]),
                tool("advance_turn","End your seat's current turn and return the new observation.",json!({}),&[]),
            ]);
        }
        if matches!(self.role, Role::Editor(_)) {
            use histeries_sim::extensions::{MAX_MANIFEST_BYTES, MAX_MODULE_BYTES};
            tools.push(tool(
                "apply_editor_package",
                "Run one ABI v1 editor package atomically as this editor seat.",
                json!({"manifest":{"type":"string","description":"TOML text encoded in at most 4096 UTF-8 bytes.","maxLength":MAX_MANIFEST_BYTES},
                    "wasm_base64":{"type":"string","maxLength":MAX_MODULE_BYTES.div_ceil(3)*4}}),
                &["manifest", "wasm_base64"],
            ));
        }
        tools
    }

    fn call(&self, name: &str, mut args: JsonObject) -> Result<Value, String> {
        let mut game = self.game.lock().map_err(|_| "session unavailable")?;
        match name {
            "apply_editor_package" if args.len() == 2 => {
                let manifest = args
                    .get("manifest")
                    .and_then(Value::as_str)
                    .ok_or("expected manifest text")?;
                let module = args
                    .get("wasm_base64")
                    .and_then(Value::as_str)
                    .ok_or("expected base64 Wasm")?;
                game.apply_package(self.role, manifest, module)
            }
            "list_commands" if args.is_empty() => Ok(json!({"commands": match self.role {
                Role::Spectator => json!([]),
                role => wire::commands(matches!(role, Role::Editor(_))),
            }})),
            "observe" if args.is_empty() => game.observe(self.role),
            "advance_turn" if args.is_empty() => game.dispatch(self.role, "turn.end", json!({})),
            "dispatch" if args.len() == 2 => {
                let payload = args
                    .remove("args")
                    .filter(Value::is_object)
                    .ok_or("args must be an object")?;
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("id must be a command ID")?;
                game.dispatch(self.role, id, payload)
            }
            "replay" if args.len() == 1 => {
                let turn = args
                    .get("turn")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("turn must be a u32")?;
                game.replay(self.role, turn)
            }
            _ => Err("unexpected or missing tool arguments".into()),
        }
    }
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new("histeries",env!("CARGO_PKG_VERSION")))
            .with_instructions("Use observe, list_commands and the ruleset resource to plan. End your turn with advance_turn. Stop when observe returns a winner.")
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    async fn call_tool(
        &self,
        req: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !self.tools().iter().any(|t| t.name == req.name) {
            return Err(ErrorData::invalid_params(
                "unknown tool for this credential",
                None,
            ));
        }
        Ok(
            match self.call(&req.name, req.arguments.unwrap_or_default()) {
                Ok(value) => CallToolResult::structured(value),
                Err(reason) => CallToolResult::structured_error(json!({"error":reason})),
            }
            .into(),
        )
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            self.resources()
                .map(|(uri, name, mime, _)| Resource::new(uri, name).with_mime_type(mime))
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        req: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let (uri, _, mime, body) = self
            .resources()
            .find(|r| r.0 == req.uri)
            .ok_or_else(|| ErrorData::resource_not_found("unknown resource", None))?;
        let content = ResourceContents::TextResourceContents {
            uri: uri.into(),
            mime_type: Some(mime.into()),
            text: body.into(),
            meta: None,
        };
        Ok(ReadResourceResult::new(vec![content]).into())
    }
}

type HttpServices = Arc<BTreeMap<String, StreamableHttpService<Handler, LocalSessionManager>>>;

async fn authenticated(State(services): State<HttpServices>, request: Request) -> Response {
    let key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, key)| key);
    let Some(service) = key.and_then(|key| services.get(key)) else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    };
    match service.clone().oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(never) => match never {},
    }
}

fn credentials(seats: &[PlayerId], http: bool) -> Result<BTreeMap<String, Role>, String> {
    let mut map = BTreeMap::new();
    let mut read = |var: String, role, required| {
        if let Ok(token) = env::var(&var) {
            if token.is_empty()
                || !token.bytes().all(|c| c.is_ascii_graphic())
                || map.insert(token, role).is_some()
            {
                return Err(format!(
                    "{var} must be a unique nonempty token without whitespace"
                ));
            }
        } else if required {
            return Err(format!("set {var} to a secret token for this seat"));
        }
        Ok(())
    };
    for &seat in seats {
        read(
            format!("HISTERIES_MCP_SEAT_{}_TOKEN", seat.index()),
            Role::Seat(seat),
            http,
        )?;
    }
    read(
        "HISTERIES_MCP_EDITOR_TOKEN".into(),
        Role::Editor(seats[0]),
        false,
    )?;
    read(
        "HISTERIES_MCP_SPECTATOR_TOKEN".into(),
        Role::Spectator,
        false,
    )?;
    Ok(map)
}

/// Host the supplied game. A loopback listen address selects
/// Streamable HTTP. With stdio, the process credential selects a seat and the other seats use greedy AI.
pub fn serve(session: Session, listen: Option<SocketAddr>) -> Result<(), String> {
    let seats: Vec<_> = session.seat_ids().collect();
    if !(2..=8).contains(&seats.len()) {
        return Err("MCP requires 2 to 8 players".into());
    }
    let tokens = credentials(&seats, listen.is_some())?;
    if let Some(address) = listen {
        if !address.ip().is_loopback() {
            return Err("MCP HTTP listens on a loopback address".into());
        }
        let game = Arc::new(Mutex::new(Game::new(session, vec![])));
        let services = Arc::new(
            tokens
                .into_iter()
                .map(|(token, role)| {
                    let handler = Handler {
                        game: game.clone(),
                        role,
                    };
                    let service = StreamableHttpService::new(
                        move || Ok(handler.clone()),
                        Default::default(),
                        StreamableHttpServerConfig::default()
                            .with_legacy_session_mode(false)
                            .with_json_response(true)
                            .enforce_origin_validation()
                            .with_max_request_body_bytes(64 * 1024),
                    );
                    (token, service)
                })
                .collect(),
        );
        runtime()?.block_on(async move {
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(|e| e.to_string())?;
            eprintln!(
                "Histeries MCP listening at http://{}/mcp",
                listener.local_addr().map_err(|e| e.to_string())?
            );
            axum::serve(
                listener,
                Router::new()
                    .route("/mcp", any(authenticated))
                    .with_state(services),
            )
            .await
            .map_err(|e| e.to_string())
        })
    } else {
        let token = env::var("HISTERIES_MCP_TOKEN")
            .map_err(|_| "set HISTERIES_MCP_TOKEN to a configured credential")?;
        let role = tokens
            .get(&token)
            .copied()
            .ok_or("unknown stdio credential")?;
        let bots = seats
            .iter()
            .copied()
            .filter(|s| role.player().is_ok_and(|p| p != *s))
            .collect();
        let mut game = Game::new(session, bots);
        game.resume(role)?;
        let handler = Handler {
            game: Arc::new(Mutex::new(game)),
            role,
        };
        serve_stdio(handler)
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

fn serve_stdio(handler: Handler) -> Result<(), String> {
    runtime()?.block_on(async move {
        handler
            .serve(stdio())
            .await
            .map_err(|e| e.to_string())?
            .waiting()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Exposes one authenticated seat of an existing LAN match to a stdio MCP host.
pub fn connect(address: SocketAddr, seat: PlayerId) -> Result<(), String> {
    let token = env::var("HISTERIES_LAN_TOKEN")
        .map_err(|_| "set HISTERIES_LAN_TOKEN for the assigned seat")?;
    let client = LanClient::connect(address, seat, token).map_err(seat_lan_error)?;
    serve_stdio(Handler {
        game: Arc::new(Mutex::new(client)),
        role: Role::Seat(seat),
    })
}

#[cfg(test)]
mod package_tests {
    use super::*;
    use base64::prelude::*;
    use histeries_hex::FlatHex;
    use histeries_rules_core::PlayerId;
    use histeries_sim::{MapState, Session};

    #[test]
    fn package_tool_requires_editor_and_accepts_only_inline_bytes() {
        let seat = PlayerId::new(1);
        let map = MapState::blank(FlatHex::new(3, 2, false, false).unwrap());
        let session = Session::with_map([seat, PlayerId::new(2)], map);
        let game = Arc::new(Mutex::new(Game::new(session, vec![])));
        let manifest = include_str!("../../../data/extensions/woodland/manifest.toml");
        let wasm = BASE64_STANDARD.encode(include_bytes!(
            "../../../data/extensions/woodland/plugin.wasm"
        ));
        let args = json!({"manifest":manifest, "wasm_base64":wasm})
            .as_object()
            .unwrap()
            .clone();
        for role in [Role::Seat(seat), Role::Spectator, Role::Editor(seat)] {
            let handler = Handler {
                role,
                game: game.clone(),
            };
            let authorized = matches!(role, Role::Editor(_));
            assert_eq!(
                handler
                    .resources()
                    .any(|resource| resource.0 == EDITOR_PACKAGE_DOCS.0),
                authorized
            );
            assert_eq!(
                handler
                    .tools()
                    .iter()
                    .any(|tool| tool.name == "apply_editor_package"),
                authorized
            );
            let result = handler.call("apply_editor_package", args.clone());
            if authorized {
                assert_eq!(result.unwrap(), json!({"id":"woodland","commands":1}));
                let paths = json!({"path":"manifest.toml","module":"plugin.wasm"});
                assert!(
                    handler
                        .call("apply_editor_package", paths.as_object().unwrap().clone())
                        .is_err()
                );
                let large = json!({"manifest":manifest,"wasm_base64":"A".repeat(90000)});
                assert!(
                    handler
                        .call("apply_editor_package", large.as_object().unwrap().clone())
                        .is_err()
                );
            } else {
                assert!(result.unwrap_err().contains("credential"));
            }
        }
    }
}
