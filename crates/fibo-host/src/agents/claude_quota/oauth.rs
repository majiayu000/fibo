//! The current Claude Code credential, used only with Anthropic's usage endpoint.
//! Tokens never enter the plugin cache, arguments or errors. Rotation invalidates that cache.
use fibo_core::t;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::Duration;

pub(super) struct Credential {
    pub token: String,
    pub scope: String,
}

impl Credential {
    fn new(token: String) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err(t!("Claude 登录凭据为空，请重新登录 Claude Code").into());
        }
        let scope = format!("{:x}", Sha256::digest(token.as_bytes()));
        Ok(Self { token, scope })
    }
}

pub(super) fn credential(interactive: bool) -> Result<Credential, String> {
    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        return Credential::new(token);
    }
    let home = fibo_platform::dirs::home().ok_or(t!("找不到家目录"))?;
    let custom = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|s| !s.is_empty());
    let dir = custom
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    // A custom profile must never fall through to another account's global keychain entry.
    let bytes = if custom.is_none() {
        fibo_platform::keychain::password("Claude Code-credentials", interactive).map_err(|_| {
            t!("无法读取 Claude 登录凭据；请在终端运行 fibo claude-quota refresh 授权读取")
        })?
    } else {
        None
    };
    let bytes = bytes
        .map(Ok)
        .unwrap_or_else(|| std::fs::read(dir.join(".credentials.json")))
        .map_err(|_| {
            t!("没有可读取的 Claude 登录凭据；请登录 Claude Code 后运行 fibo claude-quota refresh")
        })?;
    let doc: Value =
        serde_json::from_slice(&bytes).map_err(|_| t!("Claude 登录凭据格式无法读取"))?;
    let login = &doc["claudeAiOauth"];
    if login["expiresAt"]
        .as_u64()
        .is_some_and(|at| at <= super::usage::now() * 1000)
    {
        return Err(t!("Claude 登录凭据已过期，请在 Claude Code 中刷新登录后重试").into());
    }
    if login["scopes"]
        .as_array()
        .is_some_and(|s| !s.iter().any(|v| v == "user:profile"))
    {
        return Err(t!("Claude 登录凭据没有额度读取权限，请使用 Claude Code 的账号登录").into());
    }
    Credential::new(
        login["accessToken"]
            .as_str()
            .ok_or(t!("Claude 登录凭据没有账号访问令牌"))?
            .to_owned(),
    )
}

pub(super) struct Failure {
    pub message: String,
    pub retry_at: Option<u64>,
    pub discard: bool,
}

impl Failure {
    pub fn read(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retry_at: None,
            discard: false,
        }
    }
}

pub(super) fn fetch(credential: &Credential) -> Result<Value, Failure> {
    request(
        "https://api.anthropic.com/api/oauth/usage",
        credential,
        super::usage::now(),
    )
}

fn request(url: &str, credential: &Credential, now: u64) -> Result<Value, Failure> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Failure::read(t!("额度 HTTP 客户端无法启动")))?;
    let response = client
        .get(url)
        .bearer_auth(&credential.token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .header("User-Agent", "claude-code/2.1.0")
        .send()
        .map_err(|_| Failure::read(t!("额度查询未成功：网络不可用或请求超时")))?;
    let status = response.status().as_u16();
    if status == 429 {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok());
        return Err(Failure {
            message: t!("额度查询被限流，等待后自动重试").into(),
            retry_at: Some(retry_at(retry, now)),
            discard: false,
        });
    }
    if status == 401 || status == 403 {
        return Err(Failure {
            message: t!("额度查询未获授权，请检查 Claude Code 登录和额度读取权限").into(),
            retry_at: None,
            discard: true,
        });
    }
    if status != 200 {
        return Err(Failure::read(t!("额度查询失败（HTTP {status}）", status)));
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::read(t!("额度响应读取失败")))?;
    if bytes.len() > 1024 * 1024 {
        return Err(Failure::read(t!("额度响应过大")));
    }
    let doc: Value =
        serde_json::from_slice(&bytes).map_err(|_| Failure::read(t!("额度响应不是 JSON")))?;
    windows(&doc).map_err(Failure::read)
}

fn retry_at(header: Option<&str>, now: u64) -> u64 {
    header
        .and_then(|h| {
            h.trim()
                .parse::<u64>()
                .ok()
                .map(|seconds| now.saturating_add(seconds))
                .or_else(|| {
                    jiff::fmt::rfc2822::parse(h.trim())
                        .ok()
                        .map(|d| d.timestamp().as_second().max(0) as u64)
                })
        })
        .unwrap_or(now.saturating_add(300))
        .max(now.saturating_add(60))
}

fn window(value: &Value) -> Result<Option<Value>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let used = value["utilization"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.)
        .ok_or(t!("额度响应缺少有效百分比"))?;
    let resets_at = match value.get("resets_at").filter(|v| !v.is_null()) {
        None => 0,
        Some(v) => {
            v.as_str()
                .ok_or(t!("额度重置时间格式错误"))?
                .parse::<jiff::Timestamp>()
                .map_err(|_| t!("额度重置时间格式错误"))?
                .as_second()
                .max(0) as u64
        }
    };
    Ok(Some(json!({"used": used, "resets_at": resets_at})))
}

fn windows(doc: &Value) -> Result<Value, String> {
    // An empty/error object must not look like an authoritative account with no windows.
    if doc.get("five_hour").is_none() && doc.get("seven_day").is_none() {
        return Err(t!("额度响应缺少额度窗口").into());
    }
    let mut out = json!({"models": []});
    for (key, name) in [("five_hour", "session"), ("seven_day", "week")] {
        if let Some(w) = window(&doc[key])? {
            out[name] = w;
        }
    }
    let mut models = Vec::new();
    for (key, value) in doc.as_object().ok_or(t!("额度响应格式错误"))? {
        if let Some(name) = key.strip_prefix("seven_day_") {
            if let Some(mut w) = window(value)? {
                w["name"] = name.into();
                models.push(w);
            }
        }
    }
    if let Some(limits) = doc["limits"].as_array() {
        for limit in limits
            .iter()
            .filter(|l| l["group"] == "weekly" && l["kind"] == "weekly_scoped")
        {
            if let Some(name) = limit
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
            {
                let mut w = window(
                    &json!({"utilization": limit["percent"], "resets_at": limit["resets_at"]}),
                )?
                .ok_or(t!("模型额度窗口为空"))?;
                w["name"] = name.into();
                if !models.iter().any(|old| {
                    old["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                }) {
                    models.push(w);
                }
            }
        }
    }
    out["models"] = models.into();
    Ok(out)
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
