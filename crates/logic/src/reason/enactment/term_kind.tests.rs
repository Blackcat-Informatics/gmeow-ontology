// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent authored fixtures exercise the production integrity gate without a corpus.

use gmeow_logic_compile::{frontend::extract_all_constraints, ir::LogicProgram};

use super::enactment_gate_markers_with_laws;

const PREFIXES: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .
    @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
    @prefix ex: <https://example.org/> .";

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

fn bad_record() -> Vec<(String, String, String)> {
    vec![(
        "https://example.org/bad".into(),
        "https://example.org/KindLaw".into(),
        "https://example.org/BadMetadata".into(),
    )]
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
