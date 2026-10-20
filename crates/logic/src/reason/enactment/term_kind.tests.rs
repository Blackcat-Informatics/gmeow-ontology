// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent authored fixtures exercise the production integrity gate without a corpus.

use gmeow_logic_compile::{frontend::extract_all_constraints, ir::LogicProgram};

use super::enactment_gate_markers_with_laws;

const PREFIXES: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .
    @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
    @prefix ex: <https://example.org/> .";

/// Run a selected node-kind law against independently authored RDF input.
fn findings(kind: &str, class_scoped: bool, data: &str) -> Vec<(String, String, String)> {
    let target = if class_scoped {
        "logic:onClass ex:Record ;"
    } else {
        ""
    };
    authored_findings(
        &format!(
            "
            ex:metadata a logic:AnnotationProperty .
            ex:KindLaw a logic:PathNodeKindConstraint ;
                {target} logic:valuePath ex:metadata ; logic:nodeKind \"{kind}\" ;
                logic:formalizes ex:Projection ; gmeow:enforcesFailureClass ex:BadMetadata ."
        ),
        data,
        1,
    )
}

/// Check the exact compiled rule count before executing an authored law through the native gate.
fn authored_findings(law: &str, data: &str, rule_count: usize) -> Vec<(String, String, String)> {
    let source = purrdf::parse_dataset(format!("{PREFIXES} {law}").as_bytes(), "text/turtle", None)
        .expect("independent authored law parses");
    let (constraints, diagnostics) = extract_all_constraints(&source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(constraints.len(), 1);
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(constraints);
    let laws = crate::relational_core::lower_violation_rules(&program).unwrap();
    assert!(laws.residue.is_empty(), "{:?}", laws.residue);
    assert_eq!(laws.rules.len(), rule_count);
    let edb = purrdf::parse_dataset(format!("{PREFIXES} {data}").as_bytes(), "text/turtle", None)
        .expect("independent input parses");
    enactment_gate_markers_with_laws(&edb, &[], &laws)
        .unwrap()
        .into_iter()
        .map(|finding| (finding.subject, finding.law, finding.failure_class))
        .collect()
}

/// Expect one finding with the original focus, law and failure-class identities.
fn bad_record() -> Vec<(String, String, String)> {
    vec![(
        "https://example.org/bad".into(),
        "https://example.org/KindLaw".into(),
        "https://example.org/BadMetadata".into(),
    )]
}

#[test]
fn finite_numeric_intrinsic_runs_through_the_native_integrity_gate() {
    let law = r#"
        ex:KindLaw a logic:Constraint ; logic:integrity ex:all ;
            logic:formalizes ex:Projection ; gmeow:enforcesFailureClass ex:BadMetadata .
        ex:all a logic:Formula ;
            logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ],
                                     [ logic:termIndex 1 ; logic:termVariable "value" ] ;
            logic:forall ex:implies .
        ex:implies a logic:Formula ; logic:antecedent ex:path ; logic:consequent ex:finite .
        ex:path a logic:Formula ; logic:relation ex:metadata ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
                           [ logic:termIndex 1 ; logic:termVariable "value" ] .
        ex:finite a logic:Formula ; logic:relation logic:termIsFiniteNumeric ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "value" ] .
    "#;
    for value in [
        "0",
        "-7",
        "2.5",
        "\"1.5\"^^<http://www.w3.org/2001/XMLSchema#double>",
    ] {
        assert!(authored_findings(law, &format!("ex:good ex:metadata {value} ."), 1).is_empty());
    }
    for value in [
        "\"text\"",
        "\"2.5\"",
        "\"oops\"^^<http://www.w3.org/2001/XMLSchema#double>",
        "ex:pole",
        "_:blank",
        "\"NaN\"^^<http://www.w3.org/2001/XMLSchema#double>",
        "\"INF\"^^<http://www.w3.org/2001/XMLSchema#double>",
    ] {
        assert_eq!(
            authored_findings(law, &format!("ex:bad ex:metadata {value} ."), 1),
            bad_record(),
            "{value}"
        );
    }
}

