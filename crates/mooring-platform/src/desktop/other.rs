pub const HAS_COMMAND_KEY: bool = false;
pub const DRAWS_WINDOW_BUTTONS: bool = cfg!(windows);

/// Not read on these systems yet.
pub fn reduce_motion() -> bool {
    false
}

/// Nothing yet: a symbol this leaves out shows as a blank box, and nothing else depends on it.
pub fn register_font(_bytes: &'static [u8]) -> Result<(), &'static str> {
    Ok(())
}
