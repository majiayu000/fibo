//! api 2: a CLI that answers its approvals over a local HTTP API, declared by a plugin.
use serde::Deserialize;
use serde_json::Value;

/// Paths on the port its process listens on, and JSON pointers into what they return.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    /// GET: a JSON array of what it is asking now.
    pub pending: String,
    /// Pointer to an entry's id, for `{id}` in `reply`.
    pub id: String,
    /// Pointers to what it asks about; the first that holds text counts.
    pub subject: Vec<String>,
    /// Pointer to its kind, as a title: `bash`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Pointer to a list of what 「always」 grants. Empty or missing: no such option.
    #[serde(default)]
    pub always: Option<String>,
    /// The variable in the CLI's environment holding its API's password, for Basic auth. Set
    /// it to a random value in `start`: an open local port takes requests from any web page.
    #[serde(default)]
    pub password_env: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    /// POST, with `{id}`.
    pub reply: String,
    /// The bodies to POST.
    pub allow: Value,
    pub allow_always: Value,
    pub deny: Value,
}
impl Answers {
    pub fn check(&self) -> Result<(), String> {
        let paths = [&self.pending, &self.reply];
        if paths.iter().any(|p| !p.starts_with('/')) || !self.reply.contains("{id}") {
            return Err("answers 的 pending、reply 要以 / 开头，reply 要含 {id}".into());
        }
        let pointers = std::iter::once(&self.id)
            .chain(&self.subject)
            .chain(&self.kind)
            .chain(&self.always);
        if self.subject.is_empty() || pointers.into_iter().any(|p| !p.starts_with('/')) {
            return Err("answers 里的指针要以 / 开头，subject 至少一个".into());
        }
        Ok(())
    }
}
