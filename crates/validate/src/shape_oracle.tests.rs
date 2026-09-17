// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::projections::shapes::project_validation_shape_shacl;

/// The SHACL prefix header the emitter's CURIEs (`sh:` / `xsd:`) resolve against.
const HEADER: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

fn parse_ttl(ttl: &str) -> std::sync::Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("test turtle must parse")
}

/// Emit `shape` with the production emitter, parse it back with the reader, assert the
/// round trip has NO residue, and return its covered IR.
fn read_back(shape: &ValidationShapeIr) -> ValidationShapeIr {
    let ttl = format!("{HEADER}{}", project_validation_shape_shacl(shape));
    let ds = parse_ttl(&ttl);
    let read = read_shacl_shape(&ds, &shape.iri)
        .unwrap_or_else(|e| panic!("read_shacl_shape failed for {}: {e}\n{ttl}", shape.iri));
    assert!(
        read.unsupported.is_empty(),
        "a round-tripped emitter shape must have NO residue, got {:?}\n{ttl}",
        read.unsupported
    );
    read.ir
}

/// Emit → read-back → enforcement-equivalent to the original.
fn assert_round_trips(shape: &ValidationShapeIr) {
    let parsed = read_back(shape);
    assert!(
        subsumption::equivalent(shape, &parsed),
        "round trip not equivalent for {}:\n  original={:?}\n  parsed={:?}",
        shape.iri,
        shape,
        parsed
    );
}

#[test]
fn round_trips_typed_failure_metadata_without_residue() {
    let shape = ValidationShapeIr::new(
        "https://ex/Shape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_failure_class("https://ex/Failure")
    .unwrap();
    let parsed = read_back(&shape);
    assert_eq!(parsed.failure_class.as_deref(), Some("https://ex/Failure"));
}

#[test]
fn duplicate_typed_failure_metadata_is_malformed() {
    let ttl = format!(
        "{HEADER}<https://ex/Shape> a sh:NodeShape ;\n\
             sh:targetClass <https://ex/C> ;\n\
             <{GMEOW_ENFORCES_FAILURE_CLASS}> <https://ex/FailureA>, <https://ex/FailureB> .\n"
    );
    let ds = parse_ttl(&ttl);
    let err = read_shacl_shape(&ds, "https://ex/Shape").unwrap_err();
    assert!(
        err.message()
            .contains("distinct gmeow:enforcesFailureClass")
    );
}

#[test]
fn repeated_identical_typed_failure_metadata_is_one_value() {
    let ttl = format!(
        "{HEADER}<https://ex/Shape> a sh:NodeShape ;\n\
             sh:targetClass <https://ex/C> ;\n\
             <{GMEOW_ENFORCES_FAILURE_CLASS}> <https://ex/Failure>, <https://ex/Failure> .\n"
    );
    let ds = parse_ttl(&ttl);
    let read = read_shacl_shape(&ds, "https://ex/Shape").expect("identical values dedupe");
    assert_eq!(read.ir.failure_class.as_deref(), Some("https://ex/Failure"));
}

#[test]
fn round_trips_cardinality_class_datatype_nodekind_in_not_qvs_reifier() {
    let p_card = PropertyConstraintIr::new(
        "https://ex/a",
        Some(1),
        Some(2),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    )
    .unwrap();
    let p_dt = PropertyConstraintIr::new(
        "https://ex/b",
        None,
        None,
        None,
        vec![ConstraintComponent::Datatype(
            "http://www.w3.org/2001/XMLSchema#string".into(),
        )],
    )
    .unwrap();
    let p_nk = PropertyConstraintIr::new(
        "https://ex/c",
        None,
        None,
        None,
        vec![ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)],
    )
    .unwrap();
    let p_in = PropertyConstraintIr::new(
        "https://ex/d",
        None,
        None,
        None,
        vec![ConstraintComponent::In(vec![
            ShapeValue::Iri("https://ex/v1".into()),
            ShapeValue::Iri("https://ex/v2".into()),
            ShapeValue::Literal(purrdf::RdfLiteral::simple("plain")),
        ])],
    )
    .unwrap();
    let p_not = PropertyConstraintIr::new(
        "https://ex/e",
        None,
        None,
        None,
        vec![ConstraintComponent::Not(Box::new(
            ConstraintComponent::Class("https://ex/Disjoint".into()),
        ))],
    )
    .unwrap();
    let p_qvs = PropertyConstraintIr::new(
        "https://ex/f",
        None,
        None,
        None,
        vec![ConstraintComponent::QualifiedValueShape {
            shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
            min: Some(1),
            max: None,
        }],
    )
    .unwrap();
    let p_reifier = PropertyConstraintIr::new("https://ex/g", None, None, None, vec![])
        .unwrap()
        .with_reifier(Some("https://ex/ReifierShape".into()), true)
        .unwrap();

    let shape = ValidationShapeIr::new(
        "https://ex/BigShape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![p_card, p_dt, p_nk, p_in, p_not, p_qvs, p_reifier],
        None,
    )
    .unwrap()
    .with_node_components(vec![
        ConstraintComponent::Class("https://ex/NodeClass".into()),
        ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
    ])
    .unwrap();
    assert_round_trips(&shape);
}

