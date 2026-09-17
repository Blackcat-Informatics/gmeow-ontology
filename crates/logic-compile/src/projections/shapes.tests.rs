// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{ConstraintProvenance, ShaclNodeKind, ShaclSeverity};

fn prop(path: &str, comps: Vec<ConstraintComponent>) -> PropertyConstraintIr {
    PropertyConstraintIr::new(path, None, None, None, comps).unwrap()
}

fn shape(iri: &str, class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
    ValidationShapeIr::new(iri, ShapeTarget::Class(class.to_owned()), props, None).unwrap()
}

#[test]
fn validation_shape_projects_failure_class_metadata() {
    let s = shape("https://ex/Shape", "https://ex/C", vec![])
        .with_failure_class("https://ex/Failure")
        .unwrap();
    let ttl = project_validation_shape_shacl(&s);
    assert!(ttl.contains("gmeow/enforcesFailureClass> <https://ex/Failure>"));
}

#[test]
fn class_guarded_constraint_strips_the_type_atom_from_the_violation_where() {
    // A `∀ this. C(this) → ∃v. P(this, v)` constraint targets `sh:targetClass C` (which follows
    // rdfs:subClassOf). The violation WHERE must therefore NOT re-assert `$this a C` (a plain BGP
    // triple would wrongly exclude the subclass instances `sh:targetClass` selects); it keeps only
    // the negated condition.
    let this = Term::Var("this".into());
    let integrity = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::Iri(RDF_TYPE.into()),
                    vec![this.clone(), Term::Iri("https://ex/C".into())],
                )
                .unwrap(),
            ),
            Box::new(Formula::Exists {
                vars: vec!["v".into()],
                body: Box::new(
                    Formula::atom(
                        Term::Iri("https://ex/p".into()),
                        vec![this, Term::Var("v".into())],
                    )
                    .unwrap(),
                ),
            }),
        )),
    };
    let c = ConstraintIr::new("https://ex/C1", integrity, ShaclSeverity::Violation, None)
        .unwrap()
        .with_formalizes("https://ex/C")
        .unwrap();
    let block = project_procedural_constraint(&c);
    assert!(
        block.contains("sh:targetClass <https://ex/C>"),
        "targetClass must select the focus: {block}"
    );
    assert!(
        !block.contains("$this a <https://ex/C>"),
        "the redundant class-guard triple must be stripped from the WHERE: {block}"
    );
    assert!(
        block.contains("FILTER NOT EXISTS { $this <https://ex/p> ?v"),
        "the negated existential condition must remain: {block}"
    );
}

#[test]
fn half_open_quantity_interval_emits_min_inclusive_and_max_exclusive() {
    let s = shape(
        "https://ex/BpShape",
        "https://ex/Systolic",
        vec![prop(
            "https://ex/magnitude",
            vec![
                ConstraintComponent::NumericRange {
                    min: Some(0.0),
                    max: Some(1000.0),
                    min_inclusive: true,
                    max_inclusive: false,
                },
                ConstraintComponent::Datatype("http://www.w3.org/2001/XMLSchema#decimal".into()),
            ],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(ttl.contains("a sh:NodeShape"), "{ttl}");
    assert!(
        ttl.contains("sh:targetClass <https://ex/Systolic>"),
        "{ttl}"
    );
    assert!(ttl.contains("sh:minInclusive 0"), "{ttl}");
    assert!(ttl.contains("sh:maxExclusive 1000"), "{ttl}");
    assert!(
        !ttl.contains("sh:maxInclusive"),
        "half-open upper must be exclusive: {ttl}"
    );
    assert!(
        ttl.contains("sh:datatype <http://www.w3.org/2001/XMLSchema#decimal>"),
        "{ttl}"
    );
}

#[test]
fn cardinality_emits_min_and_max_count() {
    let p = PropertyConstraintIr::new(
        "https://ex/systolic",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OptNative),
        vec![],
    )
    .unwrap();
    let ttl = project_validation_shape_shacl(&shape("https://ex/S", "https://ex/C", vec![p]));
    assert!(ttl.contains("sh:minCount 1"), "{ttl}");
    assert!(ttl.contains("sh:maxCount 1"), "{ttl}");
}

#[test]
fn inline_value_set_emits_sh_in_list() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/code",
            vec![ConstraintComponent::In(vec![
                ShapeValue::Iri("https://ex/at0004".into()),
                ShapeValue::Iri("https://ex/at0005".into()),
            ])],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        ttl.contains("sh:in ( <https://ex/at0004> <https://ex/at0005> )"),
        "{ttl}"
    );
}

