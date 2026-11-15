// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A tiny synthetic case under a temporary checkout root, with empty goldens to refresh.
fn synthetic_case(root: &Path, id: &str, profile: &str) {
    let case = root.join("conformance/logic/cases").join(id);
    std::fs::create_dir_all(case.join("expected/projections")).expect("case tree");
    std::fs::write(
        case.join("input.logic.ttl"),
        "<urn:test:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:test:B> .",
    )
    .expect("case input");
    std::fs::write(case.join("profile.json"), profile).expect("case profile");
    for golden in ["projections/owl-dl.ttl", "verdicts.json"] {
        std::fs::write(case.join("expected").join(golden), "").expect("empty golden");
    }
}

/// Protects: the maintenance command refreshes exactly the goldens a named case already
/// commits and reports each one, a second run reports nothing, and an unknown case or a case
/// that selects shipped rules is refused before any named case is written.
#[test]
fn named_cases_are_refreshed_and_a_refused_case_writes_nothing() {
    let checkout = tempfile::tempdir().expect("temporary checkout");
    let root = checkout.path();
    synthetic_case(root, "synthetic/class-axiom", r#"{"mode":"native"}"#);
    synthetic_case(
        root,
        "synthetic/library-rule",
        r#"{"mode":"native","shipped_rules":["urn:test:rule"]}"#,
    );
    let case = PathBuf::from("synthetic/class-axiom");
    let golden = root.join("conformance/logic/cases/synthetic/class-axiom/expected/verdicts.json");

    for (refused, reason) in [
        ("synthetic/absent", "no conformance case named"),
        ("synthetic/library-rule", "compiled rule library"),
    ] {
        // gmeow-test-input: synthetic-only; every case is constructed above in a temporary tree.
        let error = bless_cases(
            root,
            &[case.clone(), PathBuf::from(refused)],
            Goldens::Existing,
        )
        .expect_err("the named case is refused");
        assert!(error.to_string().contains(reason), "{error}");
        assert_eq!(std::fs::read(&golden).expect("golden"), b"");
    }

    // gmeow-test-input: synthetic-only; every case is constructed above in a temporary tree.
    let blessed =
        bless_cases(root, std::slice::from_ref(&case), Goldens::Existing).expect("the case runs");
    assert_eq!(blessed.len(), 1);
    assert_eq!(blessed[0].case_id, "synthetic/class-axiom");
    assert_eq!(
        blessed[0].changes,
        [
            (PathBuf::from("projections/owl-dl.ttl"), Change::Changed),
            (PathBuf::from("verdicts.json"), Change::Changed),
        ]
    );

    // gmeow-test-input: synthetic-only; every case is constructed above in a temporary tree.
    let again = bless_cases(root, &[case], Goldens::Existing).expect("the case runs again");
    assert_eq!(again[0].changes, []);
}
