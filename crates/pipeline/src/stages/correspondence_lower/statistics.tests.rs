// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed;

#[test]
fn statistics_preserve_membership_when_correspondences_deduplicate() {
    let dataset = purrdf::parse_dataset(
        br#"
            @prefix gm: <https://blackcatinformatics.ca/gmeow/> .
            @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            gm:Person skos:closeMatch gm:Agent {|
                gm:sssomFile "people.sssom.tsv" ; logic:morphismClass logic:AffineCorrespondence
            |} .
            gm:Person skos:closeMatch gm:Agent {|
                gm:sssomFile "agents.sssom.tsv" ; logic:morphismKind logic:InstitutionMorphism
            |} .
            gm:people a gm:MappingSet ; gm:sssomFile "people.sssom.tsv" .
            gm:peopleAlso a gm:MappingSet ; gm:sssomFile "people.sssom.tsv" .
            gm:agents a gm:MappingSet ; gm:sssomFile "agents.sssom.tsv" .
        "#,
        "text/turtle",
        None,
    )
    .unwrap();
    let view = DslView::new(&dataset);
    let (program, analysis) = transpile_correspondences_indexed(&view).unwrap();
    assert_eq!(program.correspondences.len(), 1);
    let rows = gmeow_logic_compile::projections::sssom::lower_sssom(
        &view,
        "test",
        "2026-01-01",
        &analysis,
    )
    .unwrap();
    assert_eq!(
        rows.sets.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["agents.sssom.tsv", "people.sssom.tsv"]
    );
    let output = emit(&view, &analysis, &gmeow_ns::gmeow_slice_vocab()).unwrap();
    let actual: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({
            "cells_by_set": { "agents.sssom.tsv": 1, "people.sssom.tsv": 1 },
            "equivalences": 2, "functions": 0, "mapping_sets": 2, "projections": 0,
        })
    );
}
