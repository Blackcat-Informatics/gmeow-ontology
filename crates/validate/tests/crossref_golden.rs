// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Byte-exact CrossRef deposit-XML goldens for `gmeow_validate::crossref`.
//!
//! These tests replace the retired Python parity/structural suite
//! (`tests/test_crossref_parity.py`, `tests/_crossref_legacy.py`,
//! `tests/test_crossref.py`). The goldens under `tests/goldens/crossref/` were
//! generated from the parity-green state with the same fixed inputs the parity
//! test used (`FIXED_TIMESTAMP = "20240115120000"`,
//! `FIXED_BATCH_ID = "test-batch-001"`):
//!
//! * each `<name>.input.json` is the `DepositInput` JSON;
//! * each `<name>.deposit.xml` is `build_deposit_xml(json, TS, B)` over it.
//!
//! Byte-equality therefore subsumes every structural assertion the Python tests
//! made (well-formedness, dataset count, version links, crossmark nesting,
//! citation projection); the explicit structural asserts below pin the most
//! load-bearing of those invariants directly so a future golden regeneration
//! cannot silently drop them.

use std::path::{Path, PathBuf};

const FIXED_TIMESTAMP: &str = "20240115120000";
const FIXED_BATCH_ID: &str = "test-batch-001";

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/crossref")
}

