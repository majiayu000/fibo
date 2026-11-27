//! `transcribe`: a phone's recording (base64 `audio`, an `m4a` from the phone's own recorder)
//! turned into text on this computer, for a phone whose system has no speech recognition it
//! lends to apps (2026-11-28: an OPPO with none). `ffmpeg` makes 16 kHz mono WAV of it and
//! `whisper-cli` (whisper.cpp) with the model in the runtime's `models` reads it. Nothing leaves
//! the computer. `hello` lists the op only where both programs and the model are there.
use super::text;
use base64::Engine;
use serde_json::{json, Value};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";
/// Skips what no one says: without it, a pause before speaking came back as a video's opening
/// credits (2026-11-28 phone: 「优优独播剧场——YoYo Television Series Exclusive」).
const VAD: &str = "ggml-silero-v5.1.2.bin";
/// Brew's folders: an App started from Finder has neither on its PATH.
const MORE_PATHS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];
/// Steers whisper to simplified Chinese with punctuation, and to the words said to an Agent.
const ZH_PROMPT: &str =
    "以下是普通话，用简体中文和标点。会提到 Claude、Codex、bun test、cargo、git、PR 这类编程词。";

fn model(socket: &Path) -> PathBuf {
    socket.with_file_name("models").join(MODEL)
}

/// A program on PATH or in brew's folders.
fn program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    (std::env::split_paths(&path).chain(MORE_PATHS.iter().map(PathBuf::from)))
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Both programs and the model are here: the phone may offer its microphone through this computer.
pub(super) fn available(socket: &Path) -> bool {
    model(socket).is_file() && program("ffmpeg").is_some() && program("whisper-cli").is_some()
}

pub(super) fn transcribe(socket: &Path, request: &Value) -> io::Result<Value> {
    let audio = base64::engine::general_purpose::STANDARD
        .decode(text(request, "audio")?)
        .map_err(|e| io::Error::other(t!("audio 不是 base64：{e}", e)))?;
    let english = request.get("lang").and_then(Value::as_str) == Some("en");
    let missing = || io::Error::other(t!("这台电脑还没装好语音识别"));
    let (ffmpeg, whisper) = (
        program("ffmpeg").ok_or_else(missing)?,
        program("whisper-cli").ok_or_else(missing)?,
    );
    let model = model(socket);
    if !model.is_file() {
        return Err(missing());
    }
    // One folder per recording, gone when it is read, whether or not that worked.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = socket
        .with_file_name("phone-voice")
        .join(format!("{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let vad = Some(socket.with_file_name("models").join(VAD)).filter(|v| v.is_file());
    let said = read(
        &dir,
        &audio,
        (&ffmpeg, &whisper, &model, vad.as_deref()),
        english,
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(json!({ "text": said?.trim() }))
}

fn read(
    dir: &Path,
    audio: &[u8],
    (ffmpeg, whisper, model, vad): (&Path, &Path, &Path, Option<&Path>),
    english: bool,
) -> io::Result<String> {
    let (m4a, wav) = (dir.join("in.m4a"), dir.join("in.wav"));
    std::fs::write(&m4a, audio)?;
    run(Command::new(ffmpeg)
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(&m4a)
        .args(["-ar", "16000", "-ac", "1"])
        .arg(&wav))?;
    let mut whisper = Command::new(whisper);
    whisper
        .arg("-m")
        .arg(model)
        .args(["-nt", "-np", "-sns", "-f"])
        .arg(&wav);
    if let Some(vad) = vad {
        whisper.arg("--vad").arg("-vm").arg(vad);
    }
    match english {
        true => whisper.args(["-l", "en"]),
        false => whisper.args(["-l", "zh", "--prompt", ZH_PROMPT]),
    };
    run(&mut whisper)
}

/// The program's output, or why it failed.
fn run(command: &mut Command) -> io::Result<String> {
    fibo_platform::process::no_console(command);
    let out = command.stdin(Stdio::null()).output()?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let last = why.lines().last().unwrap_or_default().to_owned();
        return Err(io::Error::other(t!("语音识别失败：{last}", last)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_model_the_op_is_not_offered_and_says_why() {
        let dir = std::env::temp_dir().join(format!("fibo-transcribe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("host.sock");
        assert!(!available(&socket));
        let err = transcribe(&socket, &json!({ "audio": "AAAA" })).unwrap_err();
        assert_eq!(err.to_string(), t!("这台电脑还没装好语音识别"));
        let bad = transcribe(&socket, &json!({ "audio": "not base64!" })).unwrap_err();
        assert!(bad.to_string().contains("base64"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
