use serde_json::Value;

use super::constants::MAX_SUMMARY_CHARS;
use crate::tools::names as tool_names;

/// Short, bounded summary of a tool call's target for transcript rendering.
/// Never includes secret material and is capped so a large argument cannot
/// inflate a transcript line.
pub fn tool_call_summary(name: &str, arguments: &Value) -> Option<String> {
    let raw = match name {
        tool_names::BASH => arguments.get("command")?.as_str()?.to_owned(),
        tool_names::TMUX => arguments.get("args")?.as_str()?.to_owned(),
        tool_names::WORKSPACE => {
            let op = arguments.get("op").and_then(Value::as_str)?;
            let path = arguments.get("path").and_then(Value::as_str)?;
            format!("{op} {path}")
        }
        tool_names::JOURNAL => {
            let start = arguments.get("start").and_then(Value::as_i64)?;
            let end = arguments.get("end").and_then(Value::as_i64)?;
            format!("{start}..{end}")
        }
        tool_names::TEAM | tool_names::REPORT => {
            arguments.get("op").and_then(Value::as_str)?.to_owned()
        }
        _ => return None,
    };
    let summary = raw.trim();
    if summary.is_empty() {
        return None;
    }
    let condensed = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate_chars(&condensed, MAX_SUMMARY_CHARS))
}

pub fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let truncated = value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}