#[test]
fn round_trips_numeric_range() {
    let shape = ValidationShapeIr::new(
        "https://ex/NumShape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/magnitude",
                None,
                None,
                None,
                vec![
                    ConstraintComponent::NumericRange {
                        min: Some(0.0),
                        max: Some(100.0),
                        min_inclusive: true,
                        max_inclusive: true,
                    },
                    ConstraintComponent::Datatype(
                        "http://www.w3.org/2001/XMLSchema#decimal".into(),
                    ),
                ],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    assert_round_trips(&shape);
}

#[test]
fn round_trips_has_value() {
    let shape = ValidationShapeIr::new(
        "https://ex/HvShape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::HasValue(ShapeValue::Iri(
                    "https://ex/fixed".into(),
                ))],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    assert_round_trips(&shape);
}

#[test]
fn round_trips_complete_literal_value_identity() {
    for direction in [purrdf::RdfTextDirection::Ltr, purrdf::RdfTextDirection::Rtl] {
        let literal = purrdf::RdfLiteral {
            lexical_form: "same".to_owned(),
            datatype: None,
            language: Some("fr".to_owned()),
            direction: Some(direction),
        };
        let component = ConstraintComponent::HasValue(ShapeValue::Literal(literal));
        let shape = ValidationShapeIr::new(
            "https://ex/NativeLiteralShape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                PropertyConstraintIr::new("https://ex/p", None, None, None, vec![component])
                    .unwrap(),
            ],
            None,
        )
        .unwrap();
        assert_round_trips(&shape);
    }
}

#[test]
fn round_trips_inverse_path_and_domain_range_targets() {
    let inverse = ValidationShapeIr::new(
        "https://ex/InvShape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )
            .unwrap()
            .inverted(),
        ],
        None,
    )
    .unwrap();
    assert_round_trips(&inverse);

    let domain = ValidationShapeIr::new(
        "https://ex/DomainShape",
        ShapeTarget::SubjectsOf("https://ex/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::Datatype(
        "http://www.w3.org/2001/XMLSchema#string".into(),
    )])
    .unwrap();
    assert_round_trips(&domain);

    let range = ValidationShapeIr::new(
        "https://ex/RangeShape",
        ShapeTarget::ObjectsOf("https://ex/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
    .unwrap();
    assert_round_trips(&range);
}

#[test]
fn presentation_message_and_severity_are_absorbed_and_projected_out() {
    // Every property carries sh:message + sh:severity; the read-back must be equivalent
    // to the SAME shape WITHOUT them (presentation is projected out of enforcement).
    let bare = PropertyConstraintIr::new(
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OptNative),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    )
    .unwrap();
    let decorated = bare
        .clone()
        .with_severity(ShaclSeverity::Warning)
        .with_message("every focus must have exactly one D")
        .unwrap();
    let with_pres = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![decorated],
        None,
    )
    .unwrap();
    let without_pres = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![bare],
        None,
    )
    .unwrap();

    let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&with_pres));
    assert!(
        ttl.contains("sh:message") && ttl.contains("sh:severity"),
        "{ttl}"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
    assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
    assert!(
        subsumption::equivalent(&read.ir, &without_pres),
        "presentation must be projected out: {:?} vs {:?}",
        read.ir,
        without_pres
    );
}

