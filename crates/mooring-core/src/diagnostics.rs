//! A short, read-only host measurement. These observations are not workspace facts.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Write};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    pub started_at: u64,
    pub elapsed_secs: f64,
    pub complete: bool,
    pub process_count: usize,
    /// Logical cores, so the view can say how many of them the ranking explains.
    #[serde(default)]
    pub cpu_cores: Option<usize>,
    /// All cores together: 100% means every core busy.
    pub cpu_percent: Option<f64>,
    /// Latest device reading, not a sum of the per-app shares below.
    pub gpu_percent: Option<f64>,
    /// Latest whole-machine memory reading.
    #[serde(default)]
    pub memory: Option<Memory>,
    pub apps: Vec<AppUsage>,
    /// Memory of each managed terminal's whole process tree: the Agent and its children.
    #[serde(default)]
    pub terminals: Vec<TerminalUsage>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TerminalUsage {
    pub id: String,
    pub memory_bytes: u64,
    pub processes: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppUsage {
    pub name: String,
    pub cpu_percent: f64,
    /// Peak of the measured intervals, not an instantaneous peak.
    pub cpu_peak_percent: f64,
    /// Physical footprint, the number Activity Monitor's Memory column shows. It counts
    /// compressed pages at their original size, so apps can add up to more than used;
    /// processes of other users fall back to RSS.
    #[serde(alias = "rss_bytes")]
    pub memory_bytes: u64,
    pub processes: usize,
    /// Share of observed GPU counter growth; never a device utilization percentage.
    pub gpu_share_percent: Option<f64>,
    pub terminals: Vec<String>,
}

/// Physical memory as Activity Monitor counts it: used is app memory, wired and compressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub compressed_bytes: u64,
    #[serde(default)]
    pub wired_bytes: u64,
    /// File cache and purgeable pages: given back as soon as anything needs them.
    #[serde(default)]
    pub cached_bytes: u64,
    #[serde(default)]
    pub swap_bytes: u64,
    #[serde(default)]
    pub pressure: Option<Pressure>,
}

/// The kernel's memory pressure, as Activity Monitor's pressure graph colors it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pressure {
    Normal,
    Warn,
    Critical,
}
impl Pressure {
    /// `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warn, 4 critical.
    pub fn from_level(level: i32) -> Option<Self> {
        match level {
            1 => Some(Self::Normal),
            2 => Some(Self::Warn),
            4 => Some(Self::Critical),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => crate::t!("正常"),
            Self::Warn => crate::t!("偏紧"),
            Self::Critical => crate::t!("紧张"),
        }
    }
}
impl Memory {
    /// App memory: used minus wired and compressed.
    pub fn app_bytes(&self) -> u64 {
        self.used_bytes
            .saturating_sub(self.wired_bytes)
            .saturating_sub(self.compressed_bytes)
    }
    pub fn percent(&self) -> Option<f64> {
        (self.total_bytes > 0).then(|| self.used_bytes as f64 / self.total_bytes as f64 * 100.)
    }
}
/// Bytes in GB with one decimal, as Activity Monitor shows memory.
pub fn gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1024_f64.powi(3))
}

/// How many apps the ranking shows; the summary, the view and the core stack share it.
pub const RANKED: usize = 20;

