//! Secrets out of anything a tool call shows a phone. One pass, one function, on the machine
//! before the text leaves: a relay and a phone are not trusted with them. It works from what
//! a secret looks like, so it can miss one and can hide what only looks like one; the
//! numbers on real logs are in docs/phone-app/v2-plan.md.
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::sync::LazyLock;

pub const HIDDEN: &str = "[已隐藏]";

/// A private key's whole block; one cut off before its end hides what is left.
static KEY_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\z)").unwrap()
});
/// `scheme://user:secret@host`, and `scheme://long-token@host`.
static URL_LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]*:)[^/\s@]+@|([A-Za-z][A-Za-z0-9+.-]*://)[A-Za-z0-9_-]{16,}@").unwrap()
});
/// Tokens that say what they are by how they begin.
static PREFIXED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:",
        r"sk-(?:ant-|proj-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}",
        r"|gh[pousr]_[A-Za-z0-9]{20,}",
        r"|github_pat_[A-Za-z0-9_]{20,}",
        r"|xox[abeoprs]-[A-Za-z0-9-]{10,}",
        r"|xapp-[0-9]-[A-Za-z0-9-]{10,}",
        r"|A(?:KIA|SIA)[0-9A-Z]{16}",
        r"|AIza[0-9A-Za-z_-]{30,}",
        r"|glpat-[A-Za-z0-9_-]{16,}",
        r"|npm_[A-Za-z0-9]{30,}",
        r"|pypi-[A-Za-z0-9_-]{30,}",
        r"|hf_[A-Za-z0-9]{30,}",
        r"|gsk_[A-Za-z0-9]{30,}",
        r"|ya29\.[A-Za-z0-9_-]{20,}",
        r"|[rs]k_(?:live|test)_[A-Za-z0-9]{16,}",
        r"|SG\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}",
        r"|eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
        r")",
    ))
    .unwrap()
});
/// `Authorization: Bearer …`, or a bare `Bearer …`.
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\bauthorization[\x22']?\s*[:=]\s*[\x22']?(?:bearer\s+|basic\s+|token\s+)?|\bbearer\s+)([A-Za-z0-9._~+/=-]{8,})").unwrap()
});
/// `api_key = …`, `"password": "…"`, `?token=…`, `export GH_TOKEN=…`.
static ASSIGNED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?i)([A-Za-z0-9_.-]*(?:token|secret|password|passwd|pwd|api[_-]?key|access[_-]?key|private[_-]?key|credential)[A-Za-z0-9_.-]*\\?["']?\s*(?::=|=>|[:=])\s*)"#,
        r#"(\\?"(?:[^"\\\n]|\\[^"\n])*\\?"|'[^'\n]*'|`[^`\n]*`|[^\s"'`,;&)}\]]+)"#,
    ))
    .unwrap()
});

/// `--token abc`, `--api-key abc`: a flag's value after a space.
static FLAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(--[a-z0-9-]*(?:token|secret|password|passwd|api-?key)[a-z0-9-]*\s+)([^\s\x22'`,;&)}\]]+)").unwrap()
});
/// Words a value being shown as an example or a reference is made of.
const NOT_A_VALUE: &[&str] = &[
    "${",
    "{{",
    "$(",
    "process.env",
    "os.environ",
    "env.",
    "xxx",
    "***",
    "...",
    "…",
    "your",
    "example",
    "placeholder",
    "changeme",
    "redacted",
    "hidden",
    "已隐藏",
];

