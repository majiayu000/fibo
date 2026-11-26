//! The other relays a machine stays on (`Config::also`), as a configure changes them.
use fibo_core::protocol::RemoteRelay;

/// `asked` replaces `saved` (None keeps it). A relay asked without a key keeps the key
/// saved for it, an empty key clears it; blanks, repeats and the main `relay` drop out.
pub(crate) fn merge_also(
    saved: Vec<RemoteRelay>,
    asked: Option<Vec<RemoteRelay>>,
    relay: &str,
) -> Vec<RemoteRelay> {
    let Some(asked) = asked else {
        return saved.into_iter().filter(|a| a.relay != relay).collect();
    };
    let mut out: Vec<RemoteRelay> = Vec::new();
    for a in asked {
        let url = a.relay.trim().to_owned();
        if url.is_empty() || url == relay || out.iter().any(|o| o.relay == url) {
            continue;
        }
        let key = match a.key {
            None => saved
                .iter()
                .find(|s| s.relay == url)
                .and_then(|s| s.key.clone()),
            Some(k) => Some(k.trim().to_owned()).filter(|k| !k.is_empty()),
        };
        out.push(RemoteRelay { relay: url, key });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(relay: &str, key: Option<&str>) -> RemoteRelay {
        RemoteRelay {
            relay: relay.into(),
            key: key.map(Into::into),
        }
    }

    #[test]
    fn keys_stay_unless_given_and_the_main_relay_is_not_repeated() {
        let saved = vec![r("https://b", Some("kb"))];
        assert_eq!(merge_also(saved.clone(), None, "https://a"), saved);
        assert_eq!(
            merge_also(
                saved.clone(),
                Some(vec![r(" https://b ", None)]),
                "https://a"
            ),
            saved
        );
        assert_eq!(
            merge_also(
                saved.clone(),
                Some(vec![r("https://b", Some(""))]),
                "https://a"
            ),
            vec![r("https://b", None)]
        );
        let asked = vec![
            r("https://a", None),
            r("", None),
            r("https://c", Some("kc")),
            r("https://c", None),
        ];
        assert_eq!(
            merge_also(saved.clone(), Some(asked), "https://a"),
            vec![r("https://c", Some("kc"))]
        );
        // Moving the main relay onto what was an extra one drops it from the extras.
        assert!(merge_also(saved, None, "https://b").is_empty());
        assert!(merge_also(vec![], Some(vec![]), "https://a").is_empty());
    }
}