fn read_golden(name: &str, ext: &str) -> String {
    let path = goldens_dir().join(format!("{name}.{ext}"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read golden {}: {e}", path.display()))
}

/// Render the deposit XML from `<name>.input.json` and assert byte-identity with
/// `<name>.deposit.xml`. On mismatch, report the first differing char index and
/// the two lengths (mirroring the retired Python parity test's diagnostics).
fn assert_golden(name: &str) -> String {
    let json = read_golden(name, "input.json");
    let expected = read_golden(name, "deposit.xml");
    let actual =
        gmeow_validate::crossref::build_deposit_xml(&json, FIXED_TIMESTAMP, FIXED_BATCH_ID)
            .unwrap_or_else(|e| panic!("build_deposit_xml failed for {name}: {e}"));
    if actual != expected {
        let first_diff = actual
            .char_indices()
            .zip(expected.char_indices())
            .find(|((_, a), (_, b))| a != b)
            .map(|((i, _), _)| i as isize)
            .unwrap_or(-1);
        panic!(
            "Byte-identity FAILED ({name}):\n  \
             actual length: {}, golden length: {}\n  \
             first diff at char {}",
            actual.len(),
            expected.len(),
            first_diff
        );
    }
    actual
}

// ─────────────────────────────────────────────────────────────────────────────
// Byte-exact goldens
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn golden_real() {
    let xml = assert_golden("real");
    // `real` projects exactly len(ALIGNMENT_TARGETS) citations: read the count
    // from the input JSON's config.alignment_targets array and pin the XML count.
    let input = read_golden("real", "input.json");
    let parsed: serde_json::Value = serde_json::from_str(&input).expect("input json parses");
    let target_count = parsed["config"]["alignment_targets"]
        .as_array()
        .expect("alignment_targets is an array")
        .len();
    assert_eq!(
        xml.matches("<citation ").count(),
        target_count,
        "expected one <citation> per alignment target"
    );
}

#[test]
fn golden_concept_only() {
    let xml = assert_golden("concept_only");
    // Exactly ONE record and no concept↔version link.
    assert_eq!(
        xml.matches("<dataset dataset_type=\"record\">").count(),
        1,
        "concept-only deposit must carry exactly one dataset record"
    );
    assert!(
        !xml.contains("isVersionOf") && !xml.contains("hasVersion"),
        "concept-only deposit must carry no isVersionOf/hasVersion link"
    );
}

#[test]
fn golden_with_version() {
    let xml = assert_golden("with_version");
    // TWO records and both concept↔version edges.
    assert_eq!(
        xml.matches("<dataset dataset_type=\"record\">").count(),
        2,
        "version deposit must carry two dataset records"
    );
    assert!(
        xml.contains("isVersionOf"),
        "version deposit must carry an isVersionOf link"
    );
    assert!(
        xml.contains("hasVersion"),
        "version deposit must carry a hasVersion link"
    );
}

#[test]
fn golden_crossmark_disabled() {
    let xml = assert_golden("crossmark_disabled");
    // Crossmark disabled: the AccessIndicators license program sits directly
    // under <dataset> (no <crossmark> wrapper, no <custom_metadata> nesting).
    assert!(
        !xml.contains("<crossmark>"),
        "crossmark-disabled deposit must not emit a <crossmark> element"
    );
    assert!(
        xml.contains("<ai:program name=\"AccessIndicators\">"),
        "crossmark-disabled deposit must carry a top-level ai:program"
    );
    assert!(
        !xml.contains("<custom_metadata>"),
        "crossmark-disabled deposit must not nest ai:program in custom_metadata"
    );
}

#[test]
fn golden_crossmark_enabled() {
    let xml = assert_golden("crossmark_enabled");
    // Crossmark enabled: the license program nests under <crossmark> /
    // <custom_metadata> rather than sitting directly under <dataset>.
    assert!(
        xml.contains("<crossmark>"),
        "crossmark-enabled deposit must emit a <crossmark> element"
    );
    assert!(
        xml.contains("<custom_metadata>"),
        "crossmark-enabled deposit must nest ai:program under custom_metadata"
    );
    assert!(
        xml.contains("<ai:program name=\"AccessIndicators\">"),
        "crossmark-enabled deposit must still carry the AccessIndicators program"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Error path — crossmark enabled without a policy DOI must hard-fail
// (replaces test_crossmark_enabled_without_policy_doi_fails_fast)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn crossmark_enabled_without_policy_doi_is_err() {
    // Start from the crossmark_enabled golden input and blank the policy DOI.
    let json = read_golden("crossmark_enabled", "input.json");
    let mut parsed: serde_json::Value = serde_json::from_str(&json).expect("input json parses");
    parsed["config"]["crossmark_policy_doi"] = serde_json::Value::String(String::new());
    let blanked = serde_json::to_string(&parsed).expect("re-serialise");
    let result =
        gmeow_validate::crossref::build_deposit_xml(&blanked, FIXED_TIMESTAMP, FIXED_BATCH_ID);
    let err = result.expect_err("crossmark enabled with empty policy DOI must fail fast");
    assert!(
        err.contains("CROSSMARK_POLICY_DOI must be non-empty"),
        "unexpected error message: {err}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Lint — small hand-crafted LintInput fixtures
// (replaces the parity lint tests; tiny, never bakes the real ontology TTL)
// ─────────────────────────────────────────────────────────────────────────────

/// A minimal, clean `LintInput` JSON (no file contents) that `lint_deposit`
/// reports zero problems for. Tests mutate a copy of this to provoke each lint.
fn clean_lint_input() -> serde_json::Value {
    serde_json::json!({
        "self_description": {
            "title": "T",
            "version": "1.0",
            "release_date": "2026-01-01",
            "concept_doi": "10.67342/26w4o",
            "version_doi": null,
            "version_iri": "https://blackcatinformatics.ca/gmeow",
            "depositor_name": "D",
            "depositor_email": "d@e.ca",
            "registrant": "R",
            "registrant_wikidata": "http://www.wikidata.org/entity/Q1",
            "license_uri": "https://creativecommons.org/licenses/by/4.0/",
            "homepage": "",
            "description": "desc",
            "repo_url": "https://repo.example/",
            "contributors": [
                {"kind": "organization", "name": "Org", "orcid": null,
                 "sequence": "first", "role": "author"}
            ]
        },
        "config": {
            "ontology_iri": "https://blackcatinformatics.ca/gmeow",
            "dataset_slug": "GMEOW",
            "deposit_format": "Turtle",
            "registrant_place": "P",
            "registrant_acronym": "R",
            "crossmark_enabled": false,
            "crossmark_policy_doi": "",
            "alignment_targets": [
                {"key": "gufo", "name": "gUFO", "namespace": "https://ns.example/",
                 "kind": "upper", "doi": null, "related_identifier": "https://ns.example/"}
            ]
        },
        "citation_cff": null,
        "ontology_ttl": null
    })
}

fn lint(input: &serde_json::Value) -> Vec<String> {
    let json = serde_json::to_string(input).expect("serialise lint input");
    gmeow_validate::crossref::lint_deposit(&json).expect("lint_deposit must not error")
}

#[test]
fn lint_clean_input_has_no_problems() {
    assert_eq!(lint(&clean_lint_input()), Vec::<String>::new());
}

#[test]
fn lint_flags_placeholder_concept_doi() {
    let mut input = clean_lint_input();
    input["self_description"]["concept_doi"] = serde_json::json!("10.XXXXX/gmeow");
    let problems = lint(&input);
    assert!(
        problems.iter().any(|p| p.contains("placeholder")),
        "expected a placeholder problem, got: {problems:?}"
    );
}

#[test]
fn lint_flags_missing_license() {
    let mut input = clean_lint_input();
    input["self_description"]["license_uri"] = serde_json::json!("");
    let problems = lint(&input);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("no dcterms:license for ai:program")),
        "expected a license problem, got: {problems:?}"
    );
}

#[test]
fn lint_flags_missing_wikidata() {
    let mut input = clean_lint_input();
    input["self_description"]["registrant_wikidata"] = serde_json::Value::Null;
    let problems = lint(&input);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("no Wikidata authority for registrant")),
        "expected a wikidata problem, got: {problems:?}"
    );
}
