//! What a host needs to run the coach tools outside of a turn: an MCP server,
//! and a model that is a separate program. Every function here is read-only.
//! The tool set has no draft tool, and a call to it is refused.

use anyhow::Result;
use garmin_core::db::Db;
use serde_json::{json, Value};

use crate::prompt::{grounding_message, system_text, trim_history};
use crate::tools::{self, Period};
use crate::turn::HistoryMessage;

/// The read-only tools as MCP tool objects: `name`, `description`, `inputSchema`.
pub fn mcp_tools() -> Vec<Value> {
    let list = tools::schemas(false);
    list.as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let f = &t["function"];
            json!({
                "name": f["name"],
                "description": f["description"],
                "inputSchema": f["parameters"],
            })
        })
        .collect()
}

/// Run one tool. The result is `(text, is_error)`. A name that is not in
/// [`mcp_tools`] is an error, so a draft or a write never runs.
pub fn call_tool(
    name: &str,
    args: &Value,
    open_db: &dyn Fn() -> Result<Db>,
    period: Option<Period>,
) -> (String, bool) {
    let known = mcp_tools().iter().any(|t| t["name"] == name);
    let out = if known {
        tools::run(name, args, open_db, period)
    } else {
        json!({ "error": format!("unknown tool {name}") })
    };
    let bad = out.get("error").is_some();
    (out.to_string(), bad)
}

/// The label that the person sees for a tool call.
pub fn tool_label(name: &str, args: &Value) -> String {
    tools::describe(name, args)
}

/// The system prompt for a model that reads the tools over MCP: the coach
/// prompt, then today's grounding.
pub fn mcp_system_prompt(db: &Db, period: Option<Period>) -> Result<String> {
    let grounding = grounding_message(db, period)?;
    Ok(format!(
        "{}\n\n{}\n\nYour tools are MCP tools of the server \"garmin\". Call them to read the data.",
        system_text(false),
        grounding["content"].as_str().unwrap_or_default(),
    ))
}

/// The history as one plain transcript. The newest user message comes last.
pub fn transcript(history: Vec<HistoryMessage>) -> String {
    trim_history(history)
        .iter()
        .map(|m| {
            let who = if m.role == "user" { "User" } else { "Coach" };
            format!("{who}: {}", m.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_are_read_only() {
        let tools = mcp_tools();
        assert!(tools.len() > 10);
        for t in &tools {
            assert!(t["inputSchema"]["type"] == "object", "{t}");
            assert!(!t["description"].as_str().unwrap().is_empty());
        }
        let db = || Db::open(std::path::Path::new(":memory:"));
        let (text, bad) = call_tool(tools::DRAFT_TOOL, &json!({}), &db, None);
        assert!(bad && text.contains("unknown tool"));
        let (_, bad) = call_tool("sync", &json!({}), &db, None);
        assert!(bad);
    }

    #[test]
    fn a_transcript_ends_with_the_newest_user_message() {
        let msg = |role: &str, content: &str| HistoryMessage {
            role: role.into(),
            content: content.into(),
        };
        let text = transcript(vec![
            msg("user", "Hi"),
            msg("assistant", "Hello"),
            msg("user", "Sleep?"),
        ]);
        assert_eq!(text, "User: Hi\n\nCoach: Hello\n\nUser: Sleep?");
    }
}
