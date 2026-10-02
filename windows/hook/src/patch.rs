//! What the relay keeps of a tool's result: for an edit, the patch and nothing
//! else.
//!
//! Claude Code's PostToolUse carries `tool_response`, which for Edit and Write
//! includes the whole file before the change (`originalFile`) and for Read the
//! whole file read. None of that is forwarded. For Edit and Write only the
//! changed lines with a little context (`structuredPatch`, the hunks Claude Code
//! itself computed, with their line numbers) and whether Write created the file
//! go to the island, capped. Shape checked against Claude Code 2.1.287.

use serde_json::{json, Map, Value};

/// Hunks forwarded at most.
const MAX_HUNKS: usize = 10;
/// Patch lines forwarded at most, all hunks together.
const MAX_LINES: usize = 120;

/// The part of `response` worth forwarding for `tool`, or `None` to forward
/// nothing at all.
pub fn slim(tool: &str, response: &Value) -> Option<Value> {
    if tool != "Edit" && tool != "Write" {
        return None;
    }
    let response = response.as_object()?;
    let hunks = response.get("structuredPatch")?.as_array()?;

    let mut kept = Vec::new();
    let mut room = MAX_LINES;
    let mut truncated = hunks.len() > MAX_HUNKS;
    for hunk in hunks.iter().take(MAX_HUNKS) {
        if room == 0 {
            truncated = true;
            break;
        }
        let number = |key: &str| hunk.get(key).and_then(Value::as_u64).unwrap_or(0);
        let all: Vec<Value> = hunk
            .get("lines")
            .and_then(Value::as_array)
            .map(|lines| lines.iter().filter(|l| l.is_string()).cloned().collect())
            .unwrap_or_default();
        if all.len() > room {
            truncated = true;
        }
        let lines: Vec<Value> = all.into_iter().take(room).collect();
        room -= lines.len();
        kept.push(json!({
            "oldStart": number("oldStart"),
            "oldLines": number("oldLines"),
            "newStart": number("newStart"),
            "newLines": number("newLines"),
            "lines": lines,
        }));
    }

    let mut out = Map::new();
    if let Some(kind) = response.get("type").and_then(Value::as_str).filter(|k| matches!(*k, "create" | "update")) {
        out.insert("type".into(), Value::String(kind.into()));
    }
    out.insert("structuredPatch".into(), Value::Array(kept));
    if truncated {
        out.insert("truncated".into(), Value::Bool(true));
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PostToolUse Edit result as Claude Code 2.1.287 sends it.
    fn edit_response() -> Value {
        json!({
            "filePath": "C:\\work\\shop\\invoice.ts",
            "oldString": "const TVA = 0.196",
            "newString": "const TVA = 0.2",
            "originalFile": "// the whole file\nconst TVA = 0.196\n",
            "structuredPatch": [{
                "oldStart": 1, "oldLines": 3, "newStart": 1, "newLines": 3,
                "lines": [" import { Item } from './types'", "-const TVA = 0.196", "+const TVA = 0.2"]
            }],
            "userModified": false,
            "replaceAll": false
        })
    }

    #[test]
    fn an_edit_keeps_only_its_hunks() {
        let kept = slim("Edit", &edit_response()).expect("an edit is forwarded");
        assert_eq!(kept, json!({
            "structuredPatch": [{
                "oldStart": 1, "oldLines": 3, "newStart": 1, "newLines": 3,
                "lines": [" import { Item } from './types'", "-const TVA = 0.196", "+const TVA = 0.2"]
            }]
        }));
    }

    #[test]
    fn a_write_says_whether_it_created_the_file() {
        let created = json!({ "type": "create", "filePath": "x", "content": "alpha\nbeta", "structuredPatch": [], "originalFile": null });
        assert_eq!(slim("Write", &created), Some(json!({ "type": "create", "structuredPatch": [] })));
        let updated = json!({ "type": "update", "content": "gamma", "originalFile": "alpha\nbeta",
            "structuredPatch": [{ "oldStart": 1, "oldLines": 2, "newStart": 1, "newLines": 1, "lines": ["-alpha", "-beta", "+gamma"] }] });
        assert_eq!(slim("Write", &updated), Some(json!({ "type": "update",
            "structuredPatch": [{ "oldStart": 1, "oldLines": 2, "newStart": 1, "newLines": 1, "lines": ["-alpha", "-beta", "+gamma"] }] })));
    }

    #[test]
    fn nothing_of_any_other_tool_is_forwarded() {
        assert_eq!(slim("Read", &json!({ "type": "text", "file": { "content": "secret" } })), None);
        assert_eq!(slim("Bash", &json!({ "stdout": "out", "structuredPatch": [] })), None, "a patch only counts for an edit");
        assert_eq!(slim("Edit", &json!("not an object")), None);
        assert_eq!(slim("Edit", &json!({ "originalFile": "x" })), None, "no patch, nothing to show");
    }

    #[test]
    fn only_numbers_and_lines_survive_in_a_hunk() {
        let odd = json!({ "structuredPatch": [{
            "oldStart": 4, "oldLines": "four", "newStart": 4, "newLines": 1,
            "lines": ["+ok", 7, { "x": 1 }], "extra": "dropped"
        }]});
        assert_eq!(slim("Edit", &odd), Some(json!({ "structuredPatch": [{
            "oldStart": 4, "oldLines": 0, "newStart": 4, "newLines": 1, "lines": ["+ok"]
        }]})));
        let bad_type = json!({ "type": "../../x", "structuredPatch": [] });
        assert_eq!(slim("Write", &bad_type), Some(json!({ "structuredPatch": [] })), "only create or update");
    }

    #[test]
    fn a_long_patch_is_capped_and_says_so() {
        let lines: Vec<Value> = (0..200).map(|i| Value::String(format!("+line {i}"))).collect();
        let hunk = json!({ "oldStart": 1, "oldLines": 0, "newStart": 1, "newLines": 200, "lines": lines });
        let many: Vec<Value> = (0..30).map(|_| hunk.clone()).collect();
        let kept = slim("Edit", &json!({ "structuredPatch": many })).unwrap();
        let hunks = kept["structuredPatch"].as_array().unwrap();
        let total: usize = hunks.iter().map(|h| h["lines"].as_array().unwrap().len()).sum();
        assert!(hunks.len() <= MAX_HUNKS);
        assert_eq!(total, MAX_LINES);
        assert_eq!(kept["truncated"], json!(true));
    }
}
