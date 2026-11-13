pub const HAS_COMMAND_KEY: bool = true;
pub const DRAWS_WINDOW_BUTTONS: bool = false;
pub const FILE_MANAGER: &str = "访达";
pub const FILE_MANAGER_EN: &str = "Finder";

/// TextEdit, or whatever the account set for plain text.
pub fn open_as_text(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg("-t")
        .arg(path)
        .spawn()
        .map(drop)
}

/// In the app bundle, under /Applications or ~/Applications.
pub fn editor_cli(editor: super::Editor) -> Option<std::path::PathBuf> {
    use super::Editor;
    let (bundles, tool): (&[&str], &str) = match editor {
        Editor::Zed => (&["Zed.app", "Zed Preview.app"], "Contents/MacOS/cli"),
        Editor::VsCode => (
            &["Visual Studio Code.app"],
            "Contents/Resources/app/bin/code",
        ),
        Editor::Cursor => (&["Cursor.app"], "Contents/Resources/app/bin/cursor"),
    };
    let home = crate::dirs::home().map(|h| h.join("Applications"));
    [Some(std::path::PathBuf::from("/Applications")), home]
        .into_iter()
        .flatten()
        .flat_map(|dir| bundles.iter().map(move |b| dir.join(b).join(tool)))
        .find(|p| p.is_file())
}

/// The app posts its own through UserNotifications.
pub fn notify(_title: &str, _body: &str, _sound: bool) -> std::io::Result<()> {
    Ok(())
}

pub fn reduce_motion() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
}

/// Through CoreText, which then sees the face for its cascade.
pub fn register_font(bytes: &'static [u8]) -> Result<(), &'static str> {
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGDataProviderCreateWithData(
            info: *mut c_void,
            data: *const c_void,
            size: usize,
            release: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize)>,
        ) -> *const c_void;
        fn CGFontCreateWithDataProvider(provider: *const c_void) -> *const c_void;
        fn CGDataProviderRelease(provider: *const c_void);
        fn CGFontRelease(font: *const c_void);
    }
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerRegisterGraphicsFont(
            font: *const c_void,
            error: *mut *const c_void,
        ) -> bool;
    }
    // Static bytes outlive the provider. CoreText retains its registered font.
    unsafe {
        let provider = CGDataProviderCreateWithData(
            std::ptr::null_mut(),
            bytes.as_ptr().cast(),
            bytes.len(),
            None,
        );
        if provider.is_null() {
            return Err("cannot create symbol font provider");
        }
        let font = CGFontCreateWithDataProvider(provider);
        CGDataProviderRelease(provider);
        if font.is_null() {
            return Err("cannot decode bundled symbol font");
        }
        let registered = CTFontManagerRegisterGraphicsFont(font, std::ptr::null_mut());
        CGFontRelease(font);
        if !registered {
            return Err("cannot register bundled symbol font");
        }
    }
    Ok(())
}

/// The account's languages in order (System Settings › Language & Region), as BCP 47 tags.
pub fn preferred_languages() -> Vec<String> {
    use std::ffi::{c_char, c_void};
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        fn CFRelease(cf: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;
    let mut out = vec![];
    // The array is ours to release; its strings belong to it.
    unsafe {
        let array = CFLocaleCopyPreferredLanguages();
        if array.is_null() {
            return out;
        }
        for i in 0..CFArrayGetCount(array) {
            let mut buffer = [0 as c_char; 64];
            let s = CFArrayGetValueAtIndex(array, i);
            if !s.is_null() && CFStringGetCString(s, buffer.as_mut_ptr(), 64, UTF8) {
                out.push(
                    std::ffi::CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        CFRelease(array);
    }
    out
}
