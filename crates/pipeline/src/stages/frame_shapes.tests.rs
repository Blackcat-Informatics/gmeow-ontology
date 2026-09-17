// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_frame_shapes() -> String {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-frame-shapes")
        .expect("authenticated frame-shapes fixture");
    String::from_utf8(
        artifacts
            .get(FRAME_SHAPES_PATH)
            .expect("frame-shapes artifact")
            .clone(),
    )
    .expect("frame-shapes utf8")
}

fn synthetic_frame_store() -> Dataset {
    Dataset::parse_turtle(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

gmeow:CharacterArc
    gmeow:requiresFrame gmeow:arcFrame ;
    gmeow:frameCardinality "exactly-one" ;
    gmeow:ruleSeverity "binding" .

gmeow:arcFrame a owl:FunctionalProperty .
"#,
        None,
        "synthetic frame declarations",
    )
    .expect("parse synthetic frame declarations")
}

/// Semantic drift-guard: the bespoke `render_frame_shape` and the shared canonical
/// projection ([`project_validation_shape_shacl`]) are TWO lowerings of the SAME
/// [`ValidationShapeIr`]. The frame surface keeps a bespoke renderer only for its exact
/// committed byte layout (a human-readable `rdfs:label`, prefixed CURIEs), but the two MUST
/// stay behaviorally identical — else a change to the shared projection would silently not
/// reach the frame surface. For every frame shape this validates the same data against BOTH
/// rendered SHACL documents and asserts they flag the SAME focus nodes (robust to the cosmetic
/// label/CURIE differences a byte comparison would trip on), and that the guard has teeth (the
/// missing-frame instance IS flagged by both).
#[test]
fn bespoke_frame_render_and_shared_projection_are_behaviorally_identical() {
    use crate::stages::native_query;
    use gmeow_logic_compile::projections::shapes::project_validation_shape_shacl;
    use purrdf::shapes::engine::{parse_shapes, validate_dataset};
    use std::collections::BTreeSet;

    let store = synthetic_frame_store();
    let shapes = frame_shapes(&store).expect("build frame IR");
    assert_eq!(
        shapes.len(),
        1,
        "the synthetic graph declares one frame shape"
    );

    let flagged = |report: &purrdf::shapes::report::ValidationReport| -> BTreeSet<String> {
        report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect()
    };

    for shape in &shapes {
        let carrier = match &shape.target {
            ShapeTarget::Class(c) => c.clone(),
            other => panic!("frame shape target must be a class, got {other:?}"),
        };
        let prop = shape.properties[0].path.clone();

        let bespoke_ttl = format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
                 @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
                 @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
            render_frame_shape(shape).expect("bespoke render")
        );
        let shared_ttl = format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n{}",
            project_validation_shape_shacl(shape)
        );
        let bespoke_shapes = parse_shapes(&bespoke_ttl, None).expect("parse bespoke frame shape");
        let shared_shapes = parse_shapes(&shared_ttl, None).expect("parse shared projection");

        // A carrier instance MISSING its frame (violates minCount 1) and one that carries it.
        let data = format!(
            "@prefix ex: <https://example.org/> .\n\
                 ex:bad a <{carrier}> .\n\
                 ex:good a <{carrier}> ; <{prop}> ex:frame .\n"
        );
        let data_store = native_query::dataset_from_turtle(data.as_bytes(), "drift").unwrap();

        let bespoke_report = validate_dataset(&data_store, &bespoke_shapes).unwrap();
        let shared_report = validate_dataset(&data_store, &shared_shapes).unwrap();

        assert_eq!(
            flagged(&bespoke_report),
            flagged(&shared_report),
            "bespoke frame render and the shared projection must flag the SAME focus nodes \
                 for {} — the shared projection changed without the bespoke renderer mirroring it",
            shape.iri
        );
        // Teeth: the missing-frame instance IS flagged by both (not a vacuous agreement).
        assert!(
            flagged(&bespoke_report).iter().any(|f| f.contains("/bad")),
            "the missing-frame instance must be flagged for {} (guard has no teeth otherwise)",
            shape.iri
        );
        assert!(
            !flagged(&shared_report).iter().any(|f| f.contains("/good")),
            "the framed instance must NOT be flagged for {}",
            shape.iri
        );
    }
}

