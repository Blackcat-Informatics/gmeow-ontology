// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared `gmeow:FlagshipScenarioShape` unwired-detection negative fixture.
//!
//! The flagship cardinality wiring is authored declaratively in the logic grounding slice
//! (`gmeow:FlagshipScenario` min-1/max-1 restrictions) and PROJECTED to the derived
//! `gmeow:FlagshipScenario-shape` in `generated/shapes/validation-shapes.ttl`; the per-slice
//! `flagship-scenario-unwired.ttl` counter-examples were deleted (the now-thin per-slice shapes
//! can no longer raise MinCount). A rule with no negative fixture is not enforced — so the shared
//! projected shape MUST have a negative fixture proving its unwired-detection gate bites.
//!
//! This test constructs, in-memory, a `gmeow:FlagshipScenario` that wires every required link
//! EXCEPT `gmeow:demonstratedByProducer`, validates it against the shared shapes graph, and
//! asserts the shared shape raises a `sh:MinCountConstraintComponent` violation on the missing
//! producer path — a violation whose `source_shape` maps, through the shape→failure-class
//! annotation (`gmeow:FlagshipScenarioShape gmeow:enforcesFailureClass
//! gmeow:UnwiredFlagshipScenario`), to `gmeow:UnwiredFlagshipScenario`. The shared unwired gate
//! bites, deterministically.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gmeow_validate::store::shacl_validate_dataset;
use purrdf::RdfTerm;
use purrdf::shapes::engine::parse_shapes;

/// The `gmeow:` namespace (the manifest/annotation vocabulary and the unwired failure class).
use gmeow_ns::GMEOW_NS;

/// A minimal `gmeow:FlagshipScenario` wiring EVERY shared-shape-required link except
/// `gmeow:demonstratedByProducer`, so exactly the producer MinCount constraint bites. The
/// datatypes/node-kinds match the shared shape's per-property constraints (string paths, IRI
/// competency/failure-class) so no OTHER constraint fires spuriously.
const UNWIRED_SCENARIO: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix cq:    <https://blackcatinformatics.ca/gmeow/examples/lang/tests/> .

<https://blackcatinformatics.ca/gmeow/examples/unwired/missingProducer>
    a gmeow:FlagshipScenario ;
    gmeow:demonstratedByExample "tests/conformance-fixtures/example.ttl" ;
    gmeow:demonstratedByCompetency cq:cqSome ;
    gmeow:guardedByCounterExample "tests/counter-examples/counter.ttl" ;
    gmeow:enforcesFailureClass lang:UnhashableSurface .
"#;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root canonicalizes")
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

/// Resolve the projected shape-to-failure-class links from the exact authenticated
/// shape artifact supplied by the explicit pre-test producer.
fn shape_class_map(shapes_text: &str) -> HashMap<String, String> {
    let dataset = purrdf::parse_dataset(shapes_text.as_bytes(), "text/turtle", None)
        .expect("authenticated production shapes parse as RDF");
    let predicate = format!("{GMEOW_NS}enforcesFailureClass");
    dataset
        .owned_quads()
        .filter_map(|quad| {
            if quad.predicate != predicate {
                return None;
            }
            let RdfTerm::Iri(subject) = quad.subject else {
                return None;
            };
            let RdfTerm::Iri(object) = quad.object else {
                return None;
            };
            Some((subject, object))
        })
        .collect()
}

#[test]
fn shared_flagship_shape_bites_on_a_missing_required_link() {
    // The projected validation-shape surface carries the derived gmeow:FlagshipScenario-shape (the
    // cardinality gate) and its gmeow:enforcesFailureClass gmeow:UnwiredFlagshipScenario annotation.
    let shapes_text = String::from_utf8(
        gmeow_bundle_import::load_authenticated_corpus_artifact(
            &repo_root(),
            "validate-production-shapes.ttl",
        )
        .expect("load authenticated production shapes without rebuilding them"),
    )
    .expect("authenticated production shapes are UTF-8");
    let shapes = parse_shapes(&shapes_text, None).expect("authenticated production shapes parse");

    // The shape -> failure-class map, resolved from the projected surface: the flagship gate
    // resolves to gmeow:UnwiredFlagshipScenario.
    let shape_class = shape_class_map(&shapes_text);

    // The unwired scenario, parsed in-memory (deterministic — no fixture on disk, no ordering).
    let data = purrdf::parse_dataset(UNWIRED_SCENARIO.as_bytes(), "text/turtle", None)
        .expect("unwired scenario Turtle parses");

    let report = shacl_validate_dataset(&data, &shapes);

    // Find the MinCount violation on the missing gmeow:demonstratedByProducer link, sourced by
    // gmeow:FlagshipScenarioShape and mapped to gmeow:UnwiredFlagshipScenario.
    let unwired_class = format!("{GMEOW_NS}UnwiredFlagshipScenario");
    let producer_path = format!("{GMEOW_NS}demonstratedByProducer");

    let mincount_hits: Vec<_> = report
        .results
        .iter()
        .filter(|r| {
            local_name(r.source_constraint_component.as_str()) == "MinCountConstraintComponent"
        })
        .collect();
    assert!(
        !mincount_hits.is_empty(),
        "the shared gmeow:FlagshipScenarioShape must raise a MinCountConstraintComponent for the \
         missing required link, but raised {:?}",
        report
            .results
            .iter()
            .map(|r| r.source_constraint_component.as_str().to_owned())
            .collect::<Vec<_>>()
    );

    // The missing-producer MinCount violation is present, path-scoped to demonstratedByProducer,
    // and maps to gmeow:UnwiredFlagshipScenario via the shared shape.
    let producer_hit = mincount_hits.iter().find(|r| {
        r.result_path
            .as_ref()
            .map(|p| p.to_string().contains(&producer_path))
            .unwrap_or(false)
    });
    let producer_hit = producer_hit.unwrap_or_else(|| {
        panic!(
            "the shared shape must raise MinCount on gmeow:demonstratedByProducer specifically; \
             MinCount paths seen: {:?}",
            mincount_hits
                .iter()
                .map(|r| r.result_path.as_ref().map(std::string::ToString::to_string))
                .collect::<Vec<_>>()
        )
    });

    let rendered = producer_hit.source_shape.to_string();
    let shape_iri = rendered
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(rendered.as_str());
    let mapped = shape_class.get(shape_iri).unwrap_or_else(|| {
        panic!(
            "the missing-producer violation's source shape {shape_iri} carries no \
             gmeow:enforcesFailureClass annotation"
        )
    });
    assert_eq!(
        mapped, &unwired_class,
        "the shared unwired gate must map to gmeow:UnwiredFlagshipScenario, mapped {mapped}"
    );
}
