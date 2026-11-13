//! `quota`: the account limits each Agent last reported, as the window's quota button
//! shows them (`fibo_core::quota::latest`: readings at most a week old).
use fibo_core::protocol::*;
use fibo_core::quota;
use fibo_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

pub(super) fn quota(socket: &Path) -> io::Result<Value> {
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    // Plugins were found at start (`agent::install`); their quota sources load once here.
    quota::install();
    let plugins = quota::from_plugins();
    let now = fibo_core::content::now();
    let agents = quota::latest(&w, &plugins, now)
        .into_iter()
        .map(|(agent, q)| reading(agent, &q))
        .collect();
    Ok(json!(rp::Quotas { agents }))
}

fn reading(agent: fibo_core::agent::Agent, q: &quota::Quota) -> rp::AgentQuota {
    rp::AgentQuota {
        agent: crate::control::agent_label(agent),
        agent_name: agent.name().to_owned(),
        plan: q.plan.clone(),
        at: q.at,
        fresh_until: Some(q.fresh_until()),
        // The account's own: a phone has one 本周 per Agent and keys its rows by length.
        windows: (q.by_usage().into_iter())
            .filter(|w| w.name.is_empty())
            .map(|w| rp::QuotaWindow {
                minutes: w.minutes,
                level: w.level().to_owned(),
                used: w.used,
                resets_at: w.resets_at,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_core::{
        agent::Agent,
        quota::{Quota, Window},
    };

    #[test]
    fn phone_quota_receives_core_order_levels_and_agent_name() {
        let q = Quota {
            windows: vec![
                Window {
                    minutes: 300,
                    used: 50,
                    ..Default::default()
                },
                Window {
                    minutes: 10080,
                    used: 80,
                    ..Default::default()
                },
                Window {
                    minutes: 43200,
                    used: 95,
                    ..Default::default()
                },
                Window {
                    minutes: 10080,
                    used: 99,
                    name: "one model".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let sent = reading(Agent::Dsh, &q);
        let wire: rp::AgentQuota = serde_json::from_value(json!(sent)).unwrap();
        assert_eq!(wire.agent_name, Agent::Dsh.name());
        assert_eq!(wire.fresh_until, Some(q.fresh_until()));
        assert_eq!(
            wire.windows
                .iter()
                .map(|w| (w.used, w.level.as_str()))
                .collect::<Vec<_>>(),
            [(95, "high"), (80, "mid"), (50, "low")]
        );
        assert_eq!(wire.windows[0].minutes, 43200);
        let old: rp::AgentQuota = serde_json::from_value(json!({ "agent":"codex", "plan":"", "at":0, "windows":[{"minutes":300,"used":81,"resets_at":0}] })).unwrap();
        assert!(old.windows[0].level.is_empty());
    }
}