#[test]
fn rule_severity_declaration_drives_shape_severity() {
    // An advisory carrier downgrades to sh:Warning; an undeclared carrier
    // stays binding (sh:Violation) — proves the read predicate is ruleSeverity.
    let ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            gmeow:Soft gmeow:requiresFrame gmeow:softFrame ; gmeow:ruleSeverity \"advisory\" .\n\
            gmeow:Hard gmeow:requiresFrame gmeow:hardFrame .\n";
    let store = Dataset::parse_turtle(ttl.as_bytes(), None, "test").unwrap();
    let rows = frame_requirements(&store).unwrap();
    let soft = rows.iter().find(|r| r.0 == "Soft").expect("Soft row");
    let hard = rows.iter().find(|r| r.0 == "Hard").expect("Hard row");
    assert_eq!(soft.3.shacl_token(), "sh:Warning");
    assert_eq!(hard.3.shacl_token(), "sh:Violation");
}

#[test]
fn frame_shapes_flow_through_validation_shape_ir() {
    use gmeow_logic_compile::ir::NodeKind;

    let store = synthetic_frame_store();
    let shapes = frame_shapes(&store).expect("build validation-shape IR");

    // The constraint data is now the canonical ValidationShapeIr.
    assert!(!shapes.is_empty(), "expected at least one frame shape");
    for shape in &shapes {
        assert_eq!(shape.node_kind, NodeKind::ValidationShape);
    }

    // A known carrier (CharacterArc → arcFrame, exactly one, binding) carries the
    // expected label / severity / message on its single property.
    let ca = shapes
        .iter()
        .find(|s| s.iri == format!("{NS}CharacterArcFrameRequirementShape"))
        .expect("CharacterArc frame shape");
    assert_eq!(
        ca.label.as_deref(),
        Some("CharacterArc frame-relativity shape (generated)")
    );
    assert_eq!(ca.target, ShapeTarget::Class(format!("{NS}CharacterArc")));
    let prop = &ca.properties[0];
    assert_eq!(prop.path, format!("{NS}arcFrame"));
    assert_eq!(prop.min_count, Some(1));
    assert_eq!(prop.max_count, Some(1));
    assert_eq!(prop.severity, Some(ShaclSeverity::Violation));
    assert_eq!(
        prop.message.as_deref(),
        Some(
            "A CharacterArc must carry exactly one reference frame (gmeow:arcFrame) — a value asserted without its frame is ill-formed (CONSTITUTION P11)."
        )
    );
}

#[test]
fn unknown_rule_severity_hard_fails() {
    let ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            gmeow:Bad gmeow:requiresFrame gmeow:badFrame ; gmeow:ruleSeverity \"bogus\" .\n";
    let store = Dataset::parse_turtle(ttl.as_bytes(), None, "test").unwrap();
    assert!(frame_requirements(&store).is_err());
}

/// Shift-left: drive the SAME native structural lint `make validate`/`make
/// check` run (`gmeow_validate::lint::structural_lint_dataset`) over this
/// generator's real output, so a missing/incorrect A-Box annotation on a
/// minted `*FrameRequirementShape` individual reds HERE — a fast `cargo
/// nextest -p gmeow-pipeline` — rather than only surfacing at the next
/// expensive whole-bundle SHACL validation (`make validate` / the
/// pipeline stage-validate) (mirrors
/// `provenance_graph::tests::minted_individuals_satisfy_the_assertional_abox_contract`).
#[test]
fn minted_shapes_satisfy_the_assertional_abox_contract() {
    use gmeow_validate::lint::{
        LintConfig, default_annotation_predicates, structural_lint_dataset,
    };

    let ttl = authenticated_frame_shapes();
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
    // kernel slice; add it here (same pattern as
    // `provenance_graph::tests::minted_individuals_satisfy_the_assertional_abox_contract`)
    // so the graphBoxRole-typing check has its declaration to resolve against.
    let doc = format!("{ttl}\n<{NS}boxABox> a <{NS}GraphBoxRole> .\n");
    let native = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse frame-shapes into native dataset");

    let cfg = LintConfig {
        namespace: NS.to_string(),
        ontology_iri: NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };
    let report = structural_lint_dataset(&native, &cfg);
    let errors = report.errors();
    let shape_errors: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains("FrameRequirementShape"))
        .collect();
    assert!(
        shape_errors.is_empty(),
        "every minted *FrameRequirementShape individual must satisfy the A-Box \
             annotation contract (rdfs:label / skos:definition / rdfs:isDefinedBy / \
             gmeow:graphBoxRole): {shape_errors:?}"
    );
}