#[test]
fn question_wording_language_guard_runs_through_native_integrity() {
    let law = r#"
        ex:KindLaw a logic:Constraint ; logic:integrity ex:all ;
            logic:formalizes gmeow:questionText ; gmeow:enforcesFailureClass ex:BadMetadata .
        ex:all a logic:Formula ;
            logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ],
                                     [ logic:termIndex 1 ; logic:termVariable "value" ] ;
            logic:forall ex:implies .
        ex:implies a logic:Formula ; logic:antecedent ex:path ; logic:consequent ex:tagged .
        ex:path a logic:Formula ; logic:relation gmeow:questionText ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
                           [ logic:termIndex 1 ; logic:termVariable "value" ] .
        ex:tagged a logic:Formula ; logic:relation logic:termHasLang ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "value" ] .
    "#;
    for (value, valid) in [
        ("\"Where?\"@en", true),
        ("\"Où ?\"@fr", true),
        ("\"Where?\"@x-gmeow-english", true),
        ("\"Where?\"@en--ltr", true),
        ("\"أين؟\"@ar--rtl", true),
        ("\"Where?\"", false),
        (
            "\"Where?\"^^<http://www.w3.org/2001/XMLSchema#string>",
            false,
        ),
        ("42", false),
        ("ex:wording", false),
        ("_:wording", false),
        ("<<(ex:s ex:p ex:o)>>", false),
    ] {
        let actual = authored_findings(law, &format!("ex:bad gmeow:questionText {value} ."), 1);
        assert_eq!(actual, if valid { vec![] } else { bad_record() }, "{value}");
    }
    assert!(
        authored_findings(
            law,
            "ex:good gmeow:questionText \"Where?\"@en, \"Où ?\"@fr .",
            1,
        )
        .is_empty()
    );
    assert!(authored_findings(law, "ex:open a gmeow:Question .", 1).is_empty());
}

/// The measure law admits the named positive pole without treating it as a
/// number, and compares finite values exactly rather than rounding to binary floats.
#[test]
fn finite_numeric_measure_law_preserves_poles_and_exact_nonnegativity() {
    let law = r#"
        @prefix math: <https://blackcatinformatics.ca/math/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        math:MeasureResultNonNegativeConstraint a logic:Constraint ;
            logic:formalizes math:MeasureEvaluation ;
            gmeow:enforcesFailureClass math:UnderspecifiedMeasureEvaluation ;
            logic:integrity math:mrForall .
        math:mrForall a logic:Formula ;
            logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ],
                                     [ logic:termIndex 1 ; logic:termVariable "value" ] ;
            logic:forall math:mrImpl .
        math:mrImpl a logic:Formula ; logic:antecedent math:mrGuard ;
            logic:consequent math:mrAllowed .
        math:mrGuard a logic:Formula ; logic:and math:mrClass, math:mrValue .
        math:mrClass a logic:Formula ; logic:relation rdf:type ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
                           [ logic:termIndex 1 ; logic:termIri math:MeasureEvaluation ] .
        math:mrValue a logic:Formula ; logic:relation math:measureResult ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
                           [ logic:termIndex 1 ; logic:termVariable "value" ] .
        math:mrAllowed a logic:Formula ; logic:or math:mrPole, math:mrFiniteNonNegative .
        math:mrPole a logic:Formula ; logic:relation logic:termIn ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "value" ],
                           [ logic:termIndex 1 ; logic:termIri math:PositiveInfinity ] .
        math:mrFiniteNonNegative a logic:Formula ; logic:and math:mrFinite, math:mrNonNegative .
        math:mrFinite a logic:Formula ; logic:relation logic:termIsFiniteNumeric ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "value" ] .
        math:mrNonNegative a logic:Formula ; logic:relation logic:termGreaterEqual ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable "value" ],
                           [ logic:termIndex 1 ; logic:termLiteral "0"^^xsd:integer ] .
    "#;
    let large_integer = "9".repeat(320);
    let tiny_positive = format!("0.{}1", "0".repeat(320));
    let tiny_negative = format!("-{tiny_positive}");
    for (value, valid) in [
        ("0", true),
        ("2.5", true),
        (large_integer.as_str(), true),
        (tiny_positive.as_str(), true),
        (tiny_negative.as_str(), false),
        ("-0.25", false),
        (
            "<https://blackcatinformatics.ca/math/PositiveInfinity>",
            true,
        ),
        (
            "<https://blackcatinformatics.ca/math/NegativeInfinity>",
            false,
        ),
        ("<urn:other>", false),
        ("_:blank", false),
        ("\"text\"", false),
        ("\"2.5\"", false),
        ("\"2.5\"@en", false),
        ("\"oops\"^^<http://www.w3.org/2001/XMLSchema#double>", false),
        ("\"NaN\"^^<http://www.w3.org/2001/XMLSchema#double>", false),
        ("\"INF\"^^<http://www.w3.org/2001/XMLSchema#double>", false),
        ("\"-INF\"^^<http://www.w3.org/2001/XMLSchema#float>", false),
    ] {
        let data = format!(
            "ex:evaluation a <https://blackcatinformatics.ca/math/MeasureEvaluation> ;
             <https://blackcatinformatics.ca/math/measureResult> {value} ."
        );
        let actual = authored_findings(law, &data, 2);
        if valid {
            assert!(actual.is_empty(), "{value}: {actual:?}");
        } else {
            assert_eq!(
                actual,
                vec![(
                    "https://example.org/evaluation".to_owned(),
                    "https://blackcatinformatics.ca/math/MeasureResultNonNegativeConstraint"
                        .to_owned(),
                    "https://blackcatinformatics.ca/math/UnderspecifiedMeasureEvaluation"
                        .to_owned(),
                )],
                "{value}"
            );
        }
    }
    for (relation, accepted) in [
        ("termLess", [true, false, false]),
        ("termLessEqual", [true, true, false]),
        ("termGreater", [false, false, true]),
        ("termGreaterEqual", [false, true, true]),
    ] {
        let selected = law.replace("logic:termGreaterEqual", &format!("logic:{relation}"));
        for (value, valid) in ["-0.25", "0", "0.25"].into_iter().zip(accepted) {
            let data = format!(
                "ex:evaluation a <https://blackcatinformatics.ca/math/MeasureEvaluation> ;
                 <https://blackcatinformatics.ca/math/measureResult> {value} ."
            );
            assert_eq!(
                authored_findings(&selected, &data, 2).is_empty(),
                valid,
                "{relation}({value}, 0)"
            );
        }
    }
}

