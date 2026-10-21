// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Report-only resource measurements and authenticated JUnit inventory evidence.
//!
//! The library owns the SHA-256, XML and resource-accounting implementations and
//! their synthetic contracts. CLI launchers share that implementation regardless
//! of whether Cargo builds the launchers' test harnesses.

pub mod junit;
pub mod sample;

use purrdf_lex::json::{self, Value};
use std::fs;
use std::io::Write as _;
use std::path::Path;

/// A failed evidence operation, retaining its operation and source detail.
#[derive(Debug)]
pub struct PerfError(String);

impl std::fmt::Display for PerfError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for PerfError {}

impl From<String> for PerfError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for PerfError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

/// Result shared by the resource, acceptance and JUnit evidence tools.
pub type PerfResult<T> = std::result::Result<T, PerfError>;

/// Publish pretty JSON with a terminal newline through a unique sibling file.
/// The completed file is synchronized before atomic replacement; a returned
/// failure drops its temporary file and leaves any prior destination intact.
/// Members are recursively sorted and repeated names retain their last value.
/// Parsed numbers retain the JSON owner's original lexemes.
///
/// # Errors
/// Refuses directory creation, write, synchronization or publication
/// failures with the destination path and underlying cause.
pub fn write_json_atomic(path: &Path, value: &Value) -> PerfResult<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create output directory {}: {error}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary evidence for {}: {error}", path.display()))?;
    let mut value = value.clone();
    json::record::sorted_last_wins(&mut value);
    temporary
        .write_all(json::write_pretty(&value).as_bytes())
        .map_err(|error| format!("write evidence {}: {error}", path.display()))?;
    temporary
        .write_all(b"\n")
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("flush evidence {}: {error}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| format!("publish evidence {}: {}", path.display(), error.error))?;
    Ok(())
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
