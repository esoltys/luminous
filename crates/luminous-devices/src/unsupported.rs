//! Platforms Luminous does not ship on: no devices are ever detected.

use crate::{EjectError, Volume};
use std::path::Path;

pub(crate) struct Watcher;

impl Watcher {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn poll(&mut self) -> Vec<Volume> {
        Vec::new()
    }
}

pub(crate) fn space(_path: &Path) -> Option<(u64, u64)> {
    None
}

pub(crate) fn eject(_volume: &Volume) -> Result<(), EjectError> {
    Err(EjectError::Failed("not supported on this platform".into()))
}
