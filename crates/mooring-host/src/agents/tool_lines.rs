//! A tool call read back from the raw log lines the index points at: what was asked and what
//! came back, whole, as each CLI writes them. Images and other binary content stay out: the
//! caller is told how many and how large.
use super::codex_script::{script_steps, Step};
use crate::content::{input, patch_lines, patch_text};
use serde_json::Value;

pub struct ToolCallLine {
    pub name: String,
    pub args: Value,
}

#[derive(Default, Debug, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    /// Images it returned, and their encoded bytes: named to the reader, never sent.
    pub images: u32,
    pub image_bytes: u64,
    pub error: bool,
    /// The exit code the line itself states.
    pub code: Option<i32>,
    pub took_ms: Option<u64>,
}
impl ToolOutput {
    /// Bytes it logged, images included.
    pub fn size(&self) -> u64 {
        self.text.len() as u64 + self.image_bytes
    }
}

/// Adds a list of content blocks: text in order, images counted.
fn add_blocks(out: &mut ToolOutput, blocks: &[Value]) {
    for b in blocks {
        match b["type"].as_str().unwrap_or("") {
            "image" | "input_image" | "image_url" => {
                out.images += 1;
                let data = (b["source"]["data"].as_str())
                    .or_else(|| b["image_url"].as_str())
                    .or_else(|| b["image_url"]["url"].as_str());
                out.image_bytes += data.map_or(0, |d| d.len() as u64);
            }
            _ => {
                let text = b["text"].as_str().or_else(|| b["tool_name"].as_str());
                if let Some(text) = text.or_else(|| b.as_str()) {
                    if !out.text.is_empty() {
                        out.text.push('\n');
                    }
                    out.text.push_str(text);
                }
            }
        }
    }
}

pub(super) fn claude_call(line: &Value, id: &str) -> Option<ToolCallLine> {
    let blocks = line["message"]["content"].as_array()?;
    let b = blocks
        .iter()
        .find(|b| b["type"] == "tool_use" && b["id"] == id)?;
    Some(ToolCallLine {
        name: b["name"].as_str()?.to_owned(),
        args: b["input"].clone(),
    })
}

pub(super) fn claude_result(line: &Value, id: &str) -> Option<ToolOutput> {
    let blocks = line["message"]["content"].as_array()?;
    let b = blocks
        .iter()
        .find(|b| b["type"] == "tool_result" && b["tool_use_id"] == id)?;
    let mut out = ToolOutput {
        error: b["is_error"] == true,
        ..Default::default()
    };
    match &b["content"] {
        Value::String(s) => out.text.clone_from(s),
        Value::Array(blocks) => add_blocks(&mut out, blocks),
        _ => (),
    }
    // Claude Code words a command's failure as a line of its output.
    if out.error {
        let lines = [out.text.lines().next(), out.text.lines().last()];
        out.code = lines
            .into_iter()
            .flatten()
            .find_map(|l| l.strip_prefix("Exit code ")?.trim().parse().ok());
    }
    Some(out)
}

/// The item a Codex line carries: a response item, or an event's finished item.
fn codex_item(line: &Value) -> &Value {
    let p = &line["payload"];
    if line["type"] == "response_item" {
        p
    } else {
        line.get("item").or_else(|| p.get("item")).unwrap_or(p)
    }
}

pub(super) fn codex_call(line: &Value, id: &str) -> Option<ToolCallLine> {
    let item = codex_item(line);
    match item["type"].as_str()? {
        "function_call" | "custom_tool_call" => Some(ToolCallLine {
            name: item["name"].as_str()?.to_owned(),
            args: input(item.get("arguments").unwrap_or(&item["input"])),
        }),
        "command_execution" | "CommandExecution" if item["id"] == id || item["call_id"] == id => {
            Some(ToolCallLine {
                name: "exec_command".into(),
                args: serde_json::json!({ "cmd": item["command"], "cwd": item["cwd"] }),
            })
        }
        _ => None,
    }
}

