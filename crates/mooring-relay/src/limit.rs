//! Per-address limits, so one address cannot fill a public relay: at most `open` sockets
//! at once, and at most `per_minute` new ones in any minute. Machines and phones count
//! alike; a socket's [`Slot`] gives its place back when the socket ends.
//!
//! Behind a proxy every socket comes from the proxy's address. The real one is then read
//! from `X-Real-IP` (or the last `X-Forwarded-For` hop) when the peer is loopback, or
//! always when `trust_proxy` is set, which is right only when the relay's port is
//! reachable through the proxy alone (a container port published on 127.0.0.1 shows
//! the bridge's address, not loopback).
//!
//! An IPv6 address counts under its /64: one home or phone gets a whole /64 and can pick
//! a new source address in it for every socket.
use axum::http::HeaderMap;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Rules {
    /// Sockets one address may hold open; 0 means no limit.
    pub open: usize,
    /// New sockets one address may open per minute; 0 means no limit.
    pub per_minute: u32,
    pub trust_proxy: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            open: 64,
            per_minute: 120,
            trust_proxy: false,
        }
    }
}

#[derive(Default)]
struct Entry {
    open: usize,
    since: Option<Instant>,
    opened: u32,
}

pub struct Limits {
    rules: Rules,
    ips: Arc<Mutex<HashMap<IpAddr, Entry>>>,
    /// When entries of addresses gone quiet were last dropped.
    swept: Mutex<Instant>,
}

/// One admitted socket; dropping it frees its place.
pub struct Slot {
    ip: IpAddr,
    ips: Arc<Mutex<HashMap<IpAddr, Entry>>>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut ips = self.ips.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = ips.get_mut(&self.ip) {
            e.open = e.open.saturating_sub(1);
            let idle = e.since.is_none_or(|s| s.elapsed() >= MINUTE);
            if e.open == 0 && idle {
                ips.remove(&self.ip);
            }
        }
    }
}

const MINUTE: Duration = Duration::from_secs(60);

impl Limits {
    pub fn new(rules: Rules) -> Self {
        Self {
            rules,
            ips: Arc::default(),
            swept: Mutex::new(Instant::now()),
        }
    }

    /// The address a socket is counted under.
    pub fn address(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        let peer = peer.ip();
        if !(self.rules.trust_proxy || peer.is_loopback()) {
            return peer;
        }
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        let forwarded = header("x-forwarded-for").and_then(|v| v.rsplit(',').next());
        header("x-real-ip")
            .or(forwarded)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(peer)
    }

    /// A place for one more socket from `ip`, or `None` when it is over a limit.
    pub fn admit(&self, ip: IpAddr) -> Option<Slot> {
        let ip = counted_as(ip);
        let rules = &self.rules;
        let mut ips = self.ips.lock().unwrap_or_else(|e| e.into_inner());
        // A socket that ends within a minute of its address's first leaves the entry
        // behind; once a minute those go, or every address ever seen stays in memory.
        let mut swept = self.swept.lock().unwrap_or_else(|e| e.into_inner());
        if swept.elapsed() >= MINUTE {
            ips.retain(|_, e| e.open > 0 || e.since.is_some_and(|s| s.elapsed() < MINUTE));
            *swept = Instant::now();
        }
        drop(swept);
        let e = ips.entry(ip).or_default();
        if e.since.is_none_or(|s| s.elapsed() >= MINUTE) {
            e.since = Some(Instant::now());
            e.opened = 0;
        }
        if (rules.open > 0 && e.open >= rules.open)
            || (rules.per_minute > 0 && e.opened >= rules.per_minute)
        {
            return None;
        }
        e.open += 1;
        e.opened += 1;
        Some(Slot {
            ip,
            ips: self.ips.clone(),
        })
    }
}

/// The key an address is counted under: an IPv6 address by its /64.
fn counted_as(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6((u128::from(v6) & !(u64::MAX as u128)).into()),
        },
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn an_address_gets_its_place_back_when_a_socket_ends() {
        let limits = Limits::new(Rules {
            open: 2,
            per_minute: 0,
            trust_proxy: false,
        });
        let a = limits.admit(ip("1.2.3.4")).unwrap();
        let _b = limits.admit(ip("1.2.3.4")).unwrap();
        assert!(limits.admit(ip("1.2.3.4")).is_none());
        assert!(
            limits.admit(ip("5.6.7.8")).is_some(),
            "others are not held up"
        );
        drop(a);
        assert!(limits.admit(ip("1.2.3.4")).is_some());
    }

    #[test]
    fn ipv6_addresses_count_by_their_64() {
        let limits = Limits::new(Rules {
            open: 1,
            per_minute: 0,
            trust_proxy: false,
        });
        let _a = limits.admit(ip("2001:db8:1:2::1")).unwrap();
        assert!(
            limits.admit(ip("2001:db8:1:2:ffff::9")).is_none(),
            "another address in the same /64 is the same place"
        );
        assert!(limits.admit(ip("2001:db8:1:3::1")).is_some());
        let _v4 = limits.admit(ip("1.2.3.4")).unwrap();
        assert!(
            limits.admit(ip("::ffff:1.2.3.4")).is_none(),
            "mapped v4 is v4"
        );
    }

    #[test]
    fn an_address_opens_only_so_many_sockets_a_minute() {
        let limits = Limits::new(Rules {
            open: 0,
            per_minute: 3,
            trust_proxy: false,
        });
        for _ in 0..3 {
            drop(limits.admit(ip("1.2.3.4")).unwrap());
        }
        assert!(limits.admit(ip("1.2.3.4")).is_none());
    }

    #[test]
    fn addresses_gone_quiet_are_forgotten() {
        let limits = Limits::new(Rules::default());
        // Closed within its first minute: the entry outlives the socket.
        drop(limits.admit(ip("1.2.3.4")).unwrap());
        let _held = limits.admit(ip("5.6.7.8")).unwrap();
        assert_eq!(limits.ips.lock().unwrap().len(), 2);
        let long_ago = Instant::now() - 2 * MINUTE;
        for e in limits.ips.lock().unwrap().values_mut() {
            e.since = Some(long_ago);
        }
        *limits.swept.lock().unwrap() = long_ago;
        drop(limits.admit(ip("9.9.9.9")).unwrap());
        let ips = limits.ips.lock().unwrap();
        assert!(
            !ips.contains_key(&ip("1.2.3.4")),
            "a quiet address is dropped"
        );
        assert!(
            ips.contains_key(&ip("5.6.7.8")),
            "one with a socket open stays"
        );
    }

    #[test]
    fn only_a_proxy_on_this_host_or_a_trusted_one_names_the_real_address() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.9.9.9".parse().unwrap());
        let open = Limits::new(Rules::default());
        let outside: SocketAddr = "8.8.8.8:5000".parse().unwrap();
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let bridge: SocketAddr = "172.17.0.1:5000".parse().unwrap();
        assert_eq!(open.address(outside, &headers), ip("8.8.8.8"));
        assert_eq!(open.address(local, &headers), ip("9.9.9.9"));
        let trusted = Limits::new(Rules {
            trust_proxy: true,
            ..Rules::default()
        });
        assert_eq!(trusted.address(bridge, &headers), ip("9.9.9.9"));
        headers.remove("x-real-ip");
        headers.insert("x-forwarded-for", "6.6.6.6, 7.7.7.7".parse().unwrap());
        assert_eq!(trusted.address(bridge, &headers), ip("7.7.7.7"));
    }
}