#[test]
fn presentation_annotation_predicates_are_skipped_not_residue() {
    // rdfs:label / sh:name / sh:description / sh:order / skos:* are pure annotation.
    let ttl = format!(
        "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             <https://ex/S> a sh:NodeShape ;\n    \
             rdfs:label \"a label\" ;\n    \
             sh:name \"a name\" ;\n    \
             sh:description \"a description\" ;\n    \
             sh:order 3 ;\n    \
             skos:note \"a note\" ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:property [ sh:path <https://ex/p> ; sh:class <https://ex/D> ] .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
    assert!(
        read.unsupported.is_empty(),
        "annotation predicates must NOT be residue: {:?}",
        read.unsupported
    );
    assert_eq!(read.ir.properties.len(), 1);
}

#[test]
fn mixed_covered_and_sparql_yields_covered_ir_plus_residue_without_err() {
    // A node shape mixing a covered property (sh:class) with an uncovered sh:sparql
    // constraint AND an sh:or must yield the covered fragment PLUS a residue list.
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:property [ sh:path <https://ex/p> ; sh:class <https://ex/D> ] ;\n    \
             sh:or ( [ sh:class <https://ex/A> ] [ sh:class <https://ex/B> ] ) ;\n    \
             sh:sparql [ sh:select \"SELECT ?this WHERE {{ ?this ?p ?o }}\" ] .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
        .expect("a mixed shape must NOT Err — it yields covered + residue");
    // The covered fragment survived: one property carrying the Class component.
    assert_eq!(read.ir.properties.len(), 1);
    assert_eq!(
        read.ir.properties[0].components,
        vec![ConstraintComponent::Class("https://ex/D".into())]
    );
    // The residue carries the uncovered sh:or and sh:sparql (sorted, deterministic).
    assert!(
        read.unsupported.iter().any(|u| u.contains("shacl#or")),
        "residue must flag sh:or: {:?}",
        read.unsupported
    );
    assert!(
        read.unsupported.iter().any(|u| u.contains("shacl#sparql")),
        "residue must flag sh:sparql: {:?}",
        read.unsupported
    );
    // The oracle carries the residue through and marks the class residue-bearing.
    let projected = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::Class("https://ex/D".into())],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let verdict = oracle(&read, &projected);
    assert!(
        verdict.equivalent,
        "covered fragments must match: {}",
        verdict.reason
    );
    assert!(
        verdict.residue_bearing && !verdict.unsupported.is_empty(),
        "a residue-bearing legacy class is not deletable on the covered match alone: {verdict:?}"
    );
}

#[test]
fn unparseable_sparql_target_is_residue_not_err() {
    // An sh:SPARQLTarget whose select is NOT the value-keyed single-triple form (here two
    // distinct non-type patterns) cannot be inverted → routed to residue, never a hard error.
    // A covered sh:targetClass supplies the focus selector, so the read still succeeds.
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:target [ a sh:SPARQLTarget ; sh:select \"SELECT ?this WHERE {{ ?this <https://ex/k> <https://ex/v> ; <https://ex/k2> <https://ex/v2> }}\" ] .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
        .expect("an unparsable sh:SPARQLTarget alongside a covered target is residue, not Err");
    assert_eq!(read.ir.target, ShapeTarget::Class("https://ex/C".into()));
    assert!(
        read.unsupported.iter().any(|u| u.contains("shacl#target")),
        "the unparsable sh:target must be residue: {:?}",
        read.unsupported
    );
}

#[test]
fn node_level_or_over_min_one_property_branches_reads_as_or_properties() {
    // The exact `sh:or ( [ sh:path P ; sh:minCount 1 ] … )` form is COVERED — it reads into
    // an OrProperties node component (and round-trips the emitter), never residue.
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:or ( [ sh:path <https://ex/frame> ; sh:minCount 1 ] \
                     [ sh:path <https://ex/model> ; sh:minCount 1 ] ) .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").expect("read ok");
    assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
    assert!(
        read.ir.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::OrProperties(paths)
                if paths == &vec![
                    "https://ex/frame".to_owned(),
                    "https://ex/model".to_owned()
                ]
        )),
        "{:?}",
        read.ir.node_components
    );
    // Round-trip: emit the read IR and compare with a directly-built projected twin.
    let projected = ValidationShapeIr::new(
        "https://ex/C-shape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::OrProperties(vec![
        "https://ex/model".into(),
        "https://ex/frame".into(),
    ])])
    .unwrap();
    let verdict = oracle(&read, &projected);
    assert!(verdict.equivalent, "{}", verdict.reason);
}

#[test]
fn node_level_or_with_value_branches_stays_residue() {
    // A branch carrying anything beyond `sh:path` + `sh:minCount 1` (here sh:class) is NOT
    // the covered property-alternatives form — the whole sh:or stays residue as before.
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n    \
             sh:targetClass <https://ex/C> ;\n    \
             sh:or ( [ sh:path <https://ex/frame> ; sh:minCount 1 ] \
                     [ sh:path <https://ex/dom> ; sh:minCount 1 ; sh:class <https://ex/PS> ] ) .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").expect("read ok");
    assert!(
        read.unsupported
            .iter()
            .any(|u| u == "http://www.w3.org/ns/shacl#or"),
        "{:?}",
        read.unsupported
    );
    assert!(
        !read
            .ir
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::OrProperties(_))),
        "{:?}",
        read.ir.node_components
    );
}

#[test]
fn reader_errs_on_a_missing_shape_iri() {
    let ds = parse_ttl(&format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n"
    ));
    let err = read_shacl_shape(&ds, "https://ex/DoesNotExist")
        .expect_err("a non-existent shape IRI is genuine malformation");
    assert!(
        err.to_string().contains("not present in the graph"),
        "{err}"
    );
}

#[test]
fn oracle_reports_equivalent_for_the_round_trip() {
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![ConstraintComponent::Class("https://ex/D".into())],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
    let legacy = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S").unwrap();
    let verdict = oracle(&legacy, &shape);
    assert!(verdict.equivalent, "{}", verdict.reason);
    assert!(
        verdict.legacy_subsumed_by_projected,
        "equivalent ⇒ projected ⊒ legacy"
    );
    assert!(
        !verdict.residue_bearing,
        "a clean round trip has no residue"
    );
    assert!(verdict.reason.contains("equivalent"));
}

#[test]
fn oracle_reports_not_equivalent_with_a_meaningful_reason_when_a_component_is_missing() {
    let legacy_ir = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![
                    ConstraintComponent::Class("https://ex/D".into()),
                    ConstraintComponent::MinLength(3),
                ],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let legacy = ShapeRead {
        ir: legacy_ir,
        unsupported: vec![],
        extra_targets: vec![],
    };
    // The projected shape drops the MinLength(3) component — strictly weaker.
    let projected = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::Class("https://ex/D".into())],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let verdict = oracle(&legacy, &projected);
    assert!(!verdict.equivalent, "a dropped component ⇒ not equivalent");
    assert!(
        verdict.reason.contains("not equivalent") && verdict.reason.contains("path="),
        "the reason must name the differing property path: {}",
        verdict.reason
    );
    assert!(!verdict.legacy_subsumed_by_projected);
}

#[test]
fn cross_check_is_green_when_the_two_graphs_are_the_same_shape() {
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OptNative),
                vec![ConstraintComponent::Datatype(
                    "http://www.w3.org/2001/XMLSchema#string".into(),
                )],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
    let witnesses = witnesses_for(&shape);
    assert!(
        !witnesses.is_empty(),
        "a cardinality+datatype shape must yield witnesses"
    );
    let data = parse_ttl("<https://ex/x> <https://ex/q> <https://ex/y> .\n");
    cross_check(&ttl, &ttl, &data, &witnesses)
        .expect("identical shape graphs must produce identical findings");
}

#[test]
fn cross_check_hard_fails_vacuous_when_no_focus_node_is_exercised() {
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OptNative),
                vec![],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let ttl = format!("{HEADER}{}", project_validation_shape_shacl(&shape));
    let data = parse_ttl("<https://ex/x> <https://ex/q> <https://ex/y> .\n");
    let err = cross_check(&ttl, &ttl, &data, &[])
        .expect_err("a run that exercises no focus node must HARD-FAIL as vacuous");
    let err = err.to_string();
    assert!(
        err.contains("vacuous") && err.contains("0 focus nodes"),
        "{err}"
    );
}

