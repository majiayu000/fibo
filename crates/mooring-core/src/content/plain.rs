//! Agent prose flattened to one line, for rows, cards and previews.

/// A Markdown table's rule line: dashes, colons and pipes, with at least one of each kind
/// a rule needs. A bare `---` is a horizontal rule, not a table.
fn table_rule(line: &str) -> bool {
    line.contains('|') && line.contains('-') && line.chars().all(|c| "|-: \t".contains(c))
}

/// Agent prose as one readable line: Markdown emphasis, headings, quotes, list
/// bullets and table rules go; table cells read as `a · b`. A pipe outside a table
/// (`a | b`, `x || y`) is just a character.
pub fn plain_line(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    // A row: pipes at the edge, or next to a rule line, where outer pipes are optional.
    let in_table = |i: usize| {
        let l = lines[i];
        if !l.contains('|') {
            return false;
        }
        if l.starts_with('|') {
            return true;
        }
        let mut up = i;
        while up > 0 && lines[up].contains('|') {
            if table_rule(lines[up - 1]) {
                return true;
            }
            up -= 1;
        }
        lines.get(i + 1).is_some_and(|n| table_rule(n))
    };
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty() && !table_rule(l) && !l.chars().all(|c| "-: ".contains(c)))
        .map(|(i, l)| {
            let l = l
                .trim_start_matches('#')
                .trim_start_matches('>')
                .trim_start();
            let l = ["- ", "* ", "+ "]
                .iter()
                .find_map(|b| l.strip_prefix(b))
                .unwrap_or(l);
            let l = l.replace("**", "").replace("__", "").replace('`', "");
            if !in_table(i) {
                return l;
            }
            l.trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_line_drops_markdown() {
        assert_eq!(
            plain_line(
                "## 结论\n**建议都接收**，见 `a.rs`\n\n| 投稿 | 建议处理 |\n|---|:-:|\n- 第一条"
            ),
            "结论 建议都接收，见 a.rs 投稿 · 建议处理 第一条"
        );
    }
    #[test]
    fn plain_line_splits_only_tables() {
        assert_eq!(
            plain_line("用 `a || b` 或 x | y 都行"),
            "用 a || b 或 x | y 都行"
        );
        assert_eq!(plain_line("| a | b |\n|---|---|\n| 1 | 2 |"), "a · b 1 · 2");
        assert_eq!(
            plain_line("a | b\n---|---\n1 | 2\n3 | 4"),
            "a · b 1 · 2 3 · 4"
        );
        assert_eq!(plain_line("上面\n---\n下面"), "上面 下面");
    }
}
