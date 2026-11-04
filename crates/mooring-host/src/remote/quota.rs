//! `quota`: the account limits each Agent last reported, as the window's quota button
//! shows them (`mooring_core::quota::latest`: readings at most a week old).
use mooring_core::protocol::*;
use mooring_core::quota;
use mooring_remote::protocol as rp;
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
    let now = mooring_core::content::now();
    let agents = quota::latest(&w, &plugins, now)
        .into_iter()
        .map(|(agent, q)| rp::AgentQuota {
            agent: crate::control::agent_label(agent),
            plan: q.plan.clone(),
            at: q.at,
            windows: (q.windows.iter())
                .map(|w| rp::QuotaWindow {
                    minutes: w.minutes,
                    used: w.used,
                    resets_at: w.resets_at,
                })
                .collect(),
        })
        .collect();
    Ok(json!(rp::Quotas { agents }))
}