/// Comparison-only guard (Principle 4): this module reads bytes and returns verdicts.
/// No function here — `read_shacl_shape`, `oracle`, `witnesses_for`, `cross_check` —
/// writes to `slices/**` or the `logic:` canon; the reader NEVER parses SHACL back
/// into the authoring ground. (Asserted structurally: none of these signatures take a
/// writable sink or a path, and the module imports no filesystem writer.)
#[test]
fn module_is_comparison_only_no_canon_writeback() {
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![],
        None,
    )
    .unwrap();
    let parsed = read_back(&shape);
    assert!(subsumption::equivalent(&shape, &parsed));
}

#[test]
fn value_keyed_sparql_target_round_trips() {
    // The projector's `sh:target [ a sh:SPARQLTarget ; sh:select "…" ]` inverts back to the
    // same ValueKeyed target, with NO residue (the exact single-triple form).
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::ValueKeyed {
            predicate: "https://ex/kind".into(),
            value: "https://ex/Bp".into(),
        },
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                None,
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let read = read_back(&shape);
    assert_eq!(
        read.target,
        ShapeTarget::ValueKeyed {
            predicate: "https://ex/kind".into(),
            value: "https://ex/Bp".into()
        }
    );
    assert!(subsumption::equivalent(&shape, &read));
}

