// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn product(stage: &str, turtle: &str) -> StageProduct {
    let dataset = purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None)
        .expect("parse synthetic dataset");
    StageProduct::from_artifacts_over(stage, dataset, BTreeMap::new())
}

#[test]
fn compose_unions_synthetic_base_and_statement_layers() {
    let upstream = BTreeMap::from([
        (
            "stage-source-load".to_string(),
            product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
        ),
        (
            "stage-statements".to_string(),
            product("stage-statements", "<urn:statement> <urn:p> <urn:o> ."),
        ),
    ]);
    let composed = compose(&upstream).expect("compose synthetic layers");
    assert_eq!(composed.quad_count(), 2);
    let nquads =
        String::from_utf8(compose_nquads(&upstream).expect("project union")).expect("utf8");
    assert!(nquads.contains("<urn:base>"));
    assert!(nquads.contains("<urn:statement>"));
}

#[test]
fn compose_fails_closed_on_missing_or_empty_required_layers() {
    let base_only = BTreeMap::from([(
        "stage-source-load".to_string(),
        product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
    )]);
    assert!(compose(&base_only).is_err());

    let empty_statements = BTreeMap::from([
        (
            "stage-source-load".to_string(),
            product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
        ),
        (
            "stage-statements".to_string(),
            StageProduct::from_artifacts("stage-statements", BTreeMap::new()),
        ),
    ]);
    assert!(compose(&empty_statements).is_err());
}