#[test]
fn iri_kind_enforces_native_values_and_exact_law_identity() {
    assert_eq!(
        findings(
            "IRI",
            true,
            r#"
        ex:good a ex:Record ; ex:metadata ex:Value .
        ex:bad a ex:Record ; ex:metadata ex:Value, "https://example.org/Value" .
        ex:unrelated a ex:Other ; ex:metadata "not an IRI" .
        ex:absent a ex:Record .
        ex:wrongPath a ex:Record ; ex:other "not an IRI" .
    "#
        ),
        bad_record()
    );
}

#[test]
fn iri_kind_rejects_blank_values_without_changing_the_focus_identity() {
    assert_eq!(
        findings("IRI", true, "ex:bad a ex:Record ; ex:metadata _:value ."),
        bad_record()
    );
}

#[test]
fn literal_kind_accepts_datatype_language_and_direction_without_coercion() {
    assert_eq!(
        findings(
            "Literal",
            true,
            r#"
        ex:text a ex:Record ; ex:metadata "hello" .
        ex:typed a ex:Record ; ex:metadata "7"^^<http://www.w3.org/2001/XMLSchema#integer> .
        ex:language a ex:Record ; ex:metadata "bonjour"@fr .
        ex:direction a ex:Record ; ex:metadata "hello"@en--ltr .
        ex:bad a ex:Record ; ex:metadata ex:Value .
    "#
        ),
        bad_record()
    );
}

#[test]
fn blank_or_iri_kind_accepts_blank_values_and_rejects_literals() {
    assert_eq!(
        findings(
            "BlankNodeOrIRI",
            true,
            r#"
        ex:iri a ex:Record ; ex:metadata ex:Value .
        ex:blank a ex:Record ; ex:metadata _:value .
        ex:bad a ex:Record ; ex:metadata "_:value" .
    "#
        ),
        bad_record()
    );
}

#[test]
fn predicate_scoped_kind_checks_every_subject_with_the_selected_path() {
    assert_eq!(
        findings(
            "IRI",
            false,
            r#"
        ex:good ex:metadata ex:Value .
        ex:bad ex:metadata "value" .
        ex:unrelated ex:other "value" .
    "#
        ),
        bad_record()
    );
}

#[test]
fn triple_values_do_not_masquerade_as_any_supported_node_kind() {
    for kind in ["IRI", "Literal", "BlankNodeOrIRI"] {
        assert_eq!(
            findings(
                kind,
                true,
                "ex:bad a ex:Record ; ex:metadata <<(ex:s ex:p ex:o)>> ."
            ),
            bad_record(),
            "{kind}"
        );
    }
}