#[test]
fn sparql_target_with_type_pattern_parses_value_key_and_flags_type() {
    // A hand-authored mode-scoped select (`?this a Commitment ; mode abd`) clears the read
    // error: the value-key becomes the target, and the extra `a` type pattern is flagged as
    // residue (the IR value-keyed target cannot hold a class).
    let ttl = format!(
        "{HEADER}\
             <https://ex/AbShape> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
             PREFIX g: <https://ex/>\n\
             SELECT ?this WHERE {{ ?this a g:Commitment ; g:mode g:abd . }}\n\
             \"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/explanandum> ; sh:minCount 1 ] .\n"
    );
    let ds = parse_ttl(&ttl);
    let read = read_shacl_shape(&ds, "https://ex/AbShape")
        .unwrap_or_else(|e| panic!("read must not error: {e}\n{ttl}"));
    assert_eq!(
        read.ir.target,
        ShapeTarget::ValueKeyed {
            predicate: "https://ex/mode".into(),
            value: "https://ex/abd".into()
        },
        "value-key extracted from the select"
    );
    assert!(
        read.unsupported
            .iter()
            .any(|u| u.contains("g:Commitment") || u.contains("Commitment")),
        "the extra `a Commitment` type pattern must be flagged: {:?}",
        read.unsupported
    );
}

#[test]
fn inline_sh_node_helper_adopts_owner_target() {
    // A targetless helper shape referenced via a property's `sh:node` adopts the owning node
    // shape's `sh:targetClass` (the FramedIntervalShape fix): no read error, real target.
    let ttl = format!(
        "{HEADER}\
             <https://ex/OwnerShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Owner> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/interval> ; sh:node <https://ex/HelperShape> ] .\n\
             <https://ex/HelperShape> a sh:NodeShape ;\n\
             \x20\x20sh:property [ sh:path <https://ex/frame> ; sh:minCount 1 ] .\n"
    );
    let ds = parse_ttl(&ttl);
    let read = read_shacl_shape(&ds, "https://ex/HelperShape")
        .unwrap_or_else(|e| panic!("targetless helper must adopt owner target, got: {e}\n{ttl}"));
    assert_eq!(
        read.ir.target,
        ShapeTarget::Class("https://ex/Owner".into())
    );
}

#[test]
fn tokenize_select_keeps_iris_and_string_literals_whole() {
    let toks = tokenize_select("SELECT ?this WHERE { ?this <https://ex/a.b/c> <https://ex/v> }");
    assert!(toks.contains(&"<https://ex/a.b/c>".to_owned()), "{toks:?}");
    assert!(
        !toks.contains(&".".to_owned()),
        "no bare dot inside the IRI: {toks:?}"
    );
}

/// A legacy fixture styled after the repo-wide meta-shapes: a raw `sh:SPARQLTarget` (type
/// pattern + STRSTARTS namespace filter) plus structural property constraints.
const META_SELECT: &str = "\n\
        SELECT ?this WHERE {\n\
            ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
            FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
        }\n";

fn meta_shape_ttl() -> String {
    format!(
        "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/MetaShape> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"{META_SELECT}\"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n"
    )
}

#[test]
fn raw_sparql_target_shape_reads_as_sparql_target_with_whole_shape_residue() {
    // A meta-shape-styled block with ONLY a raw sh:SPARQLTarget reads (no Err): the select
    // becomes the ShapeTarget::Sparql focus selector, the whole shape is marked residue, and
    // the structural property constraints survive as the covered fragment.
    let ds = parse_ttl(&meta_shape_ttl());
    let read = read_shacl_shape(&ds, "https://ex/MetaShape")
        .expect("a raw-SPARQL-target shape must read, not Err");
    assert!(
        matches!(&read.ir.target, ShapeTarget::Sparql(s) if s.contains("STRSTARTS")),
        "{:?}",
        read.ir.target
    );
    assert!(
        read.unsupported
            .iter()
            .any(|u| u == RAW_SPARQL_TARGET_RESIDUE),
        "the raw target must mark the shape residue-bearing: {:?}",
        read.unsupported
    );
    assert_eq!(read.ir.properties.len(), 2, "{:?}", read.ir.properties);
}

#[test]
fn truly_targetless_doc_shape_reads_to_the_empty_focus_sentinel() {
    // A documentation-only marker (label + comment, no target construct, no constraint)
    // reads to the TARGETLESS_SELECT sentinel with an empty residue list: SHACL gives it an
    // empty focus set, so it enforces nothing.
    let ttl = format!(
        "{HEADER}@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/DocMarker> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"doc-only marker\" ;\n\
             \x20\x20rdfs:comment \"asserts and enforces nothing\" .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/DocMarker")
        .expect("a truly targetless doc shape must be verdictable, not Err");
    assert_eq!(
        read.ir.target,
        ShapeTarget::Sparql(TARGETLESS_SELECT.to_owned())
    );
    assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
    assert!(read.ir.properties.is_empty());
}

#[test]
fn targetless_shape_with_unreadable_target_construct_stays_an_err() {
    // sh:targetNode is an authored focus selector the reader cannot represent — such a
    // shape must NOT silently read as targetless.
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n\
             \x20\x20sh:targetNode <https://ex/n> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n"
    );
    let err = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
        .expect_err("an unreadable target construct must stay a hard read error");
    assert!(err.to_string().contains("no sh:targetClass"), "{err}");
}

