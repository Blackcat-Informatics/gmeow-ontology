// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The two passthrough surfaces: `gmeow gts` (a shim to the external `gts`
//! binary) and `gmeow music` (the native `gmeow_music` engine).

use std::path::{Path, PathBuf};

use crate::{MusicCommands, BUNDLE_GTS};

/// The install hint printed when the external `gts` binary cannot be found.
pub(crate) const GTS_INSTALL_HINT: &str =
    "gts binary not found. Install gmeow-gts: pip install gmeow-gts \
     (or cargo install gmeow-gts, etc.), or set GMEOW_GTS_BIN to its path.";

/// The GTS subcommands that expect a snapshot file argument — the bundled
/// snapshot is injected for these when the user gives none.
const FILE_SUBCOMMANDS: &[&str] = &["info", "verify", "ls", "fold", "extract-key"];

/// Resolve the external `gts` binary: `GMEOW_GTS_BIN` wins, then a `gts` on
/// `PATH`. Returns `None` when neither resolves (the caller HARD-FAILS).
pub(crate) fn resolve_gts_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("GMEOW_GTS_BIN") {
        let path = PathBuf::from(&explicit);
        if !explicit.is_empty() && path.is_file() {
            return Some(path);
        }
    }
    which("gts")
}

/// Locate `name` on `PATH` (the executable-search fallback; no external crate).
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `gmeow gts …` — forward arguments verbatim to the external `gts` binary,
/// injecting the bundled snapshot path for file-expecting subcommands. Propagates
/// the child's exit code; HARD-FAILS when the binary is absent.
pub(crate) fn gts(args: &[String]) -> i32 {
    let Some(exe) = resolve_gts_binary() else {
        eprintln!("{GTS_INSTALL_HINT}");
        return 1;
    };

    let mut forwarded: Vec<String> = args.to_vec();
    // Keep the embedded snapshot staged for the child's lifetime.
    let mut staged: Option<StagedBundle> = None;

    if forwarded.is_empty() {
        forwarded.push("--help".to_owned());
    } else if FILE_SUBCOMMANDS.contains(&forwarded[0].as_str()) {
        let tail = &forwarded[1..];
        let has_file_arg = if let Some(marker) = tail.iter().position(|a| a == "--") {
            marker + 1 < tail.len()
        } else {
            tail.iter().any(|a| !a.starts_with('-'))
        };
        if !has_file_arg {
            match StagedBundle::write(BUNDLE_GTS) {
                Ok(s) => {
                    let path = s.path().to_string_lossy().into_owned();
                    staged = Some(s);
                    forwarded.insert(1, path);
                }
                Err(e) => {
                    eprintln!("cannot stage bundled snapshot: {e}");
                    return 1;
                }
            }
        }
    }

    let status = std::process::Command::new(&exe).args(&forwarded).status();
    drop(staged);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("failed to run gts: {e}");
            1
        }
    }
}

/// `gmeow music …` — the native music-package projection engine.
pub(crate) fn music(command: &MusicCommands) -> i32 {
    let result = match command {
        MusicCommands::Render { source, to, out } => {
            gmeow_music::render_file(source, &to.to_lowercase(), out)
        }
        MusicCommands::Import { source, out } => gmeow_music::import_file(source, out),
    };
    match result {
        Ok(paths) => {
            for path in paths {
                println!("wrote {}", path.display());
            }
            0
        }
        Err(message) => {
            // Mirror `ext/music/cli.py`: an unsupported-format ValueError maps to a
            // usage error (exit 2); any other failure is a runtime error (exit 1).
            let code = if message.starts_with("unsupported format:")
                || message.starts_with("MusicXML import only supports")
            {
                2
            } else {
                1
            };
            eprintln!("Error: {message}");
            code
        }
    }
}

/// A scoped temp copy of the embedded bundle, removed on drop.
struct StagedBundle {
    path: PathBuf,
}

impl StagedBundle {
    fn write(bytes: &[u8]) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "gmeow-gts-{}-{}.gts",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        path.push(unique);
        std::fs::write(&path, bytes)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedBundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