#[test]
fn node_kind_and_pattern_emit_and_pattern_is_ledgered() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/name",
            vec![
                ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal),
                ConstraintComponent::Pattern {
                    regex: "^[A-Z].*$".into(),
                    flags: Some("i".into()),
                },
            ],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(ttl.contains("sh:nodeKind sh:Literal"), "{ttl}");
    assert!(ttl.contains("sh:pattern \"^[A-Z].*$\""), "{ttl}");
    assert!(ttl.contains("sh:flags \"i\""), "{ttl}");
    // The pattern is lossy → the ledger records the regex-dialect residue.
    let residue = shacl_residue(&s);
    assert_eq!(residue.len(), 1, "pattern must be ledgered: {residue:?}");
    assert!(residue[0].contains("regex-dialect residue"), "{residue:?}");
}

#[test]
fn reifier_shape_and_requirement_emit_the_rdf12_extension() {
    // The reifier component is a PROPERTY-shape condition (keyed to a `sh:path`): the native
    // SHACL 1.2 engine reads `sh:reifierShape`/`sh:reificationRequired` only from a
    // single-predicate property shape, so they must emit INSIDE the `sh:property [ … ]` block.
    let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
        .unwrap()
        .with_reifier(Some("https://ex/ReifierShape".into()), true)
        .unwrap();
    let s = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![property],
        None,
    )
    .unwrap();
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        ttl.contains("sh:reifierShape <https://ex/ReifierShape>"),
        "{ttl}"
    );
    assert!(ttl.contains("sh:reificationRequired true"), "{ttl}");
    assert!(ttl.contains("sh:path <https://ex/p>"), "{ttl}");
}

#[test]
fn reifier_condition_is_rejected_on_an_inverse_path() {
    // The reifier component has no meaning on an inverse path (the engine hard-errors), so the
    // IR refuses to attach it there rather than emit a surface the engine rejects.
    let inverse = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
        .unwrap()
        .inverted();
    assert!(
        inverse
            .with_reifier(Some("https://ex/R".into()), true)
            .is_err()
    );
}

#[test]
fn standpoint_scope_is_carried_as_projection_residue() {
    // A standpoint-indexed shape (exercising the standpoint + reifier + reification fields
    // together) has no faithful SHACL/ShEx form: its scope must be recorded in the loss
    // ledger, never silently flattened to a universal shape.
    let sp = "https://blackcatinformatics.ca/gmeow/clinicalStandpoint";
    let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
        .unwrap()
        .with_reifier(Some("https://ex/ReifierShape".into()), true)
        .unwrap();
    let s = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![property],
        Some(sp.into()),
    )
    .unwrap();
    assert_eq!(s.standpoint.as_deref(), Some(sp));
    let shacl = shacl_residue(&s);
    assert!(
        shacl
            .iter()
            .any(|r| r.contains(sp) && r.contains("standpoint")),
        "standpoint scope must be recorded in the SHACL residue: {shacl:?}"
    );
    // ShEx residue is a superset, so it inherits the standpoint residue too.
    let shex = shex_residue(&s);
    assert!(
        shex.iter().any(|r| r.contains(sp)),
        "standpoint scope must also be in the ShEx residue: {shex:?}"
    );
}

#[test]
fn value_keyed_target_emits_sparql_target() {
    let s = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::ValueKeyed {
            predicate: "https://ex/kind".into(),
            value: "https://ex/Bp".into(),
        },
        vec![],
        None,
    )
    .unwrap();
    let ttl = project_validation_shape_shacl(&s);
    assert!(ttl.contains("a sh:SPARQLTarget"), "{ttl}");
    assert!(
        ttl.contains("?this <https://ex/kind> <https://ex/Bp>"),
        "{ttl}"
    );
}

#[test]
fn terminology_binding_is_not_emitted_but_is_ledgered() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/code",
            vec![ConstraintComponent::TerminologyBinding {
                terminology_id: "SNOMED-CT".into(),
                codes: vec!["271649006".into()],
            }],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        !ttl.contains("SNOMED"),
        "terminology must not leak into SHACL: {ttl}"
    );
    let residue = shacl_residue(&s);
    assert_eq!(residue.len(), 1);
    assert!(
        residue[0].contains("no faithful SHACL Core form"),
        "{residue:?}"
    );
}

#[test]
fn ordinal_set_projects_sh_in_symbols_and_ledgers_integers() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/value",
            vec![ConstraintComponent::OrdinalSet {
                pairs: vec![
                    (1, "https://ex/terminology/local/at0014".into()),
                    (2, "https://ex/terminology/local/at0015".into()),
                ],
            }],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        ttl.contains(
            "sh:in ( <https://ex/terminology/local/at0014> \
                 <https://ex/terminology/local/at0015> )"
        ),
        "{ttl}"
    );
    assert!(!ttl.contains(" 1 ") && !ttl.contains(" 2 "), "{ttl}");
    let residue = shacl_residue(&s);
    assert_eq!(
        residue.len(),
        1,
        "ordinal set must be ledgered: {residue:?}"
    );
    assert!(residue[0].contains("1"), "{residue:?}");
    assert!(residue[0].contains("2"), "{residue:?}");
}

