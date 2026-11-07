use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GetFileInformationByHandle,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Needs Developer Mode or the right to make links; without them this reports the error.
pub fn link_dir(to: &Path, at: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(to, at)
}

/// Where a Windows install puts the command is not decided yet (docs/windows/plan.md).
pub fn link_program(_to: &Path, _at: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows 上还不能把 mooring 放到 PATH",
    ))
}

/// Not private by itself: it takes the rights of the directory it is in, which under the user
/// profile means this account, SYSTEM and administrators. Elsewhere it may mean more people.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Takes the rights of its directory, as `create_private_dir` says.
pub fn canonical(path: &Path) -> io::Result<std::path::PathBuf> {
    Ok(without_verbatim(&std::fs::canonicalize(path)?))
}

fn without_verbatim(path: &Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}").into(),
        None => text
            .strip_prefix(r"\\?\")
            .map_or_else(|| path.to_owned(), Into::into),
    }
}

pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, bytes)?;
    let renamed = std::fs::rename(&temp, path);
    if renamed.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    renamed
}

/// std cannot count a file's names here; such a file loses its other names when replaced.
pub fn hard_linked(_path: &Path) -> bool {
    false
}

/// Files take the rights of their directory; there is no mode to narrow at creation.
pub fn create_no_wider(_options: &mut std::fs::OpenOptions, _than: &std::fs::Permissions) {}

pub fn file_id(path: &Path, _metadata: &std::fs::Metadata) -> io::Result<super::FileId> {
    use std::os::windows::fs::OpenOptionsExt;
    // Nothing to read or write, shared with everyone: only to ask, and it holds nothing up.
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    open_file_id(&file)
}

pub fn open_file_id(file: &std::fs::File) -> io::Result<super::FileId> {
    use std::os::windows::io::AsRawHandle;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is open, and the info is written by the call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(super::FileId {
        device: u64::from(info.dwVolumeSerialNumber),
        inode: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

/// It takes the rights of its folder: `secure_dir` is what keeps others out.
pub fn private_file() -> std::fs::OpenOptions {
    std::fs::OpenOptions::new()
}

pub fn lock_file(path: &Path) -> io::Result<std::fs::File> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(io::Error::other("a lock file must not be a link"));
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// This account and nothing else, with what the parent grants cut off. System services and
/// administrators can take a folder back regardless, as root can on Unix.
pub fn secure_dir(dir: &Path) -> io::Result<()> {
    match std::fs::create_dir(dir) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::other(
            "the runtime folder must be a folder, not a link",
        ));
    }
    let sid = account_sid()?;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let descriptor: Vec<u16> = format!("D:P(A;OICI;FA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: the strings are NUL-terminated; what the conversion allocates is freed below, and
    // the DACL pointer is into it.
    unsafe {
        let mut made: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            SDDL_REVISION_1,
            &mut made,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let (mut present, mut defaulted) = (0, 0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let got = GetSecurityDescriptorDacl(made, &mut present, &mut dacl, &mut defaulted);
        let set = if got == 0 {
            io::Error::last_os_error().raw_os_error().unwrap_or(1) as u32
        } else {
            SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        LocalFree(made as _);
        if set != 0 {
            return Err(io::Error::from_raw_os_error(set as i32));
        }
    }
    Ok(())
}

/// This process's account as `S-1-5-...`.
fn account_sid() -> io::Result<String> {
    // SAFETY: the token is closed and the string freed on every path; the second call fills
    // what the first one sized.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut size = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0u8; size as usize];
        let read = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let mut text: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let length = (0..).take_while(|i| *text.add(*i) != 0).count();
        let out = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        LocalFree(text as _);
        Ok(out)
    }
}
