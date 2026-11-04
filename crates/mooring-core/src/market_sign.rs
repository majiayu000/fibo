//! Who published a catalog: its `index.sig`, and the keys behind it (see `market`).
use super::*;

/// Who a catalog's signature says published it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Publisher {
    /// This repository: signed by `OFFICIAL_KEY`, or shipped inside the app itself.
    Official,
    /// Another key, valid for this index.
    Signed { name: String, key: String },
    /// No `index.sig`.
    Unsigned,
}
impl Publisher {
    /// The key to remember with what was installed.
    pub(super) fn key(&self) -> Option<String> {
        match self {
            Self::Official => Some(OFFICIAL_KEY.to_owned()),
            Self::Signed { key, .. } => Some(key.clone()),
            Self::Unsigned => None,
        }
    }
    /// A key's first characters: enough to see that it changed.
    pub(super) fn fingerprint(key: &str) -> String {
        key.chars().take(16).collect()
    }
    /// In words, for a confirmation.
    pub fn said(&self) -> String {
        match self {
            Self::Official => OFFICIAL.to_owned(),
            Self::Signed { name, key } => {
                format!("{name}（密钥 {}）", Self::fingerprint(key))
            }
            Self::Unsigned => "没有签名的目录".to_owned(),
        }
    }
}

/// `index.sig`: who signed the bytes of `index.json`, and the signature.
#[derive(Debug, Serialize, Deserialize)]
struct Signature {
    alg: String,
    publisher: String,
    key: String,
    sig: String,
}
/// `publisher.key`: a publisher's private key. Never published.
#[derive(Serialize, Deserialize)]
struct SecretKey {
    publisher: String,
    secret: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// A new publisher key: the file to keep private, and the public key to show.
pub fn new_key(publisher: &str) -> Result<(String, String), String> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("拿不到随机数：{e}"))?;
    let public = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key();
    let file = SecretKey {
        publisher: publisher.to_owned(),
        secret: to_hex(&secret),
    };
    Ok((
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())? + "\n",
        to_hex(public.as_bytes()),
    ))
}

/// `index.sig` for these exact `index.json` bytes, from a `publisher.key`.
pub fn sign_index(index: &[u8], secret_key: &[u8]) -> Result<String, String> {
    use ed25519_dalek::Signer;
    let file: SecretKey =
        serde_json::from_slice(secret_key).map_err(|e| format!("私钥文件格式不对：{e}"))?;
    let secret = from_hex::<32>(&file.secret).ok_or("私钥文件格式不对")?;
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    let signature = Signature {
        alg: "ed25519".into(),
        publisher: file.publisher,
        key: to_hex(key.verifying_key().as_bytes()),
        sig: to_hex(&key.sign(index).to_bytes()),
    };
    Ok(serde_json::to_string_pretty(&signature).map_err(|e| e.to_string())? + "\n")
}

/// Who signed these index bytes; an invalid signature is an error, never "unsigned".
pub fn verify_index(index: &[u8], signature: &[u8]) -> Result<Publisher, String> {
    use ed25519_dalek::Verifier;
    let s: Signature =
        serde_json::from_slice(signature).map_err(|e| format!("index.sig 格式不对：{e}"))?;
    if s.alg != "ed25519" {
        return Err(format!("index.sig 用的是 {}，这个版本只认 ed25519", s.alg));
    }
    let key = from_hex::<32>(&s.key)
        .and_then(|k| ed25519_dalek::VerifyingKey::from_bytes(&k).ok())
        .ok_or("index.sig 的密钥格式不对")?;
    let sig = from_hex::<64>(&s.sig).ok_or("index.sig 的签名格式不对")?;
    key.verify(index, &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| "index.json 和它的签名对不上：可能被改过，没有使用".to_owned())?;
    Ok(if s.key == OFFICIAL_KEY {
        Publisher::Official
    } else {
        Publisher::Signed {
            name: s.publisher,
            key: s.key,
        }
    })
}

/// What a catalog at `url` counts as, given its index and signature if it has one. This
/// repository's own address accepts nothing but this repository's key.
pub(super) fn publisher_of(
    url: &str,
    index: &[u8],
    signature: Option<&[u8]>,
) -> Result<Publisher, String> {
    let publisher = match signature {
        Some(s) => verify_index(index, s)?,
        None => Publisher::Unsigned,
    };
    if url == DEFAULT_REMOTE && publisher != Publisher::Official {
        return Err("官方目录没有官方的签名，没有使用".to_owned());
    }
    Ok(publisher)
}