#[test]
fn datetime_pattern_emits_no_shacl_constraint_but_is_ledgered() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/value",
            vec![ConstraintComponent::DateTimePattern(
                "yyyy-mm-ddTHH:MM:SS".into(),
            )],
        )],
    );
    // An openEHR validity pattern is a format template, not an XPath regex; emitting it as
    // `sh:pattern` would reject every valid datetime. Nothing is emitted; the meaning is
    // carried only in the loss ledger.
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        !ttl.contains("sh:pattern"),
        "datetime pattern must NOT be emitted as a SHACL constraint: {ttl}"
    );
    let residue = shacl_residue(&s);
    assert_eq!(
        residue.len(),
        1,
        "datetime pattern must be ledgered: {residue:?}"
    );
    assert!(residue[0].contains("validity pattern"), "{residue:?}");
}

#[test]
fn has_value_qualified_and_not_emit_shacl() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![
                ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/fixed".into())),
                ConstraintComponent::QualifiedValueShape {
                    shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
                    min: Some(1),
                    max: None,
                },
                ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                    "https://ex/D".into(),
                ))),
            ],
        )],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(ttl.contains("sh:hasValue <https://ex/fixed>"), "{ttl}");
    assert!(
        ttl.contains("sh:qualifiedValueShape [ sh:class <https://ex/Q> ]"),
        "{ttl}"
    );
    assert!(ttl.contains("sh:qualifiedMinCount 1"), "{ttl}");
    assert!(ttl.contains("sh:not [ sh:class <https://ex/D> ]"), "{ttl}");
}

#[test]
fn multiple_qualified_counts_preserve_each_fillers_obligation() {
    // gmeow-test-input: synthetic-only
    // Separate canonical restrictions on one path must remain conjunctive,
    // including when two count the same filler and another counts a different one.
    let qualified = |class: &str, min, max| ConstraintComponent::QualifiedValueShape {
        shape: vec![ConstraintComponent::Class(format!("https://ex/{class}"))],
        min,
        max,
    };
    for inverse in [false, true] {
        let mut property = prop(
            "https://ex/p",
            vec![
                qualified("Q", Some(1), None),
                qualified("Q", None, Some(1)),
                qualified("R", Some(1), None),
            ],
        )
        .with_message("each qualified restriction applies")
        .unwrap();
        if inverse {
            property = property.inverted();
        }
        let shape = shape("https://ex/S", "https://ex/C", vec![property])
            .with_failure_class("https://ex/Failure")
            .unwrap();
        let ttl = format!("{SHACL_PREFIXES}{}", project_validation_shape_shacl(&shape));
        assert_eq!(ttl.matches("sh:property ").count(), 2, "{ttl}");
        assert_eq!(ttl.matches("sh:message ").count(), 2, "{ttl}");
        assert_eq!(
            ttl.matches("gmeow/enforcesFailureClass>").count(),
            3,
            "{ttl}"
        );
        let shapes = purrdf::shapes::engine::parse_shapes(&ttl, None)
            .expect("the GMEOW projection must be a valid SHACL shape");
        for (q_count, r_count, conforms) in
            [(1, 1, true), (0, 1, false), (2, 1, false), (1, 0, false)]
        {
            let mut data = String::from("@prefix ex: <https://ex/> . ex:focus a ex:C .\n");
            for (class, count) in [("Q", q_count), ("R", r_count)] {
                for i in 0..count {
                    let value = format!("ex:{class}{i}");
                    data.push_str(&format!("{value} a ex:{class} .\n"));
                    let edge = if inverse {
                        format!("{value} ex:p ex:focus .\n")
                    } else {
                        format!("ex:focus ex:p {value} .\n")
                    };
                    data.push_str(&edge);
                }
            }
            let dataset = purrdf::parse_dataset(data.as_bytes(), "text/turtle", None)
                .expect("synthetic data");
            let report = purrdf::shapes::engine::validate_dataset(dataset.as_ref(), &shapes)
                .expect("validate the projected GMEOW restrictions");
            assert_eq!(
                report.conforms, conforms,
                "inverse={inverse}, Q={q_count}, R={r_count}: {report:?}"
            );
        }
    }
}

