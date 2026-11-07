use super::Memory;
use std::io;

fn parse_memory(meminfo: &str) -> Option<Memory> {
    let field = |name: &str| -> Option<u64> {
        let line = meminfo
            .lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))?;
        Some(line.split_whitespace().next()?.parse::<u64>().ok()? * 1024)
    };
    let total = field("MemTotal").filter(|t| *t > 0)?;
    let available = field("MemAvailable")?;
    Some(Memory {
        total,
        used: total.saturating_sub(available),
        compressed: 0,
        wired: field("Mlocked").unwrap_or(0),
        cached: field("Cached").unwrap_or(0) + field("Buffers").unwrap_or(0),
        swap: field("SwapTotal")
            .unwrap_or(0)
            .saturating_sub(field("SwapFree").unwrap_or(0)),
        pressure: None,
    })
}

pub fn memory() -> Option<Memory> {
    parse_memory(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_ticks(stat: &str) -> Option<[u32; 4]> {
    let line = stat.lines().find_map(|l| l.strip_prefix("cpu "))?;
    let mut fields = line.split_whitespace().map(|f| f.parse::<u64>().ok());
    let (user, nice, system, idle) = (
        fields.next()??,
        fields.next()??,
        fields.next()??,
        fields.next()??,
    );
    Some([user as u32, system as u32, idle as u32, nice as u32])
}

pub fn cpu_ticks() -> io::Result<[u32; 4]> {
    parse_ticks(&std::fs::read_to_string("/proc/stat")?)
        .ok_or_else(|| io::Error::other("no cpu line in /proc/stat"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_read_from_its_named_lines() {
        let m = parse_memory(
            "MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 400 kB\nBuffers: 10 kB\n\
             Cached: 20 kB\nSwapCached: 5 kB\nSwapTotal: 50 kB\nSwapFree: 30 kB\nMlocked: 2 kB\n",
        )
        .unwrap();
        assert_eq!(m.total, 1000 * 1024);
        assert_eq!(m.used, 600 * 1024);
        assert_eq!(m.cached, 30 * 1024, "not SwapCached");
        assert_eq!(m.swap, 20 * 1024);
        assert_eq!(m.wired, 2 * 1024);
    }

    #[test]
    fn ticks_come_in_the_interfaces_order() {
        let stat = "cpu  1 2 3 4 5 6 7 8 0 0\ncpu0 1 2 3 4 5 6 7 8 0 0\n";
        assert_eq!(parse_ticks(stat), Some([1, 3, 4, 2]));
    }
}
