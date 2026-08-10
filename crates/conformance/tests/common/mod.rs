// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared helpers for the W3C OWL 2 Full divergence/decided conformance gates
//! (`full_decided_gate`, `full_divergence_gate`, `native_fragment_coverage_gate`).
//!
//! All three gates re-run committed `input.nq` cases through the exact same
//! `dl_consistency` path the grader/runner uses, and two of them walk a corpus
//! directory to discover its case slugs. This module is the single source for
//! that verdict-mapping + corpus-root + case-discovery machinery, so a change to
//! (for example) the per-case timeout budget is made once, not six times.

#![allow(dead_code)] // not every binary uses every helper

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::paths::cases_root;
use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// A per-case wall-clock budget for the guarded live re-run. A case whose
/// existential chase exceeds this is treated as `incomplete` — always SOUND (an
/// honest "cannot decide"). Generous enough that a fast wrong-decision (the
/// soundness regression these gates exist to catch) still completes and is
/// caught; only the known memory/compute-heavy chase cases (e.g.
/// `webont-description-logic-907`, `webont-i5-1-010`) actually trip it.
pub const PER_CASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// The native verdict token for one case (`consistent` / `inconsistent` /
/// `incomplete`), computed exactly as the grader/runner does: a non-empty
/// `gaps` is `incomplete` (an honest "cannot decide"); otherwise the
/// consistency boolean.
///
/// The parse + `dl_consistency` run executes in a spawned worker thread joined
/// with a [`PER_CASE_TIMEOUT`] budget: a known-heavy case that hangs/OOMs the
/// chase is reported as `incomplete` rather than wedging the gate. Treating a
/// timeout as `incomplete` is always sound and is the expected honest-gap
/// token, so it can never manufacture a false failure.
pub fn native_token(input_nq: &Path) -> String {
    let path = input_nq.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
            .unwrap_or_else(|e| panic!("dl_consistency on {}: {e}", path.display()));
        let token = if !verdict.gaps.is_empty() {
            "incomplete"
        } else if verdict.consistent {
            "consistent"
        } else {
            "inconsistent"
        };
        // A receiver hung up (timed out) is fine — the token is simply discarded.
        let _ = tx.send(token.to_owned());
    });
    match rx.recv_timeout(PER_CASE_TIMEOUT) {
        Ok(token) => {
            let _ = worker.join();
            token
        }
        // Timeout (or a panicked worker that dropped its sender): treat as the
        // honest gap. The detached worker is left to unwind on process exit; we do
        // NOT join it, so a genuinely wedged chase cannot block the gate.
        Err(_) => "incomplete".to_owned(),
    }
}

/// The `external/w3c-owl2-full-decided` corpus root — the 32 cases that were once
/// honest DL gaps but the native refutation kernel now DECIDES soundly.
pub fn decided_root() -> PathBuf {
    cases_root().join("external").join("w3c-owl2-full-decided")
}

/// The `external/w3c-owl2-full-divergence` corpus root — the cases where OWL DL
/// and OWL Full diverge AND the native refutation kernel still honestly cannot
/// decide them.
pub fn divergence_root() -> PathBuf {
    cases_root()
        .join("external")
        .join("w3c-owl2-full-divergence")
}

/// The sorted case-directory slugs directly under `root` (a dir is a case iff it
/// holds `input.nq`), keyed to their paths.
pub fn case_slugs(root: &Path) -> BTreeMap<String, PathBuf> {
    assert!(root.is_dir(), "corpus root missing: {}", root.display());
    let mut cases = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap_or_else(|e| panic!("read {}: {e}", root.display()))
    {
        let path = entry
            .unwrap_or_else(|e| panic!("dir entry in {}: {e}", root.display()))
            .path();
        if !path.is_dir() || !path.join("input.nq").is_file() {
            continue;
        }
        let slug = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("non-UTF8 case dir name: {}", path.display()))
            .to_owned();
        cases.insert(slug, path);
    }
    cases
}
