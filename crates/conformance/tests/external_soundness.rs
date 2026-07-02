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
    audit_vendorable, load_corpus_meta, outcome_from_szs, parse_szs_status, parse_test_manifest,
    ExternalOutcome,
};
use gmeow_conformance::paths::cases_root;
use gmeow_conformance::profile::parse_profile;

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

/// Verify the raw-SZS provenance for a TPTP case (one with `source/problem.p`):
/// the `profile.json` MUST carry an `szs_status` field equal to the source's raw
/// `% SZS status` token (the fine-grained token, preserved verbatim, not the
/// 3-bucket projection). Non-TPTP cases (W3C manifests) carry no `szs_status`.
/// Returns a failure string, or `None` when the provenance is intact / N/A.
fn szs_provenance_failure(case_dir: &Path) -> Option<String> {
    let szs = case_dir.join("source").join("problem.p");
    if !szs.is_file() {
        return None;
    }
    let source = std::fs::read_to_string(&szs).expect("read SZS source");
    let raw_token = parse_szs_status(&source)
        .unwrap_or_else(|e| panic!("{}: SZS token parse: {e}", szs.display()));

    let profile_path = case_dir.join("profile.json");
    let profile_text = std::fs::read_to_string(&profile_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", profile_path.display()));
    let profile_value: serde_json::Value = serde_json::from_str(&profile_text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", profile_path.display()));
    let case_id = case_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let profile = parse_profile(case_id, &profile_value)
        .unwrap_or_else(|e| panic!("{}: profile parse: {e}", profile_path.display()));

    match profile.szs_status {
        None => Some(format!(
            "{}: TPTP case (source/problem.p) is missing the required szs_status provenance \
             field in profile.json (raw token {raw_token:?})",
            case_dir.display()
        )),
        Some(committed) if committed != raw_token => Some(format!(
            "{}: profile.json szs_status {committed:?} does not match the source \
             `% SZS status` token {raw_token:?}",
            case_dir.display()
        )),
        Some(_) => None,
    }
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

        // The `divergence` lane is the named honest-DlGap quarantine: the native
        // engine deliberately disagrees with (or cannot decide) the W3C published
        // verdict there, so `committed == declared` does NOT hold by construction.
        // The dedicated divergence gate (`el_divergence_gate`) pins those cases
        // exactly; this soundness check (committed == declared) must skip them.
        if meta.lane == gmeow_conformance::external::Lane::Divergence {
            continue;
        }

        for case_dir in subdirs(&corpus_dir) {
            // A case dir has a profile.json; skip any non-case dir defensively.
            if !case_dir.join("profile.json").is_file() {
                continue;
            }
            let declared = declared_outcome(&case_dir).verdict_status().as_str();
            let committed = committed_statuses(&case_dir);
            checked += 1;

            // Raw-SZS provenance (MAXIMAL INFORMATION FLOW): a TPTP case must pin the
            // fine-grained source token in profile.json, cross-checked against the
            // source. The 3-bucket projection (`declared` above) is applied only here.
            if let Some(f) = szs_provenance_failure(&case_dir) {
                failures.push(f);
            }

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
        checked >= 30,
        "expected ≥30 external cases (szs-mini ×3, w3c-mini ×2, w3c-owl2-el ×19, \
         tptp-mini ×6; the w3c-owl2-el-divergence and tptp-mini-divergence lanes are \
         excluded above), found {checked}"
    );
    assert!(
        failures.is_empty(),
        "external soundness violated:\n  • {}",
        failures.join("\n  • ")
    );
}
