//! Nothing here has a menu bar, notifications or a Dock icon of ours yet.
use std::ffi::c_char;

pub fn set_dock_icon(_png: &[u8]) {}
pub fn route_applied(_session: &str, _terminal: &str, _tab: &str) {}
pub fn show() {}
pub fn request_attention() {}
pub fn install(_callback: extern "C" fn(*const c_char)) {}
pub fn update(_json: &str) {}
pub fn notifications_enable() {}
pub fn withdraw(_session: &str) {}
pub fn notify(_session: &str, _title: &str, _body: &str, _sound: bool) {}
pub fn play_sound() {}
#[cfg(feature = "visual-test")]
pub fn snapshot(_path: &std::path::Path) {}