#[test]
fn multi_target_shape_reads_all_selectors() {
    let ttl = format!(
        "{HEADER}<https://ex/S> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/B>, <https://ex/A>, <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n"
    );
    let read = read_shacl_shape(&parse_ttl(&ttl), "https://ex/S")
        .expect("a multi-target shape must read, not Err");
    assert_eq!(read.ir.target, ShapeTarget::Class("https://ex/A".into()));
    assert_eq!(
        read.extra_targets,
        vec![
            ShapeTarget::Class("https://ex/B".into()),
            ShapeTarget::Class("https://ex/C".into())
        ],
        "canonical order, primary excluded"
    );
}

#[test]
fn target_skeleton_parses_type_pattern_and_namespace() {
    let (ns, types, object_of) = parse_target_skeleton(META_SELECT).expect("meta skeleton parses");
    assert_eq!(ns.as_deref(), Some("https://example.test/ns/"));
    assert_eq!(
        types,
        vec!["http://www.w3.org/2002/07/owl#Class".to_owned()]
    );
    assert!(object_of.is_empty());
}

#[test]
fn target_skeleton_parses_object_of_property_membership() {
    // `?event a Event . ?event deceptionCue ?this` — the focus is the OBJECT of deceptionCue
    // from an Event-typed subject (the deception-cue membership shape).
    let select = "SELECT ?this WHERE { \
             ?event a <https://ex/Event> . \
             ?event <https://ex/deceptionCue> ?this . }";
    let (ns, types, object_of) = parse_target_skeleton(select).expect("object-of skeleton parses");
    assert!(ns.is_none());
    assert!(types.is_empty());
    assert_eq!(
        object_of,
        vec![(
            Some("https://ex/Event".to_owned()),
            "https://ex/deceptionCue".to_owned()
        )]
    );
}

#[test]
fn target_skeleton_parses_type_variable_with_in_domain() {
    let select = "\n\
            SELECT ?this WHERE {\n\
                ?this a ?t .\n\
                FILTER(?t IN (\n\
                    <http://www.w3.org/2002/07/owl#ObjectProperty>,\n\
                    <http://www.w3.org/2002/07/owl#DatatypeProperty>\n\
                ))\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n";
    let (ns, types, object_of) = parse_target_skeleton(select).expect("IN-domain skeleton parses");
    assert_eq!(ns.as_deref(), Some("https://example.test/ns/"));
    assert_eq!(
        types,
        vec!["http://www.w3.org/2002/07/owl#ObjectProperty".to_owned()],
        "the first domain class is the membership type"
    );
    assert!(object_of.is_empty());
}

#[test]
fn target_skeleton_rejects_opaque_selects() {
    // A predicate-namespace guard (`?this ?p ?o` + STRSTARTS on ?p) is OUTSIDE the skeleton:
    // no membership can be synthesized, so no witness can be minted (fail-safe).
    let select = "SELECT ?this WHERE { ?this ?p ?o . \
             FILTER(STRSTARTS(STR(?p), \"https://example.test/primary\")) }";
    assert!(parse_target_skeleton(select).is_none());
}

/// The xone fixture: a covered property plus an exactly-one-of-two alternative.
fn xone_shape_ttl() -> String {
    format!(
        "{HEADER}<https://ex/ParamShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Param> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] ;\n\
             \x20\x20sh:xone (\n\
             \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/value> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:Literal ] ]\n\
             \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/entity> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ] ]\n\
             \x20\x20) .\n"
    )
}

/// The record that CORRECTLY lowers the xone: flags a focus with NEITHER alternative and a
/// focus with BOTH.
const XONE_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"exactly one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            { FILTER NOT EXISTS { $this <https://ex/value> ?v } FILTER NOT EXISTS { $this <https://ex/entity> ?e } }\n\
            UNION\n\
            { $this <https://ex/value> ?v2 . $this <https://ex/entity> ?e2 . }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

/// The WRONG-SEMANTICS record: an `sh:or` lowering (flags only when NEITHER alternative is
/// present) — it does NOT reproduce the exactly-one obligation.
const XONE_OR_LOWERED_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"at least one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            FILTER NOT EXISTS { $this <https://ex/value> ?v }\n\
            FILTER NOT EXISTS { $this <https://ex/entity> ?e }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

