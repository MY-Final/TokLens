//! Public installed-client data-directory environment contract.
use std::{ffi::OsString, path::PathBuf};

pub(super) fn database(
    minimax: Option<OsString>,
    mavis: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    // Electron data-dir.js and @mavis/config both trim the public overrides.
    let root = minimax
        .and_then(trimmed)
        .or_else(|| mavis.and_then(trimmed))
        .or_else(|| home.map(|home| home.join(".minimax")))?;
    Some(root.join("v2/sqlite/runtime-state.sqlite"))
}

fn trimmed(value: OsString) -> Option<PathBuf> {
    let value = value.to_string_lossy();
    let value = value.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}
