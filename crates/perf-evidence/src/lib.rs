// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Report-only resource measurements and authenticated JUnit inventory evidence.
//!
//! The library owns the SHA-256, XML and resource-accounting implementations and
//! their synthetic contracts. CLI launchers share that implementation regardless
//! of whether Cargo builds the launchers' test harnesses.

pub mod junit;
pub mod sample;

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
///
/// # Errors
/// Refuses directory creation, serialization, write, synchronization or publication
/// failures with the destination path and underlying cause.
pub fn write_json_atomic(path: &Path, value: &serde_json::Value) -> PerfResult<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create output directory {}: {error}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary evidence for {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .map_err(|error| format!("serialize evidence {}: {error}", path.display()))?;
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
mod tests {
    use super::*;

    #[test]
    fn atomic_json_replaces_complete_evidence_and_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/report.json");
        write_json_atomic(&path, &serde_json::json!({"revision": 1})).unwrap();
        let replacement = serde_json::json!({"revision": 2, "accepted": false});
        write_json_atomic(&path, &replacement).unwrap();
        let mut expected = serde_json::to_vec_pretty(&replacement).unwrap();
        expected.push(b'\n');
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn failed_atomic_publication_preserves_destination_and_removes_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        fs::create_dir(&path).unwrap();
        let prior = path.join("prior-evidence");
        fs::write(&prior, b"preserved").unwrap();
        let error = write_json_atomic(&path, &serde_json::json!({"accepted": false})).unwrap_err();
        assert!(error.to_string().contains("publish evidence"), "{error}");
        assert!(
            error.to_string().contains(&path.display().to_string()),
            "{error}"
        );
        assert_eq!(fs::read(prior).unwrap(), b"preserved");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