impl Diagnostics {
    /// Apps with measured CPU, highest first, as ranked everywhere.
    pub fn ranked(&self) -> impl Iterator<Item = &AppUsage> {
        self.apps.iter().filter(|a| a.cpu_percent > 0.).take(RANKED)
    }
    /// Apps by memory, largest first, as the memory ranking shows them.
    pub fn memory_ranked(&self) -> Vec<&AppUsage> {
        let mut apps: Vec<_> = self.apps.iter().filter(|a| a.memory_bytes > 0).collect();
        apps.sort_by(|a, b| {
            b.memory_bytes
                .cmp(&a.memory_bytes)
                .then_with(|| a.name.cmp(&b.name))
        });
        apps.truncate(RANKED);
        apps
    }
    /// Every app's memory added up; more than used when pages are compressed.
    pub fn apps_memory(&self) -> u64 {
        self.apps.iter().map(|a| a.memory_bytes).sum()
    }
    /// Cores busy outside the ranked apps: apps below the cut, or processes that started
    /// and exited between samples. None when the core count or machine CPU is unreadable.
    pub fn unattributed_cores(&self) -> Option<f64> {
        let busy = self.cpu_percent? / 100. * self.cpu_cores? as f64;
        let listed: f64 = self.ranked().map(|a| a.cpu_percent).sum::<f64>() / 100.;
        Some((busy - listed).max(0.))
    }
    /// The text handed to an Agent. `started` is the local start time; `titles` names the
    /// workbench terminals so the Agent reads a tab title, not an internal id.
    pub fn summary(&self, started: &str, titles: &HashMap<String, String>) -> String {
        let mut text = t!(
            "本机资源诊断 · 开始于 {started} · 采样 {secs} 秒 · {status}\n",
            started,
            secs = format!("{:.1}", self.elapsed_secs),
            status = if self.complete {
                t!("采样结束")
            } else {
                t!("采样中（部分结果）")
            },
        );
        let percent =
            |p: Option<f64>| p.map_or_else(|| t!("不可读#diag").into(), |v| format!("{v:.1}%"));
        let _ = writeln!(
            text,
            "{}",
            t!(
                "进程数：{procs}；CPU 核心：{cores}",
                procs = self.process_count,
                cores = self
                    .cpu_cores
                    .map_or_else(|| t!("不可读#diag").into(), |c| c.to_string()),
            )
        );
        let _ = writeln!(
            text,
            "{}",
            t!(
                "整机 CPU 平均：{cpu}（所有核心合计）；最新 GPU 设备活跃度：{gpu}",
                cpu = percent(self.cpu_percent),
                gpu = percent(self.gpu_percent),
            )
        );
        if let Some(m) = self.memory {
            let _ = writeln!(
                text,
                "{}",
                t!(
                    "内存：已用 {used} / {total}（{pct}，其中压缩 {comp}）；交换 {swap}；内存压力 {pressure}",
                    used = gib(m.used_bytes),
                    total = gib(m.total_bytes),
                    pct = percent(m.percent()),
                    comp = gib(m.compressed_bytes),
                    swap = gib(m.swap_bytes),
                    pressure = m.pressure.map_or(t!("不可读#diag"), Pressure::label),
                )
            );
        }
        let listed: f64 = self.ranked().map(|a| a.cpu_percent).sum::<f64>() / 100.;
        if let (Some(cores), Some(rest)) = (self.cpu_cores, self.unattributed_cores()) {
            let _ = writeln!(
                text,
                "{}",
                t!(
                    "排行前 {count} 项合计约 {listed} 核；整机忙碌中约 {rest} / {cores} 核没有归到任何应用（排行之外或采样间隙里的短命进程）。",
                    count = self.ranked().count(),
                    listed = format!("{listed:.1}"),
                    rest = format!("{rest:.1}"),
                    cores,
                )
            );
        }
        text.push_str(t!("\nCPU 排行（100% = 一个核心）\n"));
        for app in self.ranked() {
            text.push_str(&t!(
                "{name}：平均 {cpu}%",
                name = &app.name,
                cpu = format!("{:.1}", app.cpu_percent),
            ));
            // One interval gives a peak equal to the average; repeating it is noise.
            if app.cpu_peak_percent > app.cpu_percent + 0.05 {
                text.push_str(&t!(
                    "，区间峰值 {peak}%",
                    peak = format!("{:.1}", app.cpu_peak_percent),
                ));
            }
            text.push_str(&t!(
                "，内存 {mem}，{procs} 个进程",
                mem = mib(app.memory_bytes),
                procs = app.processes,
            ));
            let named: Vec<&str> = app
                .terminals
                .iter()
                .filter_map(|id| titles.get(id).map(String::as_str))
                .collect();
            if !named.is_empty() {
                text.push_str(&t!("，关联终端：{list}", list = named.join(t!("、"))));
            }
            let hidden = app.terminals.len() - named.len();
            if hidden > 0 {
                text.push_str(if named.is_empty() {
                    t!("，")
                } else {
                    t!("；")
                });
                text.push_str(&t!("另有 {hidden} 个终端未在工作台打开", hidden));
            }
            text.push('\n');
        }
        text.push_str(t!(
            "\n内存排行（活动监视器「内存」列口径，含被压缩部分的原始大小，合计会超过已用）\n"
        ));
        for app in self.memory_ranked().into_iter().take(10) {
            let _ = writeln!(
                text,
                "{}",
                t!(
                    "{name}：{mem}，{procs} 个进程",
                    name = &app.name,
                    mem = mib(app.memory_bytes),
                    procs = app.processes,
                )
            );
        }
        let mut terms: Vec<_> = self
            .terminals
            .iter()
            .filter_map(|t| titles.get(&t.id).map(|title| (title, t)))
            .collect();
        terms.sort_by_key(|t| std::cmp::Reverse(t.1.memory_bytes));
        if !terms.is_empty() {
            text.push_str(t!("按终端（整棵进程树，含 Agent 拉起的子进程）："));
            let list: Vec<String> = terms
                .iter()
                .map(|(title, t)| format!("{title} {}", mib(t.memory_bytes)))
                .collect();
            text.push_str(&list.join(t!("、")));
            text.push('\n');
        }
        text.push_str(t!(
            "\nGPU 活动排行（只占已观测计数增量的份额，不是整颗 GPU 利用率）\n"
        ));
        let gpu = self.gpu_ranked();
        if gpu.is_empty() {
            text.push_str(t!("没有可比较的 GPU 计数增量；不代表 GPU 空闲。\n"));
        }
        for app in gpu {
            let _ = writeln!(
                text,
                "{}",
                t!(
                    "{name}：已观测份额 {share}%",
                    name = &app.name,
                    share = format!("{:.1}", app.gpu_share_percent.unwrap_or_default()),
                )
            );
        }
        for note in &self.notes {
            let _ = writeln!(text, "{}", t!("说明：{note}", note));
        }
        text.push_str(t!(
            "\n请根据以上测量分析占用来源，区分已证实、推测与未知。需要解释根因时先收集对应日志、流量或调用栈，不要仅凭排行判断卡死或直接结束进程。\n"
        ));
        text
    }
    /// Apps with a GPU share, largest first, at most ten.
    pub fn gpu_ranked(&self) -> Vec<&AppUsage> {
        let mut gpu: Vec<_> = self
            .apps
            .iter()
            .filter(|a| a.gpu_share_percent.is_some())
            .collect();
        gpu.sort_by(|a, b| {
            b.gpu_share_percent
                .unwrap_or_default()
                .total_cmp(&a.gpu_share_percent.unwrap_or_default())
        });
        gpu.truncate(10);
        gpu
    }
}

