//! `toolcalls` and `toolcall`: what an Agent did in a stretch of work, read from its raw log,
//! on demand. The host gives rows and offsets without its lock; the lines are read here, in
//! this process, by seeking to them, so a log of gigabytes costs what its lines cost and no
//! terminal waits. What leaves goes through `tool_text::clean` first.
use mooring_core::protocol::*;
use mooring_host::tool_text::{self, bytes_word, clean, slice};
use mooring_host::{describe_tool, reads_tools, tool_call_line, tool_result_line, ToolOutput};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One log line kept in memory at most: past this it is named and left. Real tool results
/// peak around 700 KB; a line this long holds a pasted file or an image.
const MOST_LINE: u64 = 32 << 20;
/// A page keeps inside the phone's 10-second request limit; sizes past this stay unknown.
const READ_FOR: Duration = Duration::from_secs(3);
/// JSON bytes of each end sent at first, and of each stretch fetched after. All of a reply
/// stays far under `MAX_LINE` (1 MiB), whatever it holds.
const INPUT_HEAD: usize = 24 << 10;
const INPUT_TAIL: usize = 8 << 10;
const OUTPUT_HEAD: usize = 48 << 10;
const OUTPUT_TAIL: usize = 16 << 10;
const STRETCH: usize = 64 << 10;
/// One reader at a time for all this machine's phones: a line can be 32 MiB.
static READING: Mutex<()> = Mutex::new(());

pub(super) const GONE: &str = "原始记录已不在";
pub(super) const UNSUPPORTED: &str = "这个 Agent 还不支持查看工具详情";

/// The raw log, opened once per reply; every line is checked to be the file it was read from.
struct Log {
    lines: BufReader<File>,
    inode: u64,
    /// A line just read was past [`MOST_LINE`]: there, but not to be read.
    too_long: bool,
}
impl Log {
    fn open(path: &str) -> Option<Self> {
        let file = File::open(path).ok()?;
        let inode = file.metadata().ok()?.ino();
        Some(Self {
            lines: BufReader::with_capacity(1 << 16, file),
            inode,
            too_long: false,
        })
    }
    /// The JSON line at `offset`; `None` when the file is not the one it was read from.
    fn line(&mut self, inode: u64, offset: u64) -> Option<Value> {
        if inode != self.inode {
            return None;
        }
        self.lines.seek(SeekFrom::Start(offset)).ok()?;
        let mut line = Vec::new();
        (&mut self.lines)
            .take(MOST_LINE)
            .read_until(b'\n', &mut line)
            .ok()?;
        if line.len() as u64 == MOST_LINE && !line.ends_with(b"\n") {
            self.too_long = true;
            return None;
        }
        serde_json::from_slice(&line).ok()
    }
}

fn split_text(text: &str) -> (&str, Value) {
    let (name, args) = text.split_once(' ').unwrap_or((text, ""));
    (name, serde_json::from_str(args).unwrap_or(Value::Null))
}

/// Whether it failed, and the exit code when one was stated: a CLI's bare error flag is
/// recorded as code 1 in the index, which is no exit code to show.
fn failed(row: &ToolRow, output: Option<&ToolOutput>) -> (bool, Option<i32>) {
    let logged = row.results.iter().rev().find_map(|r| r.code);
    let code = match output {
        Some(o) if o.error => o.code,
        Some(o) => o.code.or(logged),
        None => logged,
    };
    let bad = logged.is_some_and(|c| c != 0)
        || code.is_some_and(|c| c != 0)
        || output.is_some_and(|o| o.error);
    (bad, code)
}

/// How long it ran, when the log can say: whole seconds between its lines otherwise.
fn took(row: &ToolRow, output: Option<&ToolOutput>) -> Option<u64> {
    output.and_then(|o| o.took_ms).or_else(|| {
        let last = row.results.last()?;
        (row.at > 0 && last.at >= row.at).then(|| (last.at - row.at) * 1000)
    })
}

fn log_of(reply: &ToolLog) -> io::Result<()> {
    if reads_tools(reply.runtime) {
        Ok(())
    } else {
        Err(io::Error::other(UNSUPPORTED))
    }
}