/// Whether what follows a secret-sounding name is one, rather than code or a count:
/// `token: Option<String>`, `max_tokens = 4096`, `password = args.password`.
fn is_secret(name: &str, value: &str) -> bool {
    let name = name.to_ascii_lowercase();
    // Counts and text-splitting, not credentials.
    if name.contains("tokeniz")
        || name.contains("tokens")
        || name.contains("token_") && name.contains("count")
    {
        return false;
    }
    let inner = value.trim_matches(|c| "\\\"'`".contains(c));
    let lower = inner.to_ascii_lowercase();
    if NOT_A_VALUE.iter().any(|w| lower.contains(w))
        || matches!(
            lower.as_str(),
            "true"
                | "false"
                | "null"
                | "none"
                | "nil"
                | "undefined"
                | "string"
                | "str"
                | "bool"
                | "number"
        )
        || inner.starts_with('$')
        || inner.starts_with('<')
    {
        return false;
    }
    if inner.len() != value.len() {
        // Quoted: a sentence is a message about a secret, a path is where one lives.
        return inner.len() >= 4
            && !inner.contains(' ')
            && !inner.starts_with('/')
            && !inner.starts_with("~/");
    }
    // Bare: a word or a call is code; a real value has digits or mixed case and some length.
    let code = "()<>[]{}".chars().any(|c| inner.contains(c))
        || inner.contains("::")
        || inner.contains("->");
    let digits = inner.chars().any(|c| c.is_ascii_digit());
    let letters = inner.chars().any(|c| c.is_ascii_alphabetic());
    let mixed = inner.chars().any(|c| c.is_ascii_lowercase())
        && inner.chars().any(|c| c.is_ascii_uppercase());
    let dotted = inner.contains('.') && !digits;
    !code
        && !dotted
        && !inner.starts_with('/')
        && !inner.starts_with('.')
        && ((digits && letters && inner.len() >= 6) || (mixed && inner.len() >= 16))
}

/// A header's value, where a plain word is prose: 「bearer authentication」.
fn is_token(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_digit())
        || value.chars().any(|c| c.is_ascii_uppercase())
            && value.chars().any(|c| c.is_ascii_lowercase())
            && value.len() >= 16
}

/// `text` with every secret it can recognize replaced by [`HIDDEN`].
pub fn redact(text: &str) -> Cow<'_, str> {
    redact_with(text, &mut |_, _, _| ())
}

