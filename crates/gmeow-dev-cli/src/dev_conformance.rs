// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev conformance-bless`: refresh the expected outputs of named conformance cases.
//!
//! Each named case is executed through the same `run_case` the producer's conformance stage
//! uses, and its `expected/` goldens are rewritten by the conformance crate's own writer. The
//! command reads only the case directory: it never runs the regeneration pipeline and needs no
//! producer output. A case that selects shipped rules needs the compiled rule library, which
//! this command does not load, so it is refused before it runs.
//!
//! Cases are always named. There is no "bless everything" form: a refreshed golden is a
//! deliberate, reviewed change to what the gate accepts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::bless::{self, Goldens};
use gmeow_conformance::error::{CaseAnatomy, Cli, Io};
use gmeow_conformance::vendored::{Lane, lane_for_case};
use gmeow_conformance::{discover, native_observation, run};
use gmeow_errors::Diag;

/// How one golden under a case's `expected/` tree differs after the refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// The file existed and its bytes changed.
    Changed,
    /// The file did not exist before the refresh.
    Created,
}

impl Change {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Created => "created",
        }
    }
}

/// The outcome of refreshing one case: its id and each golden that differs, in path order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BlessedCase {
    pub(crate) case_id: String,
    pub(crate) changes: Vec<(PathBuf, Change)>,
}

/// Refresh the named cases and print what changed. Returns the process exit code.
pub(crate) fn conformance_bless(cases: &[PathBuf], init: bool) -> i32 {
    let root = crate::dev_common::project_root();
    let goldens = if init {
        Goldens::All
    } else {
        Goldens::Existing
    };
    match bless_cases(&root, cases, goldens) {
        Ok(blessed) => {
            for case in &blessed {
                println!(
                    "{}: {} expected file(s) differ",
                    case.case_id,
                    case.changes.len()
                );
                for (path, change) in &case.changes {
                    println!("  {} {}", change.as_str(), path.display());
                }
            }
            0
        }
        Err(diag) => crate::dev_common::fail(format!("conformance-bless: {diag}")),
    }
}

/// Run every named case, then rewrite each one's goldens and report the files that differ.
///
/// Every case is resolved and executed before anything is written, so one failing case leaves
/// every golden untouched.
///
/// # Errors
/// Refuses an unknown or malformed case, an oracle-backed case, a case that selects shipped
/// rules, and any case whose run fails; propagates a failure to read or write a golden.
pub(crate) fn bless_cases(
    root: &Path,
    cases: &[PathBuf],
    goldens: Goldens,
) -> gmeow_errors::Result<Vec<BlessedCase>> {
    let library = run::RuleLibrary::default();
    let mut runs = Vec::with_capacity(cases.len());
    for name in cases {
        let case_dir = resolve_case(root, name)?;
        let case = discover::validate_case(&case_dir)?;
        let refuse = |reason: &str| {
            Diag::of_kind(Cli {
                detail: format!("case {} cannot be blessed here: {reason}", case.case_id),
            })
        };
        if lane_for_case(&case_dir)? == Some(Lane::B) {
            return Err(refuse(
                "it is oracle-backed, so its expected outputs are not produced by the native \
                 runner",
            ));
        }
        if case
            .profile
            .get("shipped_rules")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|rules| !rules.is_empty())
        {
            return Err(refuse(
                "its profile selects shipped_rules, which need the compiled rule library this \
                 command does not load",
            ));
        }
        let outputs = run::run_case(&case_dir, &library, &native_observation::read)?;
        runs.push((case_dir, case.case_id, outputs));
    }
    let mut blessed = Vec::with_capacity(runs.len());
    for (case_dir, case_id, outputs) in runs {
        let expected = case_dir.join("expected");
        let before = golden_bytes(&expected)?;
        bless::write_expected(&case_dir, &outputs, goldens)?;
        let changes = golden_bytes(&expected)?
            .into_iter()
            .filter_map(|(path, bytes)| match before.get(&path) {
                None => Some((path, Change::Created)),
                Some(earlier) if *earlier != bytes => Some((path, Change::Changed)),
                Some(_) => None,
            })
            .collect();
        blessed.push(BlessedCase { case_id, changes });
    }
    Ok(blessed)
}

/// Resolve a case named by path (absolute, or relative to the checkout) or by its
/// `<category>/<case>` id under `conformance/logic/cases`. Only a case inside that tree is
/// admitted, so a corpus kept elsewhere can never be rewritten from the engine's own output.
fn resolve_case(root: &Path, name: &Path) -> gmeow_errors::Result<PathBuf> {
    let unknown = || {
        Diag::of_kind(CaseAnatomy {
            detail: format!(
                "no conformance case named {}: expected a case directory holding profile.json \
                 under conformance/logic/cases, given as a path or as an id below that directory",
                name.display()
            ),
        })
    };
    let cases_root = root
        .join("conformance/logic/cases")
        .canonicalize()
        .map_err(|_| unknown())?;
    [root.join(name), cases_root.join(name)]
        .into_iter()
        .filter_map(|candidate| candidate.canonicalize().ok())
        .find(|candidate| {
            candidate.starts_with(&cases_root) && candidate.join("profile.json").is_file()
        })
        .ok_or_else(unknown)
}

/// Every file under `expected/`, keyed by its path relative to that directory.
fn golden_bytes(expected: &Path) -> gmeow_errors::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let unreadable = |path: &Path, error: std::io::Error| {
        Diag::of_kind(Io {
            detail: format!("read {}: {error}", path.display()),
        })
    };
    let mut files = BTreeMap::new();
    let mut pending = vec![expected.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        let entries = std::fs::read_dir(&directory).map_err(|e| unreadable(&directory, e))?;
        for entry in entries {
            let path = entry.map_err(|e| unreadable(&directory, e))?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|e| unreadable(&path, e))?;
            let relative = path.strip_prefix(expected).unwrap_or(&path).to_path_buf();
            files.insert(relative, bytes);
        }
    }
    Ok(files)
}

#[path = "dev_conformance.tests.rs"]
#[cfg(test)]
mod tests;
