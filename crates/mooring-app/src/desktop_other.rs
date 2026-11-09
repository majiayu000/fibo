//! Nothing here has a menu bar or a Dock icon of ours yet; notifications go through the
//! system's own center, without a way back to the session they name.
use std::ffi::c_char;

pub fn set_dock_icon(_png: &[u8]) {}
pub fn route_applied(_session: &str, _terminal: &str, _tab: &str) {}
pub fn show() {}
pub fn request_attention() {}
pub fn install(_callback: extern "C" fn(*const c_char)) {}
pub fn update(_json: &str) {}
pub fn notifications_enable() {}
pub fn withdraw(_session: &str) {}
pub fn notify(_session: &str, title: &str, body: &str, sound: bool) {
    if let Err(e) = mooring_platform::desktop::notify(title, body, sound) {
        eprintln!("fibo: notification not shown: {e}");
    }
}
pub fn play_sound() {}
#[cfg(feature = "visual-test")]
pub fn snapshot(_path: &std::path::Path) {}
