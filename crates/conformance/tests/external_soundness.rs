// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The external-corpus soundness gate.
//!
//! This is the *external ground truth* check that distinguishes external corpora
//! from the endogenous goldens: for every vendored case under
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
    ExternalOutcome, outcome_from_szs, parse_szs_status, parse_test_manifest,
};
use gmeow_conformance::paths::cases_root;
use gmeow_conformance::profile::parse_profile;
use gmeow_conformance::vendored::{audit_vendorable, load_corpus_meta};

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

/// The `logic:Discipline` local names a case's committed `expected/materialized.nq`
/// records as fired (`<s> <logic:violation> <logic:{Discipline}> <g> .`). The lossy
/// projection of the rich foundation output down to the discipline set is applied
/// only here, at the gate — never at ingest.
fn fired_disciplines_in_golden(case_dir: &Path) -> BTreeSet<String> {
    const VIOLATION: &str = "<https://blackcatinformatics.ca/logic/violation>";
    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    let path = case_dir.join("expected").join("materialized.nq");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut fired = BTreeSet::new();
    for line in text.lines() {
        let mut toks = line.split_whitespace();
        let (_s, p, o) = (toks.next(), toks.next(), toks.next());
        if p != Some(VIOLATION) {
            continue;
        }
        if let Some(obj) = o {
            let iri = obj.trim_start_matches('<').trim_end_matches('>');
            if let Some(local) = iri.strip_prefix(LOGIC_NS) {
                fired.insert(local.to_owned());
            }
        }
    }
    fired
}

/// Cross-check one OntoUML foundation-discipline case (`source/model.ttl`): the fired
/// `logic:Discipline` set in the committed golden MUST contain the model's documented
/// anti-pattern (agreement), and a clean-control case (no `documented_antipattern`) MUST
/// fire NOTHING — a fired discipline there is a soundness-breaking false positive. This
/// is the discipline analogue of the SZS provenance check. Returns a failure string, or
/// `None` when the case is sound / not an OntoUML case.
fn ontouml_soundness_failure(case_dir: &Path) -> Option<String> {
    if !case_dir.join("source").join("model.ttl").is_file() {
        return None;
    }
    let profile_path = case_dir.join("profile.json");
    let profile_text = std::fs::read_to_string(&profile_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", profile_path.display()));
    let profile_value: serde_json::Value = serde_json::from_str(&profile_text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", profile_path.display()));
    let case_id = case_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let profile = parse_profile(case_id, &profile_value)
        .unwrap_or_else(|e| panic!("{}: profile parse: {e}", profile_path.display()));

    let fired = fired_disciplines_in_golden(case_dir);
    match profile.documented_antipattern.as_deref() {
        Some(label) if !fired.contains(label) => Some(format!(
            "{}: documented anti-pattern {label:?} is NOT reproduced by the native \
             disciplines (fired: {fired:?}) — a divergence belongs in the sibling \
             -divergence corpus, never Lane-A",
            case_dir.display()
        )),
        None if !fired.is_empty() => Some(format!(
            "{}: clean-control case fired disciplines {fired:?} — a soundness-breaking \
             false positive; a Lane-A clean control must fire nothing",
            case_dir.display()
        )),
        _ => None,
    }
}

/// Per-corpus case-count floors for the non-`divergence`-lane corpora this
/// sweep actually walks (the `divergence` lanes are skipped in the loop below
/// and are deliberately absent here). Each constant is a floor on that one
/// corpus's case count, not an exact pin: legitimate future additions to a
/// corpus still pass. Summed into [`MIN_CHECKED_TOTAL`] below, so a
/// lane-routing regression that skips (or empties) any one of these corpora —
/// most notably the `decided` lane — drops `checked` under the sum and trips
/// the coverage-floor assertion, rather than silently passing against a
/// stale, pre-`decided`-lane magic number.
const MIN_ENTAILMENT_MINI: usize = 4;
const MIN_ONTOUML_MINI: usize = 8;
const MIN_SZS_MINI: usize = 3;
const MIN_TPTP_MINI: usize = 6;
const MIN_W3C_MINI: usize = 2;
const MIN_W3C_OWL2_EL: usize = 19;
const MIN_W3C_OWL2_FULL: usize = 261;
const MIN_W3C_OWL2_FULL_DECIDED: usize = 32;

/// The coverage floor for `checked` below: the sum of the per-corpus floors
/// above (4 + 8 + 3 + 6 + 2 + 19 + 261 + 32 == 335), i.e. the true current
/// total case count this sweep checks. Dropping the 32-case `decided` lane
/// alone (or any other lane above) is enough to fail this assertion.
const MIN_CHECKED_TOTAL: usize = MIN_ENTAILMENT_MINI
    + MIN_ONTOUML_MINI
    + MIN_SZS_MINI
    + MIN_TPTP_MINI
    + MIN_W3C_MINI
    + MIN_W3C_OWL2_EL
    + MIN_W3C_OWL2_FULL
    + MIN_W3C_OWL2_FULL_DECIDED;

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
        //
        // The `decided` lane is deliberately NOT skipped: its committed golden IS
        // the decided native token, which by construction EQUALS the W3C published
        // (== source-declared) verdict, so `committed == declared` holds and this
        // gate proves that third-party agreement statically (the dedicated
        // `full_decided_gate` proves it live).
        if meta.lane == gmeow_conformance::vendored::Lane::Divergence {
            continue;
        }

        for case_dir in subdirs(&corpus_dir) {
            // A case dir has a profile.json; skip any non-case dir defensively.
            if !case_dir.join("profile.json").is_file() {
                continue;
            }

            // OntoUML foundation-discipline cases (source/model.ttl) carry no
            // consistency verdict to compare; their soundness check is that the fired
            // discipline set contains the documented anti-pattern (and clean controls
            // fire nothing). Route them to the dedicated OntoUML soundness check.
            if case_dir.join("source").join("model.ttl").is_file() {
                checked += 1;
                if let Some(f) = ontouml_soundness_failure(&case_dir) {
                    failures.push(f);
                }
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
        checked >= MIN_CHECKED_TOTAL,
        "expected ≥{MIN_CHECKED_TOTAL} external cases (entailment-mini ×{MIN_ENTAILMENT_MINI}, \
         szs-mini ×{MIN_SZS_MINI}, w3c-mini ×{MIN_W3C_MINI}, w3c-owl2-el ×{MIN_W3C_OWL2_EL}, \
         tptp-mini ×{MIN_TPTP_MINI}, ontouml-mini ×{MIN_ONTOUML_MINI}, \
         w3c-owl2-full ×{MIN_W3C_OWL2_FULL}, w3c-owl2-full-decided ×{MIN_W3C_OWL2_FULL_DECIDED}; \
         the *-divergence lanes are excluded above, but the `decided` lane is NOT — its committed \
         verdict == the W3C-declared verdict by construction), found {checked}"
    );
    assert!(
        failures.is_empty(),
        "external soundness violated:\n  • {}",
        failures.join("\n  • ")
    );
}