/// A Codex line holds one item, and a command's later reads (`write_stdin`) are logged under
/// their own call: a result row's line is its output whatever id it names.
pub(super) fn codex_result(line: &Value) -> Option<ToolOutput> {
    let item = codex_item(line);
    let mut out = ToolOutput::default();
    let body = match item["type"].as_str()? {
        "function_call_output" | "custom_tool_call_output" => &item["output"],
        "command_execution" | "CommandExecution" => {
            let secs = item["duration"]["secs"].as_u64();
            let nanos = item["duration"]["nanos"].as_u64().unwrap_or(0);
            out.took_ms = secs.map(|s| s * 1000 + nanos / 1_000_000);
            item.get("output").unwrap_or(&item["aggregated_output"])
        }
        _ => return None,
    };
    out.code = item["exit_code"].as_i64().map(|c| c as i32);
    match body {
        Value::String(s) => match input(body) {
            // Old logs wrap a function's output: `{"output":"…","metadata":{"exit_code":0}}`.
            v if v["output"].is_string() => {
                out.text = v["output"].as_str().unwrap_or("").to_owned();
                out.code = out
                    .code
                    .or(v["metadata"]["exit_code"].as_i64().map(|c| c as i32));
            }
            _ => out.text.clone_from(s),
        },
        Value::Array(blocks) => add_blocks(&mut out, blocks),
        _ => (),
    }
    Some(out)
}

/// One call in words: for its row, and how to show its input.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Described {
    /// `command`, `edit`, `read`, `search`, `web`, `agent` or `other`.
    pub kind: &'static str,
    pub label: String,
    /// The input as it should read, whole.
    pub input: String,
    /// `command`, `diff`, `code`, `text` or `json`.
    pub input_kind: &'static str,
    /// A script's steps, with what could not be read.
    pub steps: Vec<(String, String, bool)>,
    pub note: Option<String>,
    /// A script doing nothing but run commands: each has a row of its own in the log.
    pub only_commands: bool,
}

const LABEL_CHARS: usize = 200;

/// The first line with anything on it, whitespace closed up, and short. Secrets go before the
/// cut: a token cut short no longer looks like one.
fn one_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let squeezed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let squeezed = crate::redact::redact(&squeezed);
    let more = text.trim().lines().count() > 1;
    let mut chars = squeezed.chars();
    let mut out: String = chars.by_ref().take(LABEL_CHARS).collect();
    if more || chars.next().is_some() {
        out.push_str(" …");
    }
    out
}

/// A command as a string: argv's shell wrapper (`zsh -lc "ls"`) says nothing.
fn command_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => {
            let argv: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            let wrapped =
                matches!(argv.len(), 3) && argv[1].ends_with('c') && argv[1].starts_with('-');
            Some(if wrapped {
                argv[2].to_owned()
            } else {
                argv.join(" ")
            })
        }
        _ => None,
    }
}

fn first_str<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| args[*k].as_str())
        .filter(|s| !s.is_empty())
}

fn pretty(args: &Value) -> String {
    serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string())
}

fn diff_of(args: &Value) -> String {
    let mut out = String::new();
    let mut one = |old: &str, new: &str| {
        for l in old.lines() {
            out += &format!("- {l}\n");
        }
        for l in new.lines() {
            out += &format!("+ {l}\n");
        }
    };
    match args["edits"].as_array() {
        Some(edits) => {
            for e in edits {
                one(
                    e["old_string"].as_str().unwrap_or(""),
                    e["new_string"].as_str().unwrap_or(""),
                );
            }
        }
        None => one(
            args["old_string"].as_str().unwrap_or(""),
            args["new_string"].as_str().unwrap_or(""),
        ),
    }
    out
}

fn step_words(s: &Step) -> (String, String, bool) {
    let tool = s.tool.clone();
    let Some(text) = &s.literal else {
        let label = match s.tool.as_str() {
            "exec_command" => "运行时才拼出的命令，读不到内容".to_owned(),
            "apply_patch" => "运行时才拼出的补丁，读不到内容".to_owned(),
            other => friendly(other),
        };
        let unread = matches!(s.tool.as_str(), "exec_command" | "apply_patch");
        return (tool, label, unread);
    };
    let label = match s.tool.as_str() {
        "exec_command" => one_line(text),
        "apply_patch" => {
            let files: Vec<String> = patch_lines(text)
                .into_iter()
                .map(|(path, ..)| path.rsplit('/').next().unwrap_or("").to_owned())
                .collect();
            format!("改 {}", files.join("、"))
        }
        "write_stdin" => "往运行中的命令里输入".to_owned(),
        "view_image" => format!("看图 {}", one_line(text)),
        "web__run" => format!("查网页 {}", one_line(text)),
        other => format!("{} {}", friendly(other), one_line(text)),
    };
    (tool, label, false)
}

/// `mcp__server__tool` as `server · tool`, other names as they are.
fn friendly(name: &str) -> String {
    match name.strip_prefix("mcp__") {
        Some(rest) => rest.replacen("__", " · ", 1),
        None => name.to_owned(),
    }
}

