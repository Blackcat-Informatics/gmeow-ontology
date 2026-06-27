// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The external-corpus soundness gate (#753, X1 keystone of epic #752).
//!
//! This is the *external ground truth* check that distinguishes #753 from the
//! endogenous goldens (#641): for every vendored case under
//! `conformance/logic/cases/external/<corpus>/<case>/`, the verdict the native
//! engine produced (the committed `expected/verdicts.json`, which the conformance
//! harness independently re-asserts against the live engine on every run) MUST equal
//! the verdict *declared by the third-party source* (the `source/` SZS problem or
//! W3C `manifest.ttl`), as mapped through the ingestion adapter.
//!
//! Chain: `source` →(adapter)→ declared outcome; `engine` →(harness)→ committed
//! golden; this test asserts `declared == committed`. Transitively the native engine
//! agrees with the external standard suite — soundness, not mere stability.
//!
//! It also audits each corpus's `corpus.json` license (must be IMPORT_OK to be
//! vendored) — the licensing policy applied to the real committed corpus.

use std::collections::BTreeSet;
use std::path::Path;

use gmeow_conformance::external::{
    audit_vendorable, load_corpus_meta, outcome_from_szs, parse_test_manifest, ExternalOutcome,
};
use gmeow_conformance::paths::cases_root;

/// The external-corpus root, `conformance/logic/cases/external/`.
fn external_root() -> std::path::PathBuf {
    cases_root().join("external")
}

/// Sorted immediate subdirectories of `dir`.
fn subdirs(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

/// Derive the third-party-declared outcome from a case's `source/` directory.
fn declared_outcome(case_dir: &Path) -> ExternalOutcome {
    let source = case_dir.join("source");
    let szs = source.join("problem.p");
    let manifest = source.join("manifest.ttl");

    if szs.is_file() {
        let text = std::fs::read_to_string(&szs).expect("read SZS source");
        return outcome_from_szs(&text)
            .unwrap_or_else(|e| panic!("{}: SZS parse: {e}", szs.display()));
    }
    if manifest.is_file() {
        let text = std::fs::read_to_string(&manifest).expect("read manifest source");
        // Absolute base IRI → `file:///abs/.../manifest.ttl` (empty authority) even for
        // a relative case path; `format!("file://{relative}")` would mis-read the first
        // segment as the authority. `absolute` is lexical (no filesystem / no url dep).
        let abs = std::path::absolute(&manifest).expect("resolve manifest path");
        let base = format!("file://{}", abs.display());
        let entries = parse_test_manifest(&text, Some(&base))
            .unwrap_or_else(|e| panic!("{}: manifest parse: {e}", manifest.display()));
        // The case directory name selects the entry (falls back to the sole entry).
        let case_name = case_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let entry = entries
            .iter()
            .find(|e| e.name == case_name)
            .or_else(|| {
                if entries.len() == 1 {
                    entries.first()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                panic!(
                    "{}: no manifest entry matches case {case_name:?}",
                    manifest.display()
                )
            });
        return entry.outcome();
    }
    panic!(
        "{}: external case has no source/problem.p or source/manifest.ttl",
        case_dir.display()
    );
}

/// The set of `status` strings in a case's committed `expected/verdicts.json`.
fn committed_statuses(case_dir: &Path) -> BTreeSet<String> {
    let path = case_dir.join("expected").join("verdicts.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let obj = value
        .as_object()
        .unwrap_or_else(|| panic!("{}: verdicts.json must be an object", path.display()));
    assert!(
        !obj.is_empty(),
        "{}: verdicts.json is empty — was the case blessed?",
        path.display()
    );
    obj.values()
        .map(|world| {
            world["status"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: a world has no string status", path.display()))
                .to_string()
        })
        .collect()
}

#[test]
fn external_corpus_verdicts_match_their_third_party_source() {
    let root = external_root();
    assert!(
        root.is_dir(),
        "external corpus root missing: {}",
        root.display()
    );

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for corpus_dir in subdirs(&root) {
        // License audit: a vendored corpus must be IMPORT_OK.
        let meta = load_corpus_meta(&corpus_dir.join("corpus.json"))
            .unwrap_or_else(|e| panic!("{}: corpus.json: {e}", corpus_dir.display()));
        if let Err(e) = audit_vendorable(&meta) {
            failures.push(format!("{}: license audit: {e}", corpus_dir.display()));
        }

        for case_dir in subdirs(&corpus_dir) {
            // A case dir has a profile.json; skip any non-case dir defensively.
            if !case_dir.join("profile.json").is_file() {
                continue;
            }
            let declared = declared_outcome(&case_dir).verdict_status().as_str();
            let committed = committed_statuses(&case_dir);
            checked += 1;

            // Every world's engine verdict must equal the source-declared verdict.
            for status in &committed {
                if status != declared {
                    failures.push(format!(
                        "{}: source declares {declared:?} but engine produced {status:?}",
                        case_dir.display()
                    ));
                }
            }
        }
    }

    assert!(
        checked >= 5,
        "expected ≥5 external cases (szs-mini ×3, w3c-mini ×2), found {checked}"
    );
    assert!(
        failures.is_empty(),
        "external soundness violated:\n  • {}",
        failures.join("\n  • ")
    );
}
