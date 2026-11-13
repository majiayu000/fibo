//! AppKit, through `native.m`. Every entry point runs on the GPUI/main thread.
use std::ffi::{CString, c_char};

fn cstring(value: &str) -> CString {
    CString::new(value.replace('\0', " ")).expect("removed NUL")
}

unsafe extern "C" {
    fn kl_desktop_route_applied(
        session: *const c_char,
        terminal: *const c_char,
        tab: *const c_char,
    );
    fn kl_desktop_install(callback: extern "C" fn(*const c_char));
    fn kl_desktop_update(json: *const c_char);
    fn kl_notifications_enable();
    fn kl_desktop_show();
    fn kl_request_attention();
    fn kl_set_dock_icon(png: *const u8, length: usize);
    fn kl_desktop_notify(
        session: *const c_char,
        title: *const c_char,
        body: *const c_char,
        sound: i32,
    );
    fn kl_play_sound();
    fn kl_desktop_withdraw(session: *const c_char);
}

pub fn set_dock_icon(png: &[u8]) {
    unsafe { kl_set_dock_icon(png.as_ptr(), png.len()) }
}
pub fn route_applied(session: &str, terminal: &str, tab: &str) {
    unsafe {
        kl_desktop_route_applied(
            cstring(session).as_ptr(),
            cstring(terminal).as_ptr(),
            cstring(tab).as_ptr(),
        );
    }
}
pub fn show() {
    unsafe { kl_desktop_show() }
}
pub fn request_attention() {
    unsafe { kl_request_attention() }
}
pub fn install(callback: extern "C" fn(*const c_char)) {
    unsafe { kl_desktop_install(callback) }
}
pub fn update(json: &str) {
    unsafe { kl_desktop_update(cstring(json).as_ptr()) }
}
pub fn notifications_enable() {
    unsafe { kl_notifications_enable() }
}
pub fn withdraw(session: &str) {
    unsafe { kl_desktop_withdraw(cstring(session).as_ptr()) }
}
pub fn notify(session: &str, title: &str, body: &str, sound: bool) {
    unsafe {
        kl_desktop_notify(
            cstring(session).as_ptr(),
            cstring(title).as_ptr(),
            cstring(body).as_ptr(),
            i32::from(sound),
        );
    }
}
pub fn play_sound() {
    unsafe { kl_play_sound() }
}
#[cfg(feature = "visual-test")]
pub fn snapshot(path: &std::path::Path) {
    unsafe extern "C" {
        fn kl_desktop_snapshot(path: *const c_char) -> bool;
    }
    assert!(
        unsafe { kl_desktop_snapshot(cstring(&path.to_string_lossy()).as_ptr()) },
        "native menu panel export"
    );
}