/// As [`redact`], telling `hit` the rule, the name and the value of each one hidden. Only for
/// measuring the rules on real logs (`tests/redact_corpus.rs`), which keeps counts and no text.
pub fn redact_with<'a>(text: &'a str, hit: &mut dyn FnMut(&str, &str, &str)) -> Cow<'a, str> {
    let mut out = Cow::Borrowed(text);
    let mut apply = |re: &Regex, by: &mut dyn FnMut(&Captures) -> String| {
        if re.is_match(&out) {
            let next = re.replace_all(&out, |c: &Captures| by(c)).into_owned();
            out = Cow::Owned(next);
        }
    };
    apply(&KEY_BLOCK, &mut |c| {
        hit("private_key", "", &c[0]);
        HIDDEN.to_owned()
    });
    apply(&URL_LOGIN, &mut |c| {
        hit("url_login", "", &c[0]);
        match c.get(1) {
            Some(login) => format!("{}{HIDDEN}@", login.as_str()),
            None => format!("{}{HIDDEN}@", &c[2]),
        }
    });
    apply(&PREFIXED, &mut |c| {
        hit("prefixed", "", &c[0]);
        HIDDEN.to_owned()
    });
    apply(&AUTH_HEADER, &mut |c| {
        if is_token(&c[2]) {
            hit("auth_header", &c[1], &c[2]);
            format!("{}{HIDDEN}", &c[1])
        } else {
            c[0].to_owned()
        }
    });
    apply(&FLAG, &mut |c| {
        if is_secret(&c[1], &c[2]) {
            hit("flag", &c[1], &c[2]);
            format!("{}{HIDDEN}", &c[1])
        } else {
            c[0].to_owned()
        }
    });
    apply(&ASSIGNED, &mut |c| {
        if is_secret(&c[1], &c[2]) {
            hit("assigned", &c[1], &c[2]);
            format!("{}{HIDDEN}", &c[1])
        } else {
            c[0].to_owned()
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hides(text: &str, secret: &str) {
        let out = redact(text);
        assert!(!out.contains(secret), "{text} → {out}");
        assert!(out.contains(HIDDEN), "{text} → {out}");
    }
    fn keeps(text: &str) {
        assert_eq!(redact(text), text);
    }

    #[test]
    fn tokens_that_say_what_they_are_go() {
        for t in [
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAA",
            "sk-proj-abcdefghijklmnopqrstuvwx",
            "ghp_0123456789abcdefghijklmnopqrstuv",
            "github_pat_11ABCDEFG0abcdefghijkl_mnop",
            "xoxb-123456789012-abcdefghijkl",
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA1234567890abcdefghijklmnopqrstu",
            "glpat-abcdefghijklmnopqrst",
            "npm_abcdefghijklmnopqrstuvwxyz012345",
            "hf_abcdefghijklmnopqrstuvwxyz012345",
            "sk_live_0123456789abcdefgh",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abcdefghijkl",
        ] {
            hides(&format!("curl -s {t} | jq ."), t);
        }
    }

    #[test]
    fn a_private_key_block_goes_whole_even_cut_short() {
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\nAAAA\n-----END OPENSSH PRIVATE KEY-----";
        let text = format!("before\n{key}\nafter");
        assert_eq!(redact(&text), format!("before\n{HIDDEN}\nafter"));
        let cut = redact("-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nAAAA");
        assert_eq!(cut, HIDDEN);
    }

    #[test]
    fn assignments_keep_the_name_and_lose_the_value() {
        for (text, secret) in [
            ("GITHUB_TOKEN=abc123def456", "abc123def456"),
            ("export API_KEY=Abc123XYZ789", "Abc123XYZ789"),
            (r#"{"password": "hunter2!"}"#, "hunter2!"),
            ("password: 'correct-horse'", "correct-horse"),
            ("client_secret = \"s3cr3t-value\"", "s3cr3t-value"),
            ("https://x.io/cb?token=abc123def456&page=2", "abc123def456"),
            ("gh auth login --token Abc123Def456", "Abc123Def456"),
            ("mysql --password abc123xyz db", "abc123xyz"),
            (r#"{\"api_key\":\"Zz9Yy8Xx7\"}"#, "Zz9Yy8Xx7"),
        ] {
            hides(text, secret);
        }
        assert_eq!(
            redact("GITHUB_TOKEN=abc123def456"),
            format!("GITHUB_TOKEN={HIDDEN}")
        );
        assert_eq!(
            redact("https://x.io/cb?token=abc123def456&page=2"),
            format!("https://x.io/cb?token={HIDDEN}&page=2")
        );
    }

    #[test]
    fn logins_in_a_url_and_auth_headers_go() {
        assert_eq!(
            redact("postgres://me:pa55word@db.local/app"),
            format!("postgres://me:{HIDDEN}@db.local/app")
        );
        hides(
            "https://abcdefgh12345678@github.com/x.git",
            "abcdefgh12345678",
        );
        hides("Authorization: Bearer abc.def-123456", "abc.def-123456");
        hides("-H 'authorization: token Abc123Def456'", "Abc123Def456");
        hides(
            "curl -H \"Authorization: Basic dXNlcjpwYXNz1\"",
            "dXNlcjpwYXNz1",
        );
    }

    #[test]
    fn code_and_counts_that_only_sound_like_secrets_stay() {
        for text in [
            "max_tokens: 4096",
            "input_tokens = 12",
            "tokenizer: cl100k_base",
            "token: Option<String>,",
            "password: string",
            "let secret = get_secret();",
            "password = args.password",
            "api_key=$OPENAI_API_KEY",
            "token: ${{ secrets.GITHUB_TOKEN }}",
            "password: \"<your password>\"",
            "secret_key: \"please set this in the environment\"",
            "private_key: \"/Users/me/.ssh/id_ed25519\"",
            "bearer authentication is required",
            "http://localhost:3000/a@b",
            "git@github.com:me/repo.git",
            "the password was rejected",
            "password: \"your-password-here\"",
            "api_key=\"xxxxxxxxxx\"",
            "token: \"REDACTED1234\"",
            "gh auth login --with-token < file",
            "--token-limit 4096",
            "--api-key $KEY",
        ] {
            keeps(text);
        }
    }

    #[test]
    fn hiding_twice_changes_nothing() {
        let once = redact("GITHUB_TOKEN=abc123def456 sk-ant-AAAAAAAAAAAAAAAAAAAAAAAA").into_owned();
        assert_eq!(redact(&once), once);
    }
}
