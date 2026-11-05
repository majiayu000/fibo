mod agents;
mod background_shells;
mod codex_status;
mod command_marks;
pub mod devices;
mod diagnostics;
mod file_events;
mod handoff;
mod host;
pub mod log_index;
mod official;
pub mod plugin_hooks;
mod process_exit;
mod program_status;
mod pty;
pub mod recognition;
pub mod redact;
pub mod screen_ask;
pub mod screen_rules;
mod store;
pub mod tool_text;
pub use agents::hooks;
pub use agents::{
    adapter as agent_adapter, describe_tool, reads_tools, roots as agent_log_roots, tool_call_line,
    tool_result_line, usage_line, usage_marker, Described, ToolCallLine, ToolOutput, UsageState,
};
pub use host::remote::{load as remote_config, Config as RemoteConfig};
pub use host::{serve, serve_taking_over, serve_with_options, serve_with_roots};
pub const PROTOCOL_MAJOR: u32 = mooring_core::protocol::MAJOR;
pub const PROTOCOL_MINOR: u32 = 1;

pub mod content;

/// Starts writing out the conversations declared CLIs keep in SQLite, so the host reads
/// them like any transcript. Only the real daemon calls it; fixtures use their own roots.
pub fn start_mirrors() {
    if let Some(home) = std::env::var_os("HOME") {
        agents::mirror::sync(std::path::Path::new(&home));
    }
}
