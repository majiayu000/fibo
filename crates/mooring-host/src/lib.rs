mod agents;
pub use agents::claude_quota;
mod background_shells;
mod cli_commands;
mod cli_http;
pub mod cli_setup;
mod codex_status;
mod command_marks;
pub mod devices;
mod diagnostics;
mod host;
pub mod log_index;
mod official;
pub mod plugin_hooks;
mod program_status;
mod pty;
pub mod recognition;
pub mod redact;
pub mod screen_ask;
pub mod screen_rules;
mod shell_log;
mod shell_match;
mod store;
pub mod tool_text;
pub use agents::calling_conversations;
pub use agents::hooks;
pub use agents::mirror;
pub use agents::modes_drive::switches as agent_modes_switch;
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
    if let Some(home) = mooring_platform::dirs::home() {
        agents::mirror::sync(&home);
    }
}
