// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Atomic publication of complete standalone artifacts.

use std::path::{Path, PathBuf};

/// Resolve existing parents and final symlinks, including a dangling link to a new output.
pub(crate) fn resolve_path(path: &Path) -> std::io::Result<PathBuf> {
    let mut selected = std::path::absolute(path)?;
    let mut seen = std::collections::HashSet::with_hasher(purrdf_hash::fixed::FixedState::new());
    loop {
        let name = selected.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "output needs a file name")
        })?;
        let parent = selected.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "output needs a parent directory",
            )
        })?;
        let parent = std::fs::canonicalize(parent)?;
        let candidate = parent.join(name);
        if !seen.insert(candidate.clone()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cyclic output symlink",
            ));
        }
        match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = std::fs::read_link(&candidate)?;
                selected = if target.is_absolute() {
                    target
                } else {
                    parent.join(target)
                };
            }
            Ok(_) => return std::fs::canonicalize(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> gmeow_errors::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "output needs a parent directory",
        )
    })?;
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Creation applies the caller's umask, just as an ordinary output write does.
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(parent)?;
    match std::fs::metadata(path) {
        Ok(metadata) => file.as_file().set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    file.write_all(bytes)?;
    file.flush()?;
    file.persist(path)?;
    Ok(())
}
