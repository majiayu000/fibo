//! Frames on the machine's socket to the relay: JSON text frames, and binary frames
//! `[kind][id length][id][payload]` for message bytes (alma-relay protocol 2).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The newest tunnel protocol spoken here. From 3 a machine's phones reach it only by
/// their own route tokens (`routes`), never by the token it registers with.
pub const PROTOCOL: u32 = 3;
/// The oldest relay a machine stays on: an older one routes phones by the machine's
/// token, which pairing links no longer carry, so no phone could reach it.
pub const MIN_RELAY: u32 = 3;

/// Why a machine leaves a relay that agreed on `proto` (`None`: a relay too old to say).
pub fn relay_too_old(proto: Option<u32>) -> Option<String> {
    let proto = proto.unwrap_or(1);
    (proto < MIN_RELAY)
        .then(|| format!("中继版本太旧（协议 {proto}，这台 Fibo 要 {MIN_RELAY} 以上）：请升级中继"))
}
/// Phones a machine may list in `routes`, and revoked ones in `revoked`.
pub const MAX_ROUTES: usize = 256;
/// Old tokens a machine lists as retired; a relay keeps at most this many per machine.
pub const MAX_RETIRED: usize = 8;
/// Where a machine registers.
pub const AGENT_PATH: &str = "/agent";
/// The path after `/c/<token>` that reaches Mooring's remote protocol.
pub const MOORING_PATH: &str = "/keepline";

/// The longest frame id, in bytes: a binary frame carries its length in one byte.
pub const MAX_ID_LEN: usize = u8::MAX as usize;

/// Binary frame kinds.
pub mod bin {
    /// A binary WebSocket message, either way.
    pub const WBIN: u8 = 3;
    /// A text WebSocket message, either way, UTF-8.
    pub const WTEXT: u8 = 4;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Frame {
    /// Machine → relay, first: route phones that know `sha256_hex(token)` here.
    Register {
        #[serde(rename = "tokenHash")]
        token_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ver: Option<String>,
        /// The relay's registration key, when it asks for one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proto: Option<u32>,
        /// Hashes of this machine's earlier tokens (before `--new-code`): while it is
        /// registered, a phone still on one of them is told to pair again instead of
        /// finding nothing. A relay that does not know the field ignores it.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        retired: Vec<String>,
        /// Protocol 3: `sha256_hex(token)` of each paired phone's own route token. Only
        /// these reach this machine, so a phone never learns the token it registers with
        /// and cannot take its place on the relay.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        routes: Vec<String>,
        /// Route hashes of phones it unpaired: told so with 4401 instead of finding nothing.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        revoked: Vec<String>,
    },
    /// Machine → relay, any time after registering: the whole new list of routes, when a
    /// phone was paired or unpaired.
    Routes {
        routes: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        revoked: Vec<String>,
    },
    Registered {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proto: Option<u32>,
    },
    Error {
        message: String,
    },
    /// A phone opened a WebSocket on `path` (the part after `/c/<token>`).
    Wopen {
        id: String,
        path: String,
    },
    Wopened {
        id: String,
    },
    Werr {
        id: String,
        message: String,
    },
    /// A message in JSON form; protocol 2 carries them as binary frames instead.
    Wmsg {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        s: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        b: Option<String>,
    },
    Wclose {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// A phone made a plain HTTP request; this relay never sends one, alma-relay may.
    Hreq {
        id: String,
    },
    Hres {
        id: String,
        status: u16,
        headers: serde_json::Map<String, serde_json::Value>,
    },
    Hend {
        id: String,
    },
    /// Anything else the other side speaks; ignored.
    #[serde(other)]
    Unknown,
}

impl Frame {
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).expect("frames serialize")
    }
}

/// Panics past [`MAX_ID_LEN`], which only a bug here can reach: a relay makes its own short
/// ids, and a machine refuses a longer one at `Wopen`, before any session has it.
pub fn encode(kind: u8, id: &str, payload: &[u8]) -> Vec<u8> {
    assert!(id.len() <= MAX_ID_LEN, "frame id too long");
    let mut out = Vec::with_capacity(2 + id.len() + payload.len());
    out.push(kind);
    out.push(id.len() as u8);
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(payload);
    out
}

