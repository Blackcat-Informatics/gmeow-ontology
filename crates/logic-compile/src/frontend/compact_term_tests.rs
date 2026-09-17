// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::AtomicTerm;

fn parse(text: &str) -> LogicProgram {
    let text = format!(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . {text}"
    );
    let (program, diagnostics) = parse_logic_str(&text, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    program
}

#[test]
fn compact_domain_literals_preserve_complete_native_values() {
    let program = parse(
        r#"
        <urn:s> logic:payload "?x" ; logic:typed "01"^^<http://www.w3.org/2001/XMLSchema#integer> ;
            logic:directional "right"@ar--rtl .
    "#,
    );
    let literal = |local: &str| {
        program
            .axioms
            .iter()
            .find(|atom| atom.predicate == logic_iri(local))
            .unwrap()
            .obj
            .as_literal()
            .unwrap()
    };
    assert_eq!(literal("payload").lexical_form, "?x");
    assert_eq!(
        literal("typed").datatype_iri(),
        "http://www.w3.org/2001/XMLSchema#integer"
    );
    assert_eq!(literal("typed").lexical_form, "01");
    assert_eq!(literal("directional").language.as_deref(), Some("ar"));
    assert_eq!(
        literal("directional").direction,
        Some(purrdf::RdfTextDirection::Rtl)
    );
    let projected = crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
    let (restored, diagnostics) = parse_logic_dataset(&projected.dataset, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    crate::adapter::assert_ir_isomorphic(&program, &restored).unwrap();
}

#[test]
fn rule_variables_and_question_mark_literals_remain_distinct_through_every_codec() {
    let program = parse(
        r#"
        <urn:rule> a logic:Rule ;
            logic:head [ rdf:subject "?s" ; rdf:predicate logic:answer ; rdf:object [ logic:termLiteral "?x" ] ] ;
            logic:body [ rdf:subject "?s" ; rdf:predicate logic:input ; rdf:object "?x" ] .
    "#,
    );
    assert!(matches!(program.rules[0].head.obj, AtomicTerm::Literal(_)));
    assert_eq!(
        program.rules[0].body[0].obj,
        AtomicTerm::Var("?x".to_owned())
    );
    let canonical = crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
    let (fixed, diagnostics) = parse_logic_dataset(&canonical.dataset, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(program.rules, fixed.rules);
    for restored in [
        crate::clif::parse_clif_str(&crate::clif::project_clif(&fixed).unwrap().content, None)
            .unwrap(),
        crate::cgif::parse_cgif_str(&crate::cgif::project_cgif(&fixed).unwrap().content, None)
            .unwrap(),
        crate::xcl::parse_xcl_str(&crate::xcl::project_xcl(&fixed).unwrap().content, None).unwrap(),
    ] {
        assert!(restored.1.is_empty(), "{:?}", restored.1);
        crate::adapter::assert_ir_isomorphic(&fixed, &restored.0).unwrap();
    }
}

#[test]
fn compact_external_predicates_and_variables_keep_their_declared_formula_carrier() {
    let source = r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        <urn:f> a logic:Formula; logic:relation <urn:external>;
            logic:argument [ logic:termIndex 0; logic:termVariable "subject" ],
                           [ logic:termIndex 1; logic:termVariable "object" ] .
    "#;
    let (program, diagnostics) = parse_logic_str(source, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(program.axioms.len(), 1);
    assert_eq!(program.axioms[0].obj.as_variable(), Some("?object"));
    let canonical = crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
    let (restored, diagnostics) = parse_logic_dataset(&canonical.dataset, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    crate::adapter::assert_ir_isomorphic(&program, &restored).unwrap();
    for restored in [
        crate::clif::parse_clif_str(&crate::clif::project_clif(&program).unwrap().content, None)
            .unwrap(),
        crate::cgif::parse_cgif_str(&crate::cgif::project_cgif(&program).unwrap().content, None)
            .unwrap(),
        crate::xcl::parse_xcl_str(&crate::xcl::project_xcl(&program).unwrap().content, None)
            .unwrap(),
    ] {
        assert!(restored.1.is_empty(), "{:?}", restored.1);
        crate::adapter::assert_ir_isomorphic(&program, &restored.0).unwrap();
    }
}

#[test]
fn external_scoped_claims_keep_separate_envelopes_through_canonical_projection() {
    for predicate in [
        "urn:p",
        "https://blackcatinformatics.ca/logic/testPredicate",
    ] {
        for include_global in [false, true] {
            let source = r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        <urn:a> a rdf:Statement; rdf:subject <urn:s>; rdf:predicate <urn:p>;
            rdf:object "right"@ar--rtl; logic:standpoint <urn:first>; logic:confidence 0.75 .
        <urn:b> a rdf:Statement; rdf:subject <urn:s>; rdf:predicate <urn:p>;
            rdf:object "right"@ar--rtl; logic:standpoint <urn:second>; logic:confidence 0.25 .
    "#;
            let mut source = source.replace("<urn:p>", &format!("<{predicate}>"));
            if include_global {
                source.push_str(&format!("<urn:s> <{predicate}> \"right\"@ar--rtl ."));
            }
            let (program, diagnostics) = parse_logic_str(&source, None).unwrap();
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            // An arbitrary external triple alone does not declare a logic assertion.
            let expected = 2 + usize::from(
                include_global && predicate.starts_with(crate::ir::LOGIC_NAMESPACE),
            );
            assert_eq!(program.axioms.len(), expected, "{:?}", program.axioms);
            let canonical =
                crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
            let (restored, diagnostics) = parse_logic_dataset(&canonical.dataset, None).unwrap();
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            crate::adapter::assert_ir_isomorphic(&program, &restored).unwrap();
        }
    }
}
#[test]
fn malformed_scoped_term_roles_are_errors_and_never_assertions() {
    for (subject, predicate) in [("<urn:s>", "\"urn:p\""), ("\"urn:s\"", "<urn:p>")] {
        let source = format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
             <urn:claim> a rdf:Statement; rdf:subject {subject}; rdf:predicate {predicate};
                 rdf:object <urn:o>; logic:standpoint <urn:world> ."
        );
        let (program, diagnostics) = parse_logic_str(&source, None).unwrap();
        assert!(program.axioms.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MALFORMED_SCOPED_AXIOM"
                    && diagnostic.severity == super::Severity::Error
                    && diagnostic.subject.as_deref() == Some("urn:claim")),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn contradictory_rule_term_carriers_reject_the_whole_rule() {
    for carrier in [
        r#"logic:termLiteral "?x" ; logic:termVariable "x""#,
        r#"logic:termLiteral "x"@en ; logic:termLiteralDatatype <http://www.w3.org/2001/XMLSchema#integer>"#,
    ] {
        let text = format!(
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            <urn:r> a logic:Rule ; logic:head [ rdf:subject "?s" ; rdf:predicate logic:p ; rdf:object [{carrier}] ] ."#
        );
        let (program, diagnostics) = parse_logic_str(&text, None).unwrap();
        assert!(program.rules.is_empty());
        assert!(
            diagnostics.iter().any(|d| d.code == "MALFORMED_RULE_HEAD"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn compact_keys_distinguish_literal_datatype_language_and_direction() {
    let values = [
        purrdf::RdfLiteral::simple("1"),
        purrdf::RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#integer"),
        purrdf::RdfLiteral::language_tagged("1", "en"),
        purrdf::RdfLiteral {
            lexical_form: "1".to_owned(),
            datatype: None,
            language: Some("en".to_owned()),
            direction: Some(purrdf::RdfTextDirection::Ltr),
        },
    ];
    let keys: std::collections::BTreeSet<_> = values
        .into_iter()
        .map(|literal| {
            LogicAxiom::ground("urn:s", "urn:p", AtomicTerm::Literal(literal))
                .unwrap()
                .sort_key()
        })
        .collect();
    assert_eq!(keys.len(), 4);
}

#[test]
fn datalog_omits_whole_rules_instead_of_equating_distinct_typed_literals() {
    let program = parse(
        r#"
        <urn:r> a logic:Rule ;
            logic:head [ rdf:subject "?s" ; rdf:predicate logic:answer ; rdf:object "?x" ] ;
            logic:body [ rdf:subject "?s" ; rdf:predicate logic:input ; rdf:object "01"^^<http://www.w3.org/2001/XMLSchema#integer> ] .
    "#,
    );
    let mut loss = crate::loss_ledger::LossLedger::new();
    let projection = crate::projections::text::project_datalog(&program, &mut loss);
    assert!(!projection.content.contains("answer("));
    assert!(
        loss.projection_drops_for("datalog")
            .iter()
            .any(|drop| drop.contains("entire axiom or rule"))
    );
}
