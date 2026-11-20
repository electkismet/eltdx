#[path = "mcp/f10.rs"]
mod f10;
#[path = "mcp/hq.rs"]
mod hq;
#[path = "mcp/tools.rs"]
mod tools;

use std::error::Error;
use std::io::{self, BufRead, Write};
use std::time::Duration;

use eltdx::{OperationOptions, RawPolicy};
use serde_json::{json, Value};

type ToolResult = Result<Value, Box<dyn Error>>;

const DOCS: &[(&str, &str, &str)] = &[
    (
        "overview",
        "overview",
        include_str!("../../../../docs/index.md"),
    ),
    (
        "API",
        "api",
        include_str!("../../../../docs/API_REFERENCE.md"),
    ),
    (
        "methods",
        "methods",
        include_str!("../../../../docs/METHOD_REFERENCE.md"),
    ),
    (
        "fields",
        "fields",
        include_str!("../../../../docs/FIELD_REFERENCE.md"),
    ),
    (
        "7709_commands",
        "commands",
        include_str!("../../../../docs/COMMANDS_7709.md"),
    ),
    ("F10", "f10", include_str!("../../../../docs/F10_7615.md")),
    (
        "helpers",
        "helpers",
        include_str!("../../../../docs/helpers/README.md"),
    ),
    ("MCP", "mcp", include_str!("../../../../docs/MCP.md")),
];

fn main() -> io::Result<()> {
    let mut clients = hq::Clients::default();
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => process(&request, &mut clients),
            Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            writeln!(output)?;
            output.flush()?;
        }
    }
    Ok(())
}

fn process(request: &Value, clients: &mut hq::Clients) -> Option<Value> {
    let id = request.get("id");
    let method = request["method"].as_str();
    if request["jsonrpc"] != "2.0"
        || method.is_none()
        || id.is_some_and(|id| !id.is_string() && !id.is_number())
    {
        return Some(rpc_error(
            id.cloned().unwrap_or_default(),
            -32600,
            "Invalid request",
        ));
    }
    let id = id?.clone();
    let params = &request["params"];
    let result = match method? {
        "initialize" => {
            let version = match params["protocolVersion"].as_str() {
                Some(version @ ("2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25")) => {
                    version
                }
                Some(_) => "2025-11-25",
                None => return Some(rpc_error(id, -32602, "Expected protocolVersion")),
            };
            json!({
                "protocolVersion": version,
                "serverInfo": {
                    "name": "eltdx",
                    "title": "eltdx A-share data",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {"tools": {}, "resources": {}},
                "instructions": "Use full stock codes such as sz000001 or sh600000. Prefer bounded page tools and request only the data needed."
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools::schemas()}),
        "tools/call" => {
            let name = params["name"]
                .as_str()
                .and_then(|name| name.strip_prefix("eltdx_"));
            let Some(tool) = name.and_then(tools::lookup) else {
                return Some(rpc_error(id, -32602, "Unknown tool"));
            };
            let empty = json!({});
            let args = params.get("arguments").unwrap_or(&empty);
            let result = tools::validate(tool, args).and_then(|()| call(tool, args, clients));
            match result {
                Ok(value) => json!({"content": [{"type": "text", "text": format!("{value}")}]}),
                Err(error) => {
                    json!({"content": [{"type": "text", "text": format!("{error}")}], "isError": true})
                }
            }
        }
        "resources/list" => json!({"resources": DOCS.iter().map(|(_, name, _)| json!({
            "uri": format!("eltdx://docs/{name}"),
            "name": format!("eltdx_docs_{name}"),
            "title": format!("eltdx {name} documentation"),
            "description": format!("Bundled eltdx {name} documentation."),
            "mimeType": "text/markdown",
        })).collect::<Vec<_>>()}),
        "resources/templates/list" => json!({"resourceTemplates": []}),
        "resources/read" => {
            let uri = params["uri"].as_str().unwrap_or_default();
            let name = uri.strip_prefix("eltdx://docs/");
            let Some((_, _, text)) = DOCS.iter().find(|(_, key, _)| name == Some(*key)) else {
                return Some(rpc_error(id, -32002, "Resource not found"));
            };
            json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]})
        }
        _ => return Some(rpc_error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn call(tool: &tools::Tool, args: &Value, clients: &mut hq::Clients) -> ToolResult {
    if tool.name == "docs_index" {
        return Ok(DOCS
            .iter()
            .map(|(label, name, _)| ((*label).to_owned(), json!(format!("eltdx://docs/{name}"))))
            .collect::<serde_json::Map<_, _>>()
            .into());
    }
    let default_timeout = if tool.name.starts_with("macro_") {
        10.0
    } else {
        8.0
    };
    let timeout = args["timeout"].as_f64().unwrap_or(default_timeout);
    if !timeout.is_finite() || !(0.0..=120.0).contains(&timeout) || timeout == 0.0 {
        return Err("timeout must be greater than 0 and no more than 120 seconds".into());
    }
    let mut options = OperationOptions::default().with_timeout(Duration::from_secs_f64(timeout));
    if args["include_raw"] == true {
        options = options.with_raw(RawPolicy::RetainPayload);
    }
    if tool.hq {
        clients.call(tool.name, args, &options)
    } else {
        f10::call(tool.name, args, &options)
    }
}
