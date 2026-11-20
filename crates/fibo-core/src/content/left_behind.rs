use super::Content;

pub struct LeftBehind {
    pub line: String,
    pub ports: Vec<u16>,
}

impl LeftBehind {
    pub fn port_line(&self, ports: &str) -> String {
        t!("{line} · 端口 {ports}", line = self.line, ports)
    }

    pub fn into_text(self) -> String {
        if self.ports.is_empty() {
            return self.line;
        }
        let ports: Vec<_> = self.ports.iter().map(u16::to_string).collect();
        self.port_line(&ports.join(t!("、")))
    }
}

impl Content {
    pub fn left_behind(&self) -> Option<String> {
        self.left_behind_detail().map(LeftBehind::into_text)
    }

    /// What a finished turn left running that nothing waits for. A shell with no
    /// port or not looked at yet is only known to be a process.
    pub(crate) fn left_behind_detail(&self) -> Option<LeftBehind> {
        let b = self.background?;
        let gone = self.shells.iter().filter(|t| t.ended).count() as u32;
        let left = b.shells.saturating_sub(b.jobs).saturating_sub(gone);
        if left == 0 {
            return None;
        }
        let mut ports: Vec<_> = self
            .shells
            .iter()
            .filter(|t| !t.job && !t.ended)
            .filter_map(|t| t.port)
            .collect();
        ports.sort_unstable();
        ports.dedup();
        let line = if ports.len() as u32 == left {
            t!("后台留着 {left} 个服务", left)
        } else {
            t!("后台留着 {left} 个进程", left)
        };
        Some(LeftBehind { line, ports })
    }
}
