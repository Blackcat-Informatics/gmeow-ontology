// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The live corpus root, anchored at the crate manifest dir.
fn corpus_cases_root() -> PathBuf {
    crate::paths::cases_root()
}

#[test]
fn discovers_the_full_corpus() {
    // Canonical discovery test: every case under projections/ (and the rest of the corpus)
    // is discovered. (The retired tests/test_logic_runner.py::TestDiscoverCases covered the
    // same case before being removed.)
    let cases = discover_cases(&corpus_cases_root()).expect("discovery ok");
    let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();
    assert!(
        ids.iter().any(|id| id.starts_with("projections/")),
        "no projection cases among {ids:?}"
    );
    // Every discovered case carries a JSON-object profile and an input file.
    for case in &cases {
        assert!(case.profile.is_object());
        assert!(case.case_dir.join("input.logic.ttl").is_file());
    }
    // The corpus is non-trivial (sanity floor; the exact count is asserted by
    // the harness against the Python baseline, not pinned here).
    assert!(cases.len() >= 20, "unexpectedly few cases: {}", cases.len());

    // Recursion guard: the discovered id SET must be unchanged except for
    // `external/**` additions. A standard case id is exactly `<category>/<case>`
    // (one slash); only a vendored corpus case may be deeper, and then it MUST be
    // `external/<corpus>/<case>` (two slashes). This catches the recursion
    // accidentally vacuuming up a stray nested directory under a non-external
    // category as a bogus 3-component case.
    for id in &ids {
        let depth = id.matches('/').count();
        if id.starts_with("external/") {
            assert_eq!(
                depth, 2,
                "external case id must be external/<corpus>/<case>: {id}"
            );
        } else {
            assert_eq!(
                depth, 1,
                "non-external case id must be <category>/<case>: {id}"
            );
        }
    }
}

#[test]
fn discovers_three_level_external_case_and_skips_source_subtree() {
    // A vendored external corpus lives at cases/external/<corpus>/<case>/ — three
    // levels deep. The recursive walk must find it (the binary path missed it
    // before), assign the external/-prefixed id, and NOT mistake the case's
    // own `source/` subtree for a nested case.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let cases = tmp.path().join("cases");
    let case = cases.join("external").join("w3c-mini").join("clash");
    std::fs::create_dir_all(case.join("source")).expect("mkdir case+source");
    std::fs::write(case.join("input.logic.ttl"), "").expect("input");
    std::fs::write(case.join("profile.json"), "{}").expect("profile");
    // A decoy under source/ that would be a case if recursion didn't stop at the
    // case dir.
    std::fs::write(case.join("source").join("input.logic.ttl"), "").expect("decoy input");
    std::fs::write(case.join("source").join("profile.json"), "{}").expect("decoy profile");

    let found = discover_cases(&cases).expect("discovery ok");
    let ids: Vec<&str> = found.iter().map(|c| c.case_id.as_str()).collect();
    assert_eq!(ids, vec!["external/w3c-mini/clash"], "got {ids:?}");
}

#[test]
fn hard_fails_on_missing_cases_dir() {
    // Ports TestDiscoverCases::test_hard_fails_on_missing_cases_dir.
    let missing = corpus_cases_root().join("__definitely_absent__");
    assert!(discover_cases(&missing).is_err());
}

#[test]
fn validate_case_rejects_dir_without_input() {
    // A profile.json-bearing dir missing input.logic.ttl is a hard failure.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let case = tmp.path().join("cat").join("case");
    std::fs::create_dir_all(&case).expect("mkdir");
    std::fs::write(case.join("profile.json"), "{}").expect("write");
    let err = validate_case(&case).unwrap_err();
    assert!(err.message().contains("input.logic.ttl not found"));
}

#[test]
fn validate_case_rejects_non_object_profile() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let case = tmp.path().join("cat").join("case");
    std::fs::create_dir_all(&case).expect("mkdir");
    std::fs::write(case.join("input.logic.ttl"), "").expect("write input");
    std::fs::write(case.join("profile.json"), "[1, 2, 3]").expect("write profile");
    let err = validate_case(&case).unwrap_err();
    assert!(err.message().contains("must be a JSON object"));
}
