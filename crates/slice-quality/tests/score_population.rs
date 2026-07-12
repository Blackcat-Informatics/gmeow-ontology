// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Population and grounding regressions for namespace-neutral slice ownership.

use std::path::PathBuf;

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, slice_terms};
use purrdf::RdfDataset;

const LOGIC_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
const OTHER_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/other";
const MATH_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/math";

fn parse(ttl: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("fixture parses")
}

#[test]
fn population_is_namespace_neutral_but_requires_type_and_explicit_ownership() {
    let ds = parse(&format!(
        r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix lang: <https://blackcatinformatics.ca/lang/> .
        @prefix math: <https://blackcatinformatics.ca/math/> .
        @prefix ex: <https://example.org/> .
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

        gmeow:Owned a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
        logic:Owned a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
        lang:Owned a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
        math:Owned a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
        ex:Owned a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .

        # These must not leak into the population.
        ex:TypedButUnowned a owl:Class .
        logic:OwnedByAnotherSlice a owl:Class ; rdfs:isDefinedBy <{OTHER_SLICE}> .
        math:OwnedButUntyped rdfs:isDefinedBy <{LOGIC_SLICE}> .
        [] a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
        "#
    ));

    assert_eq!(
        slice_terms(&ds, LOGIC_SLICE),
        vec![
            "https://blackcatinformatics.ca/gmeow/Owned",
            "https://blackcatinformatics.ca/lang/Owned",
            "https://blackcatinformatics.ca/logic/Owned",
            "https://blackcatinformatics.ca/math/Owned",
            "https://example.org/Owned",
        ]
    );
}

#[test]
fn population_has_no_namespace_fallback_when_ownership_is_absent() {
    let ds = parse(
        r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix owl: <http://www.w3.org/2002/07/owl#> .

        gmeow:Unowned a owl:Class .
        logic:Unowned a owl:Class .
        "#,
    );

    assert!(
        slice_terms(&ds, LOGIC_SLICE).is_empty(),
        "a missing ownership declaration must not broaden the scored population"
    );
}

fn grounding_score(ttl: &str, slice_iri: &str) -> gmeow_slice_quality::score::AxisScore {
    let ds = parse(ttl);
    let ctx = ScoreContext::new(slice_iri.to_owned(), PathBuf::new(), &ds);
    axes::resolve("grounding_axis").expect("producer exists")(&ctx)
}

#[test]
fn owned_logic_classes_are_the_intrinsic_foundation_of_the_logic_slice() {
    let result = grounding_score(
        &format!(
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            logic:Formula a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
            "#
        ),
        LOGIC_SLICE,
    );

    assert_eq!(result.score, 1.0);
    assert!(result.findings.is_empty());
}

#[test]
fn intrinsic_foundation_credit_does_not_leak_by_slice_or_namespace() {
    let logic_iri_owned_elsewhere = grounding_score(
        &format!(
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            logic:Borrowed a owl:Class ; rdfs:isDefinedBy <{OTHER_SLICE}> .
            "#
        ),
        OTHER_SLICE,
    );
    assert_eq!(logic_iri_owned_elsewhere.score, 0.0);
    assert_eq!(logic_iri_owned_elsewhere.findings.len(), 1);

    let non_logic_iri_owned_by_logic = grounding_score(
        &format!(
            r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            gmeow:DomainClass a owl:Class ; rdfs:isDefinedBy <{LOGIC_SLICE}> .
            "#
        ),
        LOGIC_SLICE,
    );
    assert_eq!(non_logic_iri_owned_by_logic.score, 0.0);
    assert_eq!(non_logic_iri_owned_by_logic.findings.len(), 1);
}

#[test]
fn real_math_population_and_maximal_axes_are_pinned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root canonicalizes");
    let dir = root.join("slices/grounding/math");
    let paths = gmeow_slice_quality::report::slice_ttl_paths(&dir);
    let refs: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    let ds = gmeow_slice_quality::dataset_from_paths(&refs).expect("math slice parses");
    let terms = slice_terms(&ds, MATH_SLICE);
    assert_eq!(
        terms.len(),
        592,
        "the scored math term population is explicit"
    );

    let ctx = ScoreContext::new(MATH_SLICE.to_owned(), dir, &ds);
    for producer in [
        "grounding_axis",
        "information_axis",
        "linkage_axis",
        "testing_axis",
        "projection_axis",
        "shape_migration_axis",
        "prose_axis",
        "translation_axis",
        "flagship_counterexample_depth_axis",
    ] {
        let result = axes::resolve(producer).expect("axis producer exists")(&ctx);
        assert_eq!(
            result.score, 1.0,
            "math {producer} population stays complete"
        );
        assert!(
            result.findings.is_empty(),
            "math {producer}: {:?}",
            result.findings
        );
    }
}
