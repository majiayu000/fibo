//! Codex's memory citation block. When Codex drew on its memory, one of the copies it logs
//! of a message ends with `<oai-mem-citation>…</oai-mem-citation>`, which its own UI never
//! shows. Words are compared and recorded without it (2026-11-12: one answer became three
//! records; 36 of 1211 replies in 104 recent rollouts carry the block).

const CITATION: &str = "<oai-mem-citation>";
const CITATION_END: &str = "</oai-mem-citation>";

/// The words without Codex's memory citation blocks, which its own UI hides.
pub(crate) fn spoken(s: String) -> String {
    if !s.contains(CITATION) {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(start) = rest.find(CITATION) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(CITATION_END) {
            Some(end) => &rest[start + end + CITATION_END.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out.trim_end().to_owned()
}