/// Memory as people read it: MB below a gigabyte, one decimal of GB above.
pub fn mib(bytes: u64) -> String {
    let mib = bytes as f64 / 1_048_576.;
    if mib >= 1024. {
        format!("{:.1} GB", mib / 1024.)
    } else if mib >= 1000. {
        // Four-digit MB read as GB, so the value column stays one line.
        format!("{:.2} GB", mib / 1024.)
    } else {
        format!("{mib:.0} MB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, cpu: f64, terminals: &[&str]) -> AppUsage {
        AppUsage {
            name: name.into(),
            cpu_percent: cpu,
            cpu_peak_percent: cpu,
            memory_bytes: 2048 * 1_048_576,
            processes: 3,
            terminals: terminals.iter().map(|t| t.to_string()).collect(),
            ..AppUsage::default()
        }
    }

    #[test]
    fn summary_names_terminals_and_unattributed_cores() {
        let report = Diagnostics {
            cpu_cores: Some(16),
            cpu_percent: Some(100.),
            apps: vec![
                app("ffmpeg", 191.5, &[]),
                app("claude", 28.7, &["t1", "t2"]),
            ],
            ..Diagnostics::default()
        };
        let titles = HashMap::from([("t1".to_string(), "mooring".to_string())]);
        let text = report.summary("13:04:00", &titles);
        assert!(text.contains("开始于 13:04:00"));
        assert!(text.contains("关联终端：mooring；另有 1 个终端未在工作台打开"));
        assert!(!text.contains("t1"));
        assert!(!text.contains("区间峰值"), "equal peak is omitted");
        assert!(text.contains("约 13.8 / 16 核没有归到任何应用"));
        assert!(text.contains("内存 2.0 GB"));
        assert_eq!(
            report.unattributed_cores().map(|c| (c * 10.).round()),
            Some(138.)
        );
    }

    #[test]
    fn summary_reports_memory_only_when_read() {
        let gb = 1024_u64.pow(3);
        let mut report = Diagnostics::default();
        assert!(
            !report
                .summary("13:04:00", &HashMap::new())
                .contains("内存：")
        );
        report.memory = Some(Memory {
            total_bytes: 36 * gb,
            used_bytes: 18 * gb,
            compressed_bytes: gb / 2,
            wired_bytes: 2 * gb,
            swap_bytes: gb,
            pressure: Some(Pressure::Warn),
            ..Memory::default()
        });
        let text = report.summary("13:04:00", &HashMap::new());
        assert!(text.contains(
            "内存：已用 18.0 GB / 36.0 GB（50.0%，其中压缩 0.5 GB）；交换 1.0 GB；内存压力 偏紧"
        ));
        assert_eq!(report.memory.unwrap().app_bytes(), 15 * gb + gb / 2);
        assert_eq!(Memory::default().percent(), None);
    }

    #[test]
    fn memory_ranking_sorts_by_memory_and_names_terminals() {
        let mut big = app("Virtualization", 1., &[]);
        big.memory_bytes = 8 << 30;
        let report = Diagnostics {
            apps: vec![app("claude", 40., &["t1"]), big],
            terminals: vec![TerminalUsage {
                id: "t1".into(),
                memory_bytes: 3 << 30,
                processes: 4,
            }],
            ..Diagnostics::default()
        };
        let names: Vec<_> = report
            .memory_ranked()
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Virtualization", "claude"]);
        assert_eq!(report.apps_memory(), 10 << 30);
        let titles = HashMap::from([("t1".to_string(), "侧栏设计".to_string())]);
        assert!(
            report
                .summary("", &titles)
                .contains("按终端（整棵进程树，含 Agent 拉起的子进程）：侧栏设计 3.0 GB")
        );
        assert_eq!(Pressure::from_level(4), Some(Pressure::Critical));
        assert_eq!(Pressure::from_level(0), None);
        assert_eq!(mib(1012 * 1_048_576), "0.99 GB");
        assert_eq!(mib(512 * 1_048_576), "512 MB");
    }

    #[test]
    fn old_reports_with_rss_still_read() {
        let app: AppUsage = serde_json::from_str(
            r#"{"name":"a","cpu_percent":1,"cpu_peak_percent":1,"rss_bytes":5,"processes":1,"gpu_share_percent":null,"terminals":[]}"#,
        )
        .unwrap();
        assert_eq!(app.memory_bytes, 5);
    }

    #[test]
    fn unknown_core_count_leaves_no_attribution_claim() {
        let report = Diagnostics {
            cpu_percent: Some(100.),
            apps: vec![app("ffmpeg", 50., &[])],
            ..Diagnostics::default()
        };
        assert_eq!(report.unattributed_cores(), None);
        assert!(!report.summary("", &HashMap::new()).contains("没有归到"));
    }
}
