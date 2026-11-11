//! What hovering a conversation's row shows.
use super::super::diagnostics::TipLine;
use super::*;

impl Workbench {
    /// The full name first (the row truncates it), then where the name comes from,
    /// what was first and last asked, and the folder it runs in.
    pub(in crate::workbench) fn session_tip(&self, s: &AgentSession) -> Vec<TipLine> {
        use mooring_core::content::short;
        use mooring_core::model::NameSource;
        let naming = self.workspace.session_naming(s).map(|(_, source)| source);
        let mut lines = vec![TipLine::Title(self.session_title(s))];
        lines.push(TipLine::Quiet(match naming {
            Some(source) => t!("{source} · 双击改名", source = source.label(s.runtime)),
            None => t!("双击改名").into(),
        }));
        if let Some(c) = self.workspace.content.get(&s.id) {
            let line = |text: &str| short(&ask_line(text), 120);
            if let Some(first) = c.asks.first().filter(|_| c.first_ask_known) {
                lines.push(TipLine::Labeled(t!("最初").into(), line(&first.text)));
            }
            // Only when it is not the first ask shown above.
            if let Some(last) = c
                .asks
                .last()
                .filter(|_| c.asks.len() > 1 || !c.first_ask_known)
            {
                lines.push(TipLine::Labeled(t!("最近").into(), line(&last.text)));
            }
        }
        let folder = super::super::settings::tilde(&s.cwd);
        // "Codex 起的名" already says which agent; otherwise the folder line does.
        lines.push(TipLine::Quiet(if naming == Some(NameSource::Agent) {
            folder
        } else {
            format!("{} · {folder}", s.runtime.name())
        }));
        lines
    }
}

/// One line of an ask: whitespace collapsed, and a pasted image's "[Image #1]",
/// which names nothing to a reader, shown as a short mark.
fn ask_line(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mark = t!("[图]");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("[Image #") {
        let after = &rest[at + "[Image #".len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let end = (digits > 0 && after[digits..].starts_with(']'))
            .then_some(at + "[Image #".len() + digits + 1);
        match end {
            Some(end) => {
                out.push_str(&rest[..at]);
                out.push_str(mark);
                rest = &rest[end..];
            }
            None => {
                out.push_str(&rest[..at + 1]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::ask_line;

    #[test]
    fn pasted_images_read_as_a_mark() {
        assert_eq!(
            ask_line("[Image #1] 这个转圈\n不是全局的吗 [Image #2] 这里"),
            "[图] 这个转圈 不是全局的吗 [图] 这里"
        );
        assert_eq!(
            ask_line("讲讲 [Image #] 和 [Image #x]"),
            "讲讲 [Image #] 和 [Image #x]"
        );
    }
}