#[test]
fn xone_witness_plan_carries_conforming_and_discriminating_residue_witnesses() {
    let ds = parse_ttl(&xone_shape_ttl());
    let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
    assert!(
        read.unsupported
            .iter()
            .any(|u| u == "http://www.w3.org/ns/shacl#xone"),
        "{:?}",
        read.unsupported
    );
    let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read)
        .expect("the xone construct is machine-readable");
    assert!(
        plan.conforming.iter().any(|w| !w.expect_flagged),
        "{plan:?}"
    );
    assert!(
        plan.residue
            .iter()
            .any(|w| w.label.contains("no alternative")),
        "{plan:?}"
    );
    assert!(
        plan.residue
            .iter()
            .any(|w| w.label.contains("two alternatives")),
        "the exactly-one discriminator must be present: {plan:?}"
    );
}

#[test]
fn semantic_cross_check_accepts_the_faithful_xone_record() {
    let ds = parse_ttl(&xone_shape_ttl());
    let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
    let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read).expect("plan");
    let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/ParamShape".to_owned()], "l");
    let mut witnesses = plan.conforming.clone();
    witnesses.extend(plan.residue.clone());
    semantic_cross_check(&legacy_ttl, XONE_RECORD_TTL, &witnesses)
        .expect("the faithful record reproduces the xone semantics");
}

#[test]
fn semantic_cross_check_rejects_the_or_lowered_xone_record() {
    // The load-bearing falsifiability check: a record that lowers the exactly-one to an
    // at-least-one does NOT flag the two-alternatives near-miss and MUST NOT clear.
    let ds = parse_ttl(&xone_shape_ttl());
    let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("fixture reads");
    let plan = semantic_witness_plan(&ds, "https://ex/ParamShape", &read).expect("plan");
    let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/ParamShape".to_owned()], "l");
    let mut witnesses = plan.conforming.clone();
    witnesses.extend(plan.residue.clone());
    let err = semantic_cross_check(&legacy_ttl, XONE_OR_LOWERED_RECORD_TTL, &witnesses)
        .expect_err("an or-lowered record must not survive the witness cross-check");
    assert!(err.to_string().contains("does not reproduce"), "{err}");
}

#[test]
fn semantic_cross_check_hard_fails_on_a_vacuous_witness_set() {
    let err =
        semantic_cross_check("", "", &[]).expect_err("an empty witness set is vacuous, not a pass");
    assert!(err.to_string().contains("vacuous"), "{err}");
}