pub(super) fn tool_calls(socket: &Path, request: &Value) -> io::Result<Value> {
    let session = super::chat::session_of(socket, request)?;
    let reply = rpc(
        socket,
        &Request::ToolCalls {
            session: session.clone(),
            seq: request["seq"].as_i64(),
            after: request["after"].as_i64(),
            limit: request["limit"].as_u64().unwrap_or(100).clamp(1, 200) as usize,
        },
    )?;
    let Reply::ToolCalls {
        log,
        tools,
        rows,
        next,
    } = reply
    else {
        return Err(unexpected(reply));
    };
    log_of(&log)?;
    let _one = READING.lock().unwrap_or_else(|e| e.into_inner());
    let mut raw = Log::open(&log.path);
    let gone = raw.is_none();
    let until = Instant::now() + READ_FOR;
    let mut calls = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let (name, args) = split_text(&row.text);
        let mut d = describe_tool(name, &args);
        // Cut down in the index to nothing to name it by: its raw line has the rest.
        let cut = d.kind == "command" && d.about.is_empty() && name == "Bash";
        if d.label.is_empty() || cut || (name == "exec" && args.get("input").is_none()) {
            if let Some(call) = raw
                .as_mut()
                .and_then(|l| l.line(row.inode, row.offset))
                .and_then(|l| tool_call_line(log.runtime, &l, &row.id))
            {
                d = describe_tool(&call.name, &call.args);
            }
        }
        // A script of commands, each of which has its row just after it.
        let until_result = row.results.first().map_or(i64::MAX, |r| r.seq);
        let shown_apart = d.only_commands
            && rows[i + 1..].iter().any(|r| {
                r.seq < until_result
                    && r.text
                        .split(' ')
                        .next()
                        .is_some_and(|n| n.ends_with("exec_command"))
            });
        if shown_apart {
            continue;
        }
        let output = match raw.as_mut() {
            Some(l) if Instant::now() < until => read_output(l, log.runtime, row),
            _ => None,
        };
        let (bad, code) = failed(row, output.as_ref());
        calls.push(rp::ToolCallInfo {
            id: row.id.clone(),
            name: name.to_owned(),
            kind: d.kind.to_owned(),
            label: clean(&d.label),
            about: Some(clean(&d.about)).filter(|a| !a.is_empty()),
            at: row.at,
            took_ms: took(row, output.as_ref()),
            failed: bad,
            code,
            open: row.results.is_empty(),
            size: output.as_ref().map(ToolOutput::size),
        });
    }
    Ok(json!(rp::ToolCalls {
        session,
        tools: tools.map(super::chat::tools),
        calls,
        next,
        gone,
    }))
}

/// All a call's result lines as one output; a line that cannot be read adds nothing.
fn read_output(
    log: &mut Log,
    runtime: mooring_core::model::Agent,
    row: &ToolRow,
) -> Option<ToolOutput> {
    let mut whole: Option<ToolOutput> = None;
    for r in &row.results {
        let Some(line) = log.line(r.inode, r.offset) else {
            continue;
        };
        let Some(part) = tool_result_line(runtime, &line, &row.id) else {
            continue;
        };
        match &mut whole {
            None => whole = Some(part),
            Some(w) => {
                if !w.text.is_empty() && !part.text.is_empty() {
                    w.text.push('\n');
                }
                w.text.push_str(&part.text);
                w.images += part.images;
                w.image_bytes += part.image_bytes;
                w.error |= part.error;
                w.code = part.code.or(w.code);
                w.took_ms = part.took_ms.or(w.took_ms);
            }
        }
    }
    whole
}