#[test]
fn domain_range_targets_and_node_components_emit_shacl() {
    // rdfs:domain P C → targetSubjectsOf P + node-level sh:class C.
    let domain_shape = ValidationShapeIr::new(
        "https://ex/p-domain-shape",
        ShapeTarget::SubjectsOf("https://ex/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
    .unwrap();
    let ttl = project_validation_shape_shacl(&domain_shape);
    assert!(ttl.contains("sh:targetSubjectsOf <https://ex/p>"), "{ttl}");
    assert!(ttl.contains("sh:class <https://ex/C>"), "{ttl}");
    // rdfs:range P C → targetObjectsOf P.
    let range_shape = ValidationShapeIr::new(
        "https://ex/p-range-shape",
        ShapeTarget::ObjectsOf("https://ex/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)])
    .unwrap();
    let ttl = project_validation_shape_shacl(&range_shape);
    assert!(ttl.contains("sh:targetObjectsOf <https://ex/p>"), "{ttl}");
    assert!(ttl.contains("sh:nodeKind sh:IRI"), "{ttl}");
}

#[test]
fn inverse_path_severity_message_and_label_emit_shacl() {
    let p = PropertyConstraintIr::new(
        "https://ex/p",
        None,
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap()
    .inverted()
    .with_severity(ShaclSeverity::Warning)
    .with_message("each object has at most one subject via p")
    .unwrap();
    let s = shape("https://ex/S", "https://ex/C", vec![p])
        .with_label("inverse-functional shape")
        .unwrap();
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        ttl.contains("sh:path [ sh:inversePath <https://ex/p> ]"),
        "{ttl}"
    );
    assert!(ttl.contains("sh:severity sh:Warning"), "{ttl}");
    assert!(
        ttl.contains("sh:message \"each object has at most one subject via p\""),
        "{ttl}"
    );
    assert!(
        ttl.contains("rdf-schema#label> \"inverse-functional shape\""),
        "{ttl}"
    );
}

#[test]
fn or_and_xone_emit_sh_or_and_sh_xone_branch_lists() {
    // `owl:unionOf` → `sh:or ( [ … ] [ … ] )`; `owl:disjointUnionOf` → `sh:xone ( … )`.
    // Branches serialize in canonical (content-key sorted) order, deterministically.
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![
            prop(
                "https://ex/target",
                vec![ConstraintComponent::Or(vec![
                    ConstraintComponent::Class("https://ex/B".into()),
                    ConstraintComponent::Class("https://ex/A".into()),
                ])],
            ),
            prop(
                "https://ex/kind",
                vec![ConstraintComponent::Xone(vec![
                    ConstraintComponent::Class("https://ex/X".into()),
                    ConstraintComponent::Class("https://ex/Y".into()),
                ])],
            ),
        ],
    );
    let ttl = project_validation_shape_shacl(&s);
    assert!(
        ttl.contains("sh:or ( [ sh:class <https://ex/A> ] [ sh:class <https://ex/B> ] )"),
        "{ttl}"
    );
    assert!(
        ttl.contains("sh:xone ( [ sh:class <https://ex/X> ] [ sh:class <https://ex/Y> ] )"),
        "{ttl}"
    );
    // A clean (non-lossy) disjunction carries no SHACL residue but IS a ShEx-only drop.
    assert!(shacl_residue(&s).is_empty(), "{:?}", shacl_residue(&s));
    assert!(
        shex_residue(&s).iter().any(|r| r.contains("sh:or"))
            && shex_residue(&s).iter().any(|r| r.contains("sh:xone")),
        "{:?}",
        shex_residue(&s)
    );
}

#[test]
fn nested_lossy_branch_is_flagged_in_disjunction_residue() {
    // A Pattern nested inside a branch of sh:or must be flagged (never silently dropped).
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![ConstraintComponent::Or(vec![
                ConstraintComponent::Class("https://ex/A".into()),
                ConstraintComponent::Pattern {
                    regex: "^x".into(),
                    flags: None,
                },
            ])],
        )],
    );
    assert!(
        shacl_residue(&s)
            .iter()
            .any(|r| r.contains("regex-dialect residue")),
        "a Pattern in an sh:or branch must be flagged: {:?}",
        shacl_residue(&s)
    );
}

#[test]
fn empty_program_yields_empty_document() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None);
    assert_eq!(project_validation_shapes_shacl(&prog), "");
}

#[test]
fn multi_shape_document_carries_prefixes_and_all_shapes() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_validation_shapes(vec![
        shape("https://ex/A", "https://ex/CA", vec![]),
        shape("https://ex/B", "https://ex/CB", vec![]),
    ]);
    let doc = project_validation_shapes_shacl(&prog);
    assert!(doc.contains("@prefix sh:"), "{doc}");
    assert!(doc.contains("<https://ex/A> a sh:NodeShape"), "{doc}");
    assert!(doc.contains("<https://ex/B> a sh:NodeShape"), "{doc}");
}
