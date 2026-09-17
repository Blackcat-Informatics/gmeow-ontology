// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn ceilings_project_deterministically_from_the_ontology() {
    let root = repo_root();
    let a = String::from_utf8(
        crate::fixture::authenticated_artifact(
            &root,
            "stage-export-projection-ceilings",
            CEILINGS_PATH,
        )
        .expect("authenticated projection ceilings"),
    )
    .expect("projection ceilings utf8");

    let data_rows: Vec<&str> = a.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        !data_rows.is_empty(),
        "projection ceilings must not be empty"
    );
    for row in &data_rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            3,
            "ceiling row is <slice-iri>\\t<vocab-prefix>\\t<count>"
        );
    }
    // Rows are sorted by (slice-iri, vocab-prefix).
    let mut sorted = data_rows.clone();
    sorted.sort();
    assert_eq!(data_rows, sorted, "ceiling rows must be sorted");
}

#[test]
fn vocabularies_project_deterministically_from_the_ontology() {
    let root = repo_root();
    let a = String::from_utf8(
        crate::fixture::authenticated_artifact(
            &root,
            "stage-export-projection-ceilings",
            VOCABULARIES_PATH,
        )
        .expect("authenticated projection vocabularies"),
    )
    .expect("projection vocabularies utf8");

    let data_rows: Vec<&str> = a.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        !data_rows.is_empty(),
        "projection vocabulary inventory must not be empty"
    );
    for row in &data_rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            5,
            "vocabulary row is <prefix>\\t<namespaces>\\t<count-kind>\\t<default-ceiling>\\t<preservation-local>"
        );
    }
    // Rows are sorted by prefix.
    let mut sorted = data_rows.clone();
    sorted.sort();
    assert_eq!(data_rows, sorted, "vocabulary rows must be sorted");
}
