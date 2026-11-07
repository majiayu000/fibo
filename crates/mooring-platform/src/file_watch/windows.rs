use super::Callback;
use std::{io, path::PathBuf, sync::Arc};

pub struct Watch;

pub fn current_id() -> u64 {
    0
}

impl Watch {
    pub fn start(_callback: Arc<Callback>, _since: u64) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "watching files is not there yet on Windows",
        ))
    }
    pub fn follow(&self, _paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
        Ok(vec![])
    }
}