pub(super) fn tool_call(socket: &Path, request: &Value) -> io::Result<Value> {
    let session = super::chat::session_of(socket, request)?;
    let call = request["call"]
        .as_str()
        .ok_or_else(|| io::Error::other("toolcall 需要 call"))?;
    let reply = rpc(
        socket,
        &Request::ToolCall {
            session,
            call: call.to_owned(),
        },
    )?;
    let Reply::ToolCall { log, row } = reply else {
        return Err(unexpected(reply));
    };
    log_of(&log)?;
    let _one = READING.lock().unwrap_or_else(|e| e.into_inner());
    let mut raw = Log::open(&log.path).ok_or_else(|| io::Error::other(GONE))?;
    let found = raw
        .line(row.inode, row.offset)
        .and_then(|line| tool_call_line(log.runtime, &line, &row.id));
    let (name, described, call_too_long) = match found {
        Some(call) => (
            call.name.clone(),
            describe_tool(&call.name, &call.args),
            false,
        ),
        // Far too long a call says what the index kept of it.
        None if raw.too_long => {
            let (name, args) = split_text(&row.text);
            (name.to_owned(), describe_tool(name, &args), true)
        }
        None => return Err(io::Error::other(GONE)),
    };
    raw.too_long = false;
    let mut output = read_output(&mut raw, log.runtime, &row);
    if output.is_none() && !row.results.is_empty() {
        // Results logged but none of them readable: the file has changed since.
        if !raw.too_long {
            return Err(io::Error::other(GONE));
        }
        output = Some(ToolOutput {
            text: format!("[这条输出超过 {} MB，手机不显示]", MOST_LINE >> 20),
            ..Default::default()
        });
    }
    let input = clean(&described.input);
    let text = output.as_ref().map(|o| clean(&output_words(o)));
    if let Some(part) = request["part"].as_str() {
        let body = match part {
            "input" => input,
            "output" => text.unwrap_or_default(),
            _ => return Err(io::Error::other("part 只能是 input 或 output")),
        };
        return Ok(json!(slice(
            part,
            &body,
            request["offset"].as_u64().unwrap_or(0),
            STRETCH
        )));
    }
    let (bad, code) = failed(&row, output.as_ref());
    Ok(json!(rp::ToolCallDetail {
        info: rp::ToolCallInfo {
            id: row.id.clone(),
            name,
            kind: described.kind.to_owned(),
            label: clean(&described.label),
            about: Some(clean(&described.about)).filter(|a| !a.is_empty()),
            at: row.at,
            took_ms: took(&row, output.as_ref()),
            failed: bad,
            code,
            open: row.results.is_empty(),
            size: output.as_ref().map(ToolOutput::size),
        },
        input_kind: described.input_kind.to_owned(),
        input: tool_text::ends(&input, INPUT_HEAD, INPUT_TAIL),
        output: text.map(|t| tool_text::ends(&t, OUTPUT_HEAD, OUTPUT_TAIL)),
        steps: (described.steps.into_iter())
            .map(|(tool, label, unread)| rp::ToolStep {
                tool,
                label: clean(&label),
                unread
            })
            .collect(),
        note: match call_too_long {
            true => Some(format!(
                "这条调用的输入超过 {} MB，只能看到记录里留下的部分",
                MOST_LINE >> 20
            )),
            false => described.note,
        },
    }))
}

/// The output as read; images named where they were, by how many and how large.
fn output_words(o: &ToolOutput) -> String {
    match o.images {
        0 => o.text.clone(),
        n => {
            let note = format!("[{n} 张图片 · {}，手机不显示]", bytes_word(o.image_bytes));
            if o.text.is_empty() {
                note
            } else {
                format!("{}\n{note}", o.text)
            }
        }
    }
}

fn unexpected(reply: Reply) -> io::Error {
    match reply {
        Reply::Error(e) => io::Error::other(e),
        other => io::Error::other(format!("unexpected reply: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_core::model::Agent;

    /// Offsets mean nothing in another file, however alike: it answers only for its own.
    #[test]
    fn a_line_is_read_only_from_the_file_it_came_from() {
        let dir = std::env::temp_dir().join(format!("kl-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.jsonl");
        std::fs::write(&path, "{\"n\":1}\n{\"n\":2}\n").unwrap();
        let mut log = Log::open(path.to_str().unwrap()).unwrap();
        let inode = log.inode;
        assert_eq!(log.line(inode, 8).unwrap()["n"], 2);
        assert_eq!(log.line(inode, 0).unwrap()["n"], 1);
        assert!(log.line(inode + 1, 8).is_none());
        assert!(log.line(inode, 1).is_none(), "not on a line's start");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_clis_whose_logs_it_reads_are_offered() {
        let log = |runtime| ToolLog {
            path: String::new(),
            runtime,
        };
        assert!(log_of(&log(Agent::Claude)).is_ok());
        assert!(log_of(&log(Agent::Codex)).is_ok());
        assert_eq!(
            log_of(&log(Agent::Grok)).unwrap_err().to_string(),
            UNSUPPORTED
        );
    }
}