#[test]
fn malformed_kind_is_a_diagnostic_not_a_silently_omitted_check() {
    let source = purrdf::parse_dataset(
        format!(
            r#"{PREFIXES}
        ex:KindLaw a logic:PathNodeKindConstraint ; logic:onClass ex:Record ;
            logic:valuePath ex:metadata ; logic:nodeKind "NotARealKind" ;
            logic:formalizes ex:Projection ; gmeow:enforcesFailureClass ex:BadMetadata .
    "#
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    let (constraints, diagnostics) = extract_all_constraints(&source);
    assert!(constraints.is_empty());
    assert!(!diagnostics.is_empty());
}

/// Distinct resource values violate single-valued metadata only on the selected class.
#[test]
fn required_single_metadata_still_rejects_distinct_iri_values() {
    assert_eq!(
        authored_findings(
            r#"
            ex:KindLaw a logic:ComparisonConstraint ; logic:onClass ex:Record ;
                logic:leftPath ex:metadata ; logic:rightPath ex:metadata ;
                logic:compareOp "!=" ; logic:formalizes ex:Projection ;
                gmeow:enforcesFailureClass ex:BadMetadata .
        "#,
            r#"
            ex:good a ex:Record ; ex:metadata ex:One .
            ex:bad a ex:Record ; ex:metadata ex:One, ex:Two .
            ex:unrelated a ex:Other ; ex:metadata ex:One, ex:Two .
        "#,
            1,
        ),
        bad_record()
    );
}

/// Exercise canonical value-comparison sugar through native enactment.
fn comparison_findings(operator: &str, data: &str) -> Vec<(String, String, String)> {
    authored_findings(
        &format!(
            r#"
        ex:KindLaw a logic:ComparisonConstraint ; logic:onClass ex:Record ;
            logic:leftPath ex:left ; logic:rightPath ex:right ;
            logic:compareOp "{operator}" ; logic:formalizes ex:Projection ;
            gmeow:enforcesFailureClass ex:BadMetadata .
    "#
        ),
        data,
        1,
    )
}

/// Either missing role violates co-occurrence; complete, absent and unrelated records pass.
#[test]
fn cooccurrence_guards_preserve_both_directions_and_absence() {
    let law = r#"
        ex:KindLaw a logic:CrossNodeConstraint ; logic:onClass ex:Record ;
            logic:roleA ex:left ; logic:roleB ex:right ; logic:crossMode "co-occur" ;
            logic:formalizes ex:Projection ; gmeow:enforcesFailureClass ex:BadMetadata .
    "#;
    for data in [
        "ex:bad a ex:Record ; ex:left ex:Value .",
        "ex:bad a ex:Record ; ex:right ex:Value .",
    ] {
        assert_eq!(authored_findings(law, data, 2), bad_record());
    }
    assert!(
        authored_findings(
            law,
            "ex:complete a ex:Record ; ex:left ex:A ; ex:right ex:B .
         ex:absent a ex:Record . ex:unrelated a ex:Other ; ex:left ex:A .",
            2,
        )
        .is_empty()
    );
}

#[test]
fn equality_filters_use_native_value_semantics_and_preserve_source_polarity() {
    assert_eq!(
        comparison_findings(
            "=",
            r#"
        ex:bad a ex:Record ; ex:left 1 ; ex:right 1.0 .
        ex:good a ex:Record ; ex:left ex:One ; ex:right ex:Two .
    "#
        ),
        bad_record()
    );
    assert_eq!(
        comparison_findings(
            "!=",
            r#"
        ex:good a ex:Record ; ex:left 1 ; ex:right 1.0 .
        ex:bad a ex:Record ; ex:left ex:One ; ex:right ex:Two .
    "#
        ),
        bad_record()
    );
}

#[test]
fn equality_expression_errors_drop_only_the_affected_candidate() {
    assert_eq!(
        comparison_findings(
            "!=",
            r#"
        ex:unknown a ex:Record ; ex:left "a"^^ex:Unknown ; ex:right "b"^^ex:Unknown .
        ex:illTyped a ex:Record ; ex:left "not an integer"^^<http://www.w3.org/2001/XMLSchema#integer> ; ex:right 1 .
        ex:bad a ex:Record ; ex:left ex:One ; ex:right ex:Two .
    "#
        ),
        bad_record()
    );
}
