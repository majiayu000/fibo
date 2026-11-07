/// Not read on these systems yet.
pub fn reduce_motion() -> bool {
    false
}

/// Nothing yet: a symbol this leaves out shows as a blank box, and nothing else depends on it.
pub fn register_font(_bytes: &'static [u8]) -> Result<(), &'static str> {
    Ok(())
}