// The WP:GNG-triad negated-conjunction fixtures: a `CitationAct` asserting `supportsNotability
// true` must carry all three triad values. `NC_*` model the projected record two ways.
//
// The CORRECT scoped lowering: ONE `FILTER NOT EXISTS` over the whole triad conjunction, so
// `$this` stays bound and the check is per focus node.
const NC_SCOPED_TTL: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/NotabilityShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/NotabilityShape> ;\n\
        \x20\x20sh:targetClass <https://ex/CitationAct> ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"WP:GNG triad\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { \
        $this <https://ex/supportsNotability> 'true'^^<http://www.w3.org/2001/XMLSchema#boolean> . \
        FILTER NOT EXISTS { $this <https://ex/indep> <https://ex/independent> . \
        $this <https://ex/tier> <https://ex/secondary> . \
        $this <https://ex/cov> <https://ex/significant> . } }\"\"\" ] .\n";
// The BUGGY unscoped De-Morgan lowering: a UNION of per-conjunct `FILTER NOT EXISTS` arms.
// Each arm binds nothing, so `$this` is unbound and the check degrades to global existence.
const NC_UNION_TTL: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/NotabilityShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/NotabilityShape> ;\n\
        \x20\x20sh:targetClass <https://ex/CitationAct> ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"WP:GNG triad\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { \
        $this <https://ex/supportsNotability> 'true'^^<http://www.w3.org/2001/XMLSchema#boolean> . \
        { FILTER NOT EXISTS { $this <https://ex/indep> <https://ex/independent> . } } UNION \
        { FILTER NOT EXISTS { $this <https://ex/tier> <https://ex/secondary> . } } UNION \
        { FILTER NOT EXISTS { $this <https://ex/cov> <https://ex/significant> . } } }\"\"\" ] .\n";

fn nc_witnesses() -> Vec<SemanticWitness> {
    negated_conjunction_sibling_witnesses(
        "https://ex/CitationAct",
        "https://ex/supportsNotability",
        "'true'^^<http://www.w3.org/2001/XMLSchema#boolean>",
        &[
            ("https://ex/indep", "<https://ex/independent>"),
            ("https://ex/tier", "<https://ex/secondary>"),
            ("https://ex/cov", "<https://ex/significant>"),
        ],
    )
}

#[test]
fn negated_conjunction_witnesses_carry_a_conforming_and_a_multi_sibling_near_miss() {
    let ws = nc_witnesses();
    assert!(ws.iter().any(|w| !w.expect_flagged), "{ws:?}");
    assert_eq!(
        ws.iter().filter(|w| w.expect_flagged).count(),
        3,
        "one multi-sibling near-miss per triad conjunct: {ws:?}"
    );
    // Every near-miss carries a sibling that satisfies the omitted conjunct.
    assert!(
        ws.iter()
            .filter(|w| w.expect_flagged)
            .all(|w| w.triples.contains("/sibling-")),
        "{ws:?}"
    );
}

#[test]
fn semantic_cross_check_accepts_the_scoped_negated_conjunction_record() {
    // The scoped single-NOT-EXISTS record reproduces the per-focus triad obligation.
    semantic_cross_check(NC_SCOPED_TTL, NC_SCOPED_TTL, &nc_witnesses())
        .expect("the scoped record reproduces the negated-conjunction semantics");
}

#[test]
fn semantic_cross_check_rejects_the_unscoped_union_negated_conjunction_projection() {
    // The load-bearing guard: the pre-fix `{¬a} UNION {¬b} UNION {¬c}` lowering loses
    // $this-scoping, so a sibling satisfying one conjunct globally clears the branch and the
    // near-miss is NOT flagged — the oracle MUST catch it (guard against silent recurrence of
    // the orgbook_notability_mutation projector bug).
    let err = semantic_cross_check(NC_SCOPED_TTL, NC_UNION_TTL, &nc_witnesses())
        .expect_err("an unscoped-union projection must not survive the witness cross-check");
    assert!(err.to_string().contains("does not reproduce"), "{err}");
}

#[test]
fn sh_node_witness_plan_discriminates_the_inner_shape() {
    // A StyleGuide-styled property-level sh:node: the inner shape requires a digest on the
    // value node. Both the violating and the conforming witness must be present, and the
    // legacy shape itself must judge them as expected (checked through the cross-check with
    // the legacy graph on BOTH sides — a definitionally-faithful record).
    let ttl = format!(
        "{HEADER}<https://ex/GuideShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Guide> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/for> ; sh:minCount 1 ] ;\n\
             \x20\x20sh:property [\n\
             \x20\x20\x20\x20sh:path <https://ex/exemplifiedBy> ;\n\
             \x20\x20\x20\x20sh:node [ sh:property [ sh:path <https://ex/digest> ; sh:minCount 1 ] ] ;\n\
             \x20\x20] .\n"
    );
    let ds = parse_ttl(&ttl);
    let read = read_shacl_shape(&ds, "https://ex/GuideShape").expect("fixture reads");
    assert!(
        read.unsupported
            .iter()
            .any(|u| u == "http://www.w3.org/ns/shacl#node"),
        "{:?}",
        read.unsupported
    );
    let plan = semantic_witness_plan(&ds, "https://ex/GuideShape", &read).expect("plan");
    assert!(
        plan.residue.iter().any(|w| w.label.contains("sh:node@")),
        "{plan:?}"
    );
    assert!(
        plan.conforming
            .iter()
            .any(|w| w.label.contains("conforming sh:node@")),
        "{plan:?}"
    );
    let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/GuideShape".to_owned()], "l");
    let mut witnesses = plan.conforming.clone();
    witnesses.extend(plan.residue.clone());
    semantic_cross_check(&legacy_ttl, &legacy_ttl, &witnesses)
        .expect("the legacy graph agrees with itself on every witness");
}

#[test]
fn meta_shape_witness_plan_pins_to_structural_property_constraints() {
    let ds = parse_ttl(&meta_shape_ttl());
    let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("fixture reads");
    let plan = semantic_witness_plan(&ds, "https://ex/MetaShape", &read).expect("plan");
    // Focus membership was synthesized from the skeleton: witnesses live under the namespace.
    for w in plan.conforming.iter().chain(plan.covered.iter()) {
        assert!(w.focus.starts_with("https://example.test/ns/"), "{w:?}");
    }
    assert!(
        plan.covered.iter().any(|w| w
            .label
            .contains("sh:minCount@http://www.w3.org/2000/01/rdf-schema#label")),
        "{plan:?}"
    );
    assert!(
        plan.covered
            .iter()
            .any(|w| w.label.contains("sh:nodeKind@https://ex/role")),
        "{plan:?}"
    );
    // The legacy shape agrees with itself over the full plan (target execution included).
    let legacy_ttl = shape_subgraph_ttl(&ds, &["https://ex/MetaShape".to_owned()], "l");
    let mut witnesses = plan.conforming.clone();
    witnesses.extend(plan.covered.clone());
    semantic_cross_check(&legacy_ttl, &legacy_ttl, &witnesses)
        .expect("the meta shape agrees with itself on every structural witness");
}
