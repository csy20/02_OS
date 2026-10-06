use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::{call_tool, list_tools};
use agent_core::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub struct McpServer {
    pub repo_root: PathBuf,
}

impl McpServer {
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    /// Validate a JSON-RPC envelope and dispatch it.
    ///
    /// A message with no `id` member is a notification and produces no response,
    /// including unknown methods and bad versions. Parse errors are handled by
    /// `run_stdio` before this is called.
    pub fn handle_message(&self, value: Value) -> Option<JsonRpcResponse> {
        let Some(obj) = value.as_object() else {
            return Some(invalid_request(Value::Null));
        };
        if !obj.contains_key("id") {
            return None;
        }
        let id_val = obj.get("id").cloned().unwrap_or(Value::Null);
        let id_ok = id_val.is_null() || id_val.is_string() || id_val.is_number();
        let response_id = if id_ok { id_val.clone() } else { Value::Null };
        let jsonrpc_ok = obj.get("jsonrpc").and_then(|v| v.as_str()) == Some("2.0");
        let method = obj.get("method").and_then(|m| m.as_str());
        let params_ok = match obj.get("params") {
            None => true,
            Some(params) if params.is_array() || params.is_object() => true,
            Some(_) => false,
        };
        if !jsonrpc_ok || !id_ok || !params_ok || method.map(|m| m.is_empty()).unwrap_or(true) {
            return Some(invalid_request(response_id));
        }
        self.handle_request(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(id_val),
            method: method.unwrap().to_string(),
            params: obj.get("params").cloned(),
        })
    }

    /// Process a single incoming JSON-RPC request and return a response if needed.
    pub fn handle_request(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = req.id.clone()?;
        if req.jsonrpc != "2.0" || req.method.is_empty() {
            return Some(invalid_request(id));
        }

        match req.method.as_str() {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "02-agent-runtime",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                Some(JsonRpcResponse::success(id, result))
            }

            "notifications/initialized" => None,

            "ping" => Some(JsonRpcResponse::success(id, json!({}))),

            "tools/list" => {
                let tools = list_tools();
                Some(JsonRpcResponse::success(id, json!({ "tools": tools })))
            }

            "tools/call" => {
                let params = req.params.unwrap_or(json!({}));
                let tool_name = params["name"].as_str().unwrap_or("");
                let arguments = &params["arguments"];

                let call_result = call_tool(&self.repo_root, tool_name, arguments);
                let result_value = serde_json::to_value(&call_result).unwrap_or(json!({
                    "content": [{"type": "text", "text": "Serialization error"}],
                    "isError": true
                }));

                Some(JsonRpcResponse::success(id, result_value))
            }

            unknown => Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method '{}' not found", unknown),
            )),
        }
    }

    /// Run stdio loop processing JSON-RPC messages line by line.
    pub fn run_stdio<R: BufRead, W: Write>(&self, reader: R, mut writer: W) -> Result<()> {
        for line in reader.lines() {
            let line_content = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let trimmed = line_content.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(err) => {
                    let err_resp =
                        JsonRpcResponse::error(json!(null), -32700, format!("Parse error: {err}"));
                    let _ = writeln!(writer, "{}", serde_json::to_string(&err_resp)?);
                    let _ = writer.flush();
                    continue;
                }
            };

            if let Some(resp) = self.handle_message(value) {
                let resp_str = serde_json::to_string(&resp)?;
                writeln!(writer, "{}", resp_str)?;
                writer.flush()?;
            }
        }

        Ok(())
    }
}

fn invalid_request(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::error(id, -32600, "Invalid Request".to_string())
}
