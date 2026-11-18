use std::io;

pub(super) fn password(_service: &str, _interactive: bool) -> io::Result<Option<Vec<u8>>> {
    // Claude Code keeps its login in its own credentials file on these platforms.
    Ok(None)
}
