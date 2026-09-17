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
fn fmt_floor_matches_python_repr_style() {
    // The migration-fidelity invariant: an integer-valued floor reads as `1.0`
    // (matching the pre-deletion hand-authored values), a fractional floor is the
    // shortest round-tripping decimal unchanged.
    assert_eq!(fmt_floor(1.0), "1.0");
    assert_eq!(fmt_floor(0.9954337899543378), "0.9954337899543378");
    assert_eq!(fmt_floor(0.0), "0.0");
}

#[test]
fn axis_floors_project_deterministically_from_the_ontology() {
    let root = repo_root();
    let a = String::from_utf8(
        crate::fixture::authenticated_artifact(
            &root,
            "stage-export-governance-floors",
            AXIS_FLOORS_PATH,
        )
        .expect("authenticated axis-floor projection"),
    )
    .expect("axis-floor utf8");

    let data_rows: Vec<&str> = a.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        !data_rows.is_empty(),
        "axis-floor projection must not be empty"
    );
    // The migration spot-check: the accounts slice's axisGmn1Coverage floor
    // reproduces its historical value exactly (ontology → projection fidelity).
    assert!(
            a.contains(
                "https://blackcatinformatics.ca/gmeow/slices/accounts\taxisGmn1Coverage\t0.9954337899543378\n"
            ),
            "the accounts axis floor must reproduce the historical 0.9954337899543378"
        );
    // Rows are sorted by (slice-iri, axis-local).
    let mut sorted = data_rows.clone();
    sorted.sort();
    assert_eq!(data_rows, sorted, "axis-floor rows must be sorted");
}

#[test]
fn tier_floors_project_deterministically_from_the_ontology() {
    let root = repo_root();
    let out = String::from_utf8(
        crate::fixture::authenticated_artifact(
            &root,
            "stage-export-governance-floors",
            TIER_FLOORS_PATH,
        )
        .expect("authenticated tier-floor projection"),
    )
    .expect("tier-floor utf8");
    let data_rows: Vec<&str> = out.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        !data_rows.is_empty(),
        "tier-floor projection must not be empty"
    );
    for row in &data_rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            2,
            "tier-floor row is <slice-iri>\\t<tier-local>"
        );
    }
}