/// How a call reads, from its name and arguments. Works on the index's cut-down arguments
/// too, where the label's field survives; `input` is whole only from the raw line.
pub fn describe_tool(name: &str, args: &Value) -> Described {
    let short = name.rsplit(['.', ':']).next().unwrap_or(name);
    let mut d = Described {
        kind: "other",
        input: pretty(args),
        input_kind: "json",
        ..Default::default()
    };
    let path = first_str(args, &["file_path", "notebook_path", "path"]).unwrap_or("");
    match short {
        "Bash" | "shell" | "local_shell" | "exec_command" => {
            let cmd = (args.get("command").or_else(|| args.get("cmd")))
                .and_then(command_text)
                .unwrap_or_default();
            d.kind = "command";
            d.label = one_line(&cmd);
            d.input = cmd;
            d.input_kind = "command";
        }
        "write_stdin" => {
            d.kind = "command";
            d.label = "往运行中的命令里输入".into();
        }
        "Read" | "NotebookRead" | "view_image" => {
            d.kind = "read";
            d.label = path.to_owned();
        }
        "Edit" | "MultiEdit" => {
            d.kind = "edit";
            d.label = path.to_owned();
            if args.get("old_string").is_some() || args.get("edits").is_some() {
                d.input = diff_of(args);
                d.input_kind = "diff";
            }
        }
        "Write" | "NotebookEdit" => {
            d.kind = "edit";
            d.label = path.to_owned();
            if let Some(body) = first_str(args, &["content", "new_source"]) {
                d.input = body.to_owned();
                d.input_kind = "text";
            }
        }
        "apply_patch" => {
            let patch = patch_text(args);
            let files: Vec<String> = patch_lines(patch).into_iter().map(|f| f.0).collect();
            d.kind = "edit";
            d.label = files.join("、");
            if !patch.is_empty() {
                d.input = patch.to_owned();
                d.input_kind = "diff";
            }
        }
        "Grep" | "Glob" | "ToolSearch" => {
            d.kind = "search";
            d.label = first_str(args, &["pattern", "query"])
                .unwrap_or("")
                .to_owned();
            if !path.is_empty() && !d.label.is_empty() {
                d.label = format!("{} · {path}", d.label);
            }
        }
        "WebFetch" | "WebSearch" => {
            d.kind = "web";
            d.label = first_str(args, &["url", "query"]).unwrap_or("").to_owned();
        }
        "Agent" | "Task" | "SendMessage" => {
            d.kind = "agent";
            d.label = first_str(args, &["description", "subagent_type", "to"])
                .unwrap_or("")
                .to_owned();
            if let Some(prompt) = first_str(args, &["prompt", "message"]) {
                d.input = prompt.to_owned();
                d.input_kind = "text";
            }
        }
        "exec" => {
            let script = args["input"].as_str().unwrap_or("");
            let steps = script_steps(script);
            let unread = steps
                .iter()
                .filter(|s| {
                    s.literal.is_none() && matches!(s.tool.as_str(), "exec_command" | "apply_patch")
                })
                .count();
            d.only_commands = !steps.is_empty() && steps.iter().all(|s| s.tool == "exec_command");
            d.steps = steps.iter().map(step_words).collect();
            let names: Vec<&str> = d.steps.iter().take(3).map(|s| s.1.as_str()).collect();
            d.label = if steps.is_empty() {
                "脚本（没有调用工具）".into()
            } else if steps.len() > 3 {
                format!("{}；等 {} 步", names.join("；"), steps.len())
            } else {
                names.join("；")
            };
            d.kind = if !steps.is_empty() && steps.iter().all(|s| s.tool == "apply_patch") {
                "edit"
            } else {
                "command"
            };
            d.input = script.to_owned();
            d.input_kind = "code";
            if unread > 0 {
                d.note = Some(format!(
                    "脚本里有 {unread} 步是运行时才拼出来的，只能看脚本原文"
                ));
            }
        }
        "js" => {
            d.label = first_str(args, &["title"]).unwrap_or("脚本").to_owned();
            d.input = first_str(args, &["code"]).unwrap_or("").to_owned();
            d.input_kind = "code";
        }
        _ => {
            d.label = [
                "description",
                "title",
                "question",
                "query",
                "pattern",
                "url",
                "name",
                "skill",
                "path",
                "file_path",
                "command",
                "cmd",
            ]
            .iter()
            .find_map(|k| args[*k].as_str())
            .unwrap_or("")
            .to_owned();
        }
    }
    d.label = one_line(&d.label);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_blocks_are_found_by_id_with_images_named_not_kept() {
        let line = json!({"type":"user","message":{"content":[
            {"type":"tool_result","tool_use_id":"a","content":"other"},
            {"type":"tool_result","tool_use_id":"b","is_error":true,"content":[
                {"type":"text","text":"one"},
                {"type":"image","source":{"data":"QUJD"}},
                {"type":"text","text":"two"}]},
        ]}});
        let out = claude_result(&line, "b").unwrap();
        assert_eq!(out.text, "one\ntwo");
        assert_eq!((out.images, out.image_bytes, out.error), (1, 4, true));
        assert_eq!(claude_result(&line, "a").unwrap().text, "other");
        assert!(claude_result(&line, "c").is_none());
        let exit = |text: &str| {
            let line = json!({"message":{"content":[{"type":"tool_result","tool_use_id":"a","is_error":true,"content":text}]}});
            claude_result(&line, "a").unwrap().code
        };
        assert_eq!(exit("Exit code 101\nerror: failed"), Some(101));
        assert_eq!(exit("error: failed\nExit code 2"), Some(2));
        assert_eq!(exit("String to replace not found"), None);
    }

    #[test]
    fn codex_lines_read_as_each_kind_writes_them() {
        let done = json!({"type":"event_msg","payload":{"type":"item_completed","item":{
            "type":"CommandExecution","id":"ex1","command":["/bin/zsh","-lc","cargo test"],
            "aggregated_output":"ok","exit_code":2,"duration":{"secs":3,"nanos":500000000}}}});
        let call = codex_call(&done, "ex1").unwrap();
        assert_eq!(describe_tool(&call.name, &call.args).label, "cargo test");
        let out = codex_result(&done).unwrap();
        assert_eq!(
            (out.text.as_str(), out.code, out.took_ms),
            ("ok", Some(2), Some(3500))
        );
        let wrapped = json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c",
            "output":"{\"output\":\"hi\",\"metadata\":{\"exit_code\":1}}"}});
        let out = codex_result(&wrapped).unwrap();
        assert_eq!((out.text.as_str(), out.code), ("hi", Some(1)));
        let blocks = json!({"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c",
            "output":[{"type":"input_text","text":"seen"},{"type":"input_image","image_url":"data:image/png;base64,QUJD"}]}});
        let out = codex_result(&blocks).unwrap();
        assert_eq!((out.text.as_str(), out.images), ("seen", 1));
    }

    #[test]
    fn labels_say_what_each_tool_did() {
        let label = |name: &str, args: Value| describe_tool(name, &args).label;
        assert_eq!(label("Bash", json!({"command":"ls  -a\nmore"})), "ls -a …");
        assert_eq!(label("Read", json!({"file_path":"/r/a.rs"})), "/r/a.rs");
        assert_eq!(
            label("Grep", json!({"pattern":"fn main","path":"src"})),
            "fn main · src"
        );
        assert_eq!(label("WebSearch", json!({"query":"mooring"})), "mooring");
        assert_eq!(
            label("mcp__x__navigate", json!({"url":"http://a"})),
            "http://a"
        );
        let edit = describe_tool(
            "Edit",
            &json!({"file_path":"/r/a.rs","old_string":"x","new_string":"y"}),
        );
        assert_eq!(
            (edit.kind, edit.input_kind, edit.input.as_str()),
            ("edit", "diff", "- x\n+ y\n")
        );
        let long = format!("{} ghp_0123456789abcdefghijklmnopqrstuv", "x".repeat(190));
        let cut = label("Bash", json!({ "command": long }));
        assert!(cut.contains("[已隐藏]") && !cut.contains("ghp_"), "{cut}");
        let kept = describe_tool("Write", &json!({"file_path":"/r/big.txt","_lines":[300,0]}));
        assert_eq!(kept.label, "/r/big.txt");
    }

    #[test]
    fn a_script_of_commands_is_said_to_be_one_and_what_cannot_be_read_is_written() {
        let script =
            "await tools.exec_command({cmd:\"cargo test\"});await tools.exec_command(opts);";
        let d = describe_tool("exec", &json!({"input":script}));
        assert!(d.only_commands);
        assert_eq!(d.label, "cargo test；运行时才拼出的命令，读不到内容");
        assert!(d.note.unwrap().contains("1 步"));
        let mixed = describe_tool(
            "exec",
            &json!({"input":"await tools.apply_patch(\"*** Begin Patch\\n*** Add File: /r/new.md\\n+x\\n*** End Patch\");await tools.web__run({query:\"q\"});"}),
        );
        assert!(!mixed.only_commands);
        assert_eq!(mixed.label, "改 new.md；查网页 q");
    }
}
