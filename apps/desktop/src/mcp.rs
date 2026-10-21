//! `garmin-companion mcp [--db PATH] [--focus A..B]`: the read-only coach tools
//! as an MCP server on stdio. Claude Code starts it (see `claude_code.rs`).
//!
//! Stdout is the transport, so nothing else may print there. The loop is
//! single-threaded on purpose: `garmin_core::with_as_of` is thread-local.
//! The tool code is the same as the cloud coach uses, with drafts off.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use garmin_coach::{external, Period};
use garmin_core::db::Db;
use serde_json::{json, Value};

/// What a request needs: which cache to read, and the period that bounds it.
pub struct Server {
    db: Option<PathBuf>,
    period: Option<Period>,
}

impl Server {
    /// The cache is the default one unless `db` names a copy.
    pub fn new(db: Option<PathBuf>, period: Option<Period>) -> Self {
        Self { db, period }
    }

    fn open(&self) -> Result<Db> {
        match &self.db {
            Some(path) => Db::open(path),
            None => Db::open_default(),
        }
    }

    /// Answer one JSON-RPC line. A notification gets no answer.
    pub fn handle(&self, line: &str) -> Option<Value> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        let id = msg.get("id")?.clone();
        let params = &msg["params"];
        Some(match msg["method"].as_str().unwrap_or_default() {
            "initialize" => reply(id, initialize(params)),
            "ping" => reply(id, json!({})),
            "tools/list" => reply(id, json!({ "tools": external::mcp_tools() })),
            "tools/call" => reply(id, self.call(params)),
            other => error(id, -32601, &format!("method not found: {other}")),
        })
    }

    fn call(&self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default();
        let open = || self.open();
        let (text, bad) = external::call_tool(name, &params["arguments"], &open, self.period);
        json!({ "content": [{ "type": "text", "text": text }], "isError": bad })
    }
}

fn initialize(params: &Value) -> Value {
    // Echo the client's version: the tool set uses no feature of a newer one.
    let version = params["protocolVersion"].as_str().unwrap_or("2024-11-05");
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "garmin", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Read requests from stdin until it closes.
pub fn serve(server: &Server) -> Result<()> {
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.handle(&line) {
            writeln!(out, "{answer}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Entry point for the `mcp` subcommand. `args` are the words after `mcp`.
pub fn run(args: &[String]) -> Result<()> {
    let db = crate::required(args, "--db")?.map(PathBuf::from);
    let period = crate::required(args, "--focus")?
        .map(|f| crate::parse_focus(&f))
        .transpose()?
        .map(|f| Period {
            start: f.start,
            end: f.end,
        });
    serve(&Server::new(db, period))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache with one wellness row, in a temp file.
    fn server(tag: &str) -> (Server, PathBuf) {
        let path = std::env::temp_dir().join(format!("gc-mcp-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        db.upsert_daily(&garmin_core::db::DailyMetrics {
            date: "2026-03-05".into(),
            resting_hr: Some(48.0),
            ..Default::default()
        })
        .unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let period = Period {
            start: day,
            end: day,
        };
        (Server::new(Some(path.clone()), Some(period)), path)
    }

    fn ask(s: &Server, req: Value) -> Value {
        s.handle(&req.to_string()).expect("an answer")
    }

    #[test]
    fn the_server_answers_the_handshake_the_list_and_a_call() {
        let (s, path) = server("rpc");
        let init = ask(
            &s,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "garmin");
        assert!(s
            .handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none());
        assert_eq!(
            ask(&s, json!({"id":2,"method":"ping"}))["result"],
            json!({})
        );

        let list = ask(&s, json!({"id":3,"method":"tools/list"}));
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"recovery") && !names.contains(&"draft_workout"));

        let call = ask(
            &s,
            json!({"id":4,"method":"tools/call","params":{"name":"recovery","arguments":{}}}),
        );
        assert_eq!(call["result"]["isError"], false);
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("2026-03-05") && text.contains("48"), "{text}");

        let bad = ask(
            &s,
            json!({"id":5,"method":"tools/call","params":{"name":"draft_workout"}}),
        );
        assert_eq!(bad["result"]["isError"], true);
        assert_eq!(
            ask(&s, json!({"id":6,"method":"nope"}))["error"]["code"],
            -32601
        );
        let _ = std::fs::remove_file(&path);
    }
}