/// `(kind, id, payload)`, or `None` when malformed.
pub fn decode(bytes: &[u8]) -> Option<(u8, &str, &[u8])> {
    let (&kind, rest) = bytes.split_first()?;
    let (&len, rest) = rest.split_first()?;
    let id = std::str::from_utf8(rest.get(..len as usize)?).ok()?;
    Some((kind, id, &rest[len as usize..]))
}

pub fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

pub fn is_token_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `/c/<token>/rest?query` → `(token, "/rest?query")`.
pub fn client_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/c/")?;
    let (token, tail) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let ok = (16..=128).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some((token, if tail.is_empty() { "/" } else { tail }))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_older_than_route_codes_is_named_as_too_old() {
        assert!(relay_too_old(None).unwrap().contains("协议 1"));
        assert!(relay_too_old(Some(2)).unwrap().contains("请升级中继"));
        assert_eq!(relay_too_old(Some(PROTOCOL)), None);
    }

    #[test]
    fn frames_match_the_alma_relay_shapes() {
        let f = Frame::Register {
            token_hash: "ab".into(),
            name: Some("mac".into()),
            ver: None,
            key: None,
            proto: Some(2),
            retired: vec![],
            routes: vec![],
            revoked: vec![],
        };
        assert_eq!(
            f.to_text(),
            r#"{"t":"register","tokenHash":"ab","name":"mac","proto":2}"#
        );
        let f: Frame =
            serde_json::from_str(r#"{"t":"wopen","id":"w1","path":"/keepline"}"#).unwrap();
        assert_eq!(
            f,
            Frame::Wopen {
                id: "w1".into(),
                path: "/keepline".into()
            }
        );
        let f: Frame =
            serde_json::from_str(r#"{"t":"pushed","id":"1","unsupported":true}"#).unwrap();
        assert_eq!(f, Frame::Unknown);
        let f: Frame = serde_json::from_str(
            r#"{"t":"hreq","id":"h1","method":"GET","path":"/","headers":{}}"#,
        )
        .unwrap();
        assert_eq!(f, Frame::Hreq { id: "h1".into() });
    }

    #[test]
    fn binary_frames_round_trip_and_reject_short_ones() {
        let b = encode(bin::WBIN, "w12", b"\x00hi");
        assert_eq!(decode(&b), Some((bin::WBIN, "w12", &b"\x00hi"[..])));
        assert_eq!(decode(&[3]), None);
        assert_eq!(decode(&[3, 5, b'a']), None);
        assert_eq!(decode(&[3, 0]), Some((3, "", &b""[..])));
    }

    #[test]
    fn a_frame_id_may_be_exactly_as_long_as_its_length_byte_says() {
        for id in ["a".repeat(MAX_ID_LEN), "中".repeat(MAX_ID_LEN / 3)] {
            assert_eq!(id.len(), 255);
            let b = encode(bin::WBIN, &id, b"x");
            assert_eq!(decode(&b), Some((bin::WBIN, id.as_str(), &b"x"[..])));
        }
    }

    #[test]
    #[should_panic(expected = "frame id too long")]
    fn encode_never_cuts_a_longer_id_down_to_its_length_byte() {
        encode(bin::WBIN, &"a".repeat(MAX_ID_LEN + 1), b"x");
    }

    #[test]
    fn client_paths_carry_a_token_and_the_rest() {
        let t = "abcdefghijklmnop";
        assert_eq!(
            client_path(&format!("/c/{t}/keepline")),
            Some((t, "/keepline"))
        );
        assert_eq!(client_path(&format!("/c/{t}")), Some((t, "/")));
        assert_eq!(client_path(&format!("/c/{t}/k?x=1")), Some((t, "/k?x=1")));
        assert_eq!(client_path(&format!("/c/{t}?x=1")), None);
        assert_eq!(client_path("/c/short/mooring"), None);
        assert_eq!(client_path("/c/abcdefghijklmnop!/x"), None);
        assert_eq!(client_path("/agent"), None);
    }

    #[test]
    fn token_hashes_are_lowercase_sha256() {
        let h = token_hash("abc");
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_token_hash(&h));
        assert!(!is_token_hash(&h.to_uppercase()));
    }
}
