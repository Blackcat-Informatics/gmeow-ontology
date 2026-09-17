// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native rule consequences must reach structural DL readers in their own world.

use super::*;
use crate::physical::SelectedDomains;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

fn functional_values(first: RdfLiteral, second: RdfLiteral) -> Arc<RdfDataset> {
    source(vec![
        resource(
            "urn:value",
            TYPE,
            "http://www.w3.org/2002/07/owl#FunctionalProperty",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:value",
            RdfTerm::literal(first),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:pending-value",
            RdfTerm::literal(second),
        ),
    ])
}

fn tagged(text: &str, language: &str, direction: Option<purrdf::RdfTextDirection>) -> RdfLiteral {
    RdfLiteral {
        lexical_form: text.into(),
        datatype: None,
        language: Some(language.into()),
        direction,
    }
}

#[test]
fn functional_clashes_use_value_identity_without_erasing_source_spellings() {
    let cases = [
        ("1", "integer", "1.0", "decimal"),
        ("true", "boolean", "1", "boolean"),
        ("1", "float", "1.0", "float"),
        ("1", "double", "1.0", "double"),
        (
            "2026-09-09T08:00:00Z",
            "dateTime",
            "2026-09-09T09:00:00+01:00",
            "dateTime",
        ),
    ];
    let program = copying_program(&[("urn:pending-value", "urn:value")]);
    for (a, ad, b, bd) in cases {
        let input = functional_values(
            RdfLiteral::typed(a, format!("http://www.w3.org/2001/XMLSchema#{ad}")),
            RdfLiteral::typed(b, format!("http://www.w3.org/2001/XMLSchema#{bd}")),
        );
        let result = super::super::reason_program(
            &program,
            prepare_reasoning_input(&input).unwrap(),
            &SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        assert!(
            !clash(&result, "urn:item", super::super::rl::DEFAULT_WORLD),
            "equal {ad}/{bd} values cannot clash"
        );
        let source_forms: std::collections::BTreeSet<_> = result
            .inferred()
            .iter()
            .filter(|row| row.subject == "urn:item" && row.predicate == "urn:value")
            .map(|row| &row.object)
            .collect();
        assert_eq!(
            source_forms.len(),
            2,
            "value equality must retain both source term spellings"
        );
    }
    let input = functional_values(
        RdfLiteral::typed("0.5", "http://www.w3.org/2001/XMLSchema#decimal"),
        RdfLiteral::typed("1/2", "http://www.w3.org/2002/07/owl#rational"),
    );
    assert!(!clash(
        &super::super::reason_program(
            &program,
            prepare_reasoning_input(&input).unwrap(),
            &SelectedDomains::new([]).unwrap(),
        )
        .unwrap(),
        "urn:item",
        super::super::rl::DEFAULT_WORLD
    ));
}

#[test]
fn functional_clashes_preserve_language_direction_and_signed_zero_distinctions() {
    use purrdf::RdfTextDirection::{Ltr, Rtl};
    let cases = [
        (tagged("word", "en", None), tagged("word", "fr", None)),
        (
            tagged("word", "ar", Some(Ltr)),
            tagged("word", "ar", Some(Rtl)),
        ),
        (
            RdfLiteral::typed("0", "http://www.w3.org/2001/XMLSchema#double"),
            RdfLiteral::typed("-0", "http://www.w3.org/2001/XMLSchema#double"),
        ),
        (
            RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#integer"),
            RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#double"),
        ),
    ];
    let program = copying_program(&[("urn:pending-value", "urn:value")]);
    for (a, b) in cases {
        let input = functional_values(a, b);
        assert!(clash(
            &super::super::reason_program(
                &program,
                prepare_reasoning_input(&input).unwrap(),
                &SelectedDomains::new([]).unwrap(),
            )
            .unwrap(),
            "urn:item",
            super::super::rl::DEFAULT_WORLD
        ));
    }
}

#[test]
fn negative_assertion_compares_derived_values_and_keeps_actual_premises() {
    let input = source(vec![
        resource(
            "urn:negative",
            TYPE,
            "http://www.w3.org/2002/07/owl#NegativePropertyAssertion",
        ),
        resource(
            "urn:negative",
            "http://www.w3.org/2002/07/owl#sourceIndividual",
            "urn:item",
        ),
        resource(
            "urn:negative",
            "http://www.w3.org/2002/07/owl#assertionProperty",
            "urn:value",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:negative"),
            "http://www.w3.org/2002/07/owl#targetValue",
            RdfTerm::literal(RdfLiteral::typed(
                "1.0",
                "http://www.w3.org/2001/XMLSchema#decimal",
            )),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:pending-value",
            RdfTerm::literal(RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ),
    ]);
    let result = super::super::reason_program(
        &copying_program(&[("urn:pending-value", "urn:value")]),
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let witness = result
        .inferred()
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:negative-property-assertion-clash"))
        .unwrap();
    assert!(witness.premises.contains(&(
        "urn:item".into(),
        "urn:value".into(),
        "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>".into()
    )));
    assert!(
        witness.premises.iter().any(
            |(s, p, _)| s == "urn:negative" && p == "http://www.w3.org/2002/07/owl#targetValue"
        )
    );
}

#[test]
fn disjoint_properties_compare_values_and_distinguish_quoted_triples() {
    let quoted = |object: &str| {
        RdfTerm::triple(purrdf::RdfTriple::new(
            RdfTerm::iri("urn:s"),
            "urn:p",
            RdfTerm::iri(object),
        ))
    };
    let cases = [
        (
            RdfTerm::literal(RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
            RdfTerm::literal(RdfLiteral::typed(
                "1.0",
                "http://www.w3.org/2001/XMLSchema#decimal",
            )),
            true,
        ),
        (quoted("urn:a"), quoted("urn:b"), false),
        (quoted("urn:a"), quoted("urn:a"), true),
    ];
    for (a, b, expected) in cases {
        let input = source(vec![
            resource(
                "urn:left",
                "http://www.w3.org/2002/07/owl#propertyDisjointWith",
                "urn:right",
            ),
            RdfQuad::new(RdfTerm::iri("urn:item"), "urn:left", a),
            RdfQuad::new(RdfTerm::iri("urn:item"), "urn:right", b),
        ]);
        let result = super::super::reason_program(
            &copying_program(&[]),
            prepare_reasoning_input(&input).unwrap(),
            &SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            clash(&result, "urn:item", super::super::rl::DEFAULT_WORLD),
            expected
        );
    }
}

#[test]
fn unknown_value_interpretations_withhold_only_the_affected_law() {
    let mut rows = vec![
        resource(
            "urn:value",
            TYPE,
            "http://www.w3.org/2002/07/owl#FunctionalProperty",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:value",
            RdfTerm::literal(RdfLiteral::typed("a", "urn:uninterpreted")),
        ),
    ];
    let program = copying_program(&[]);
    let domains = SelectedDomains::new([]).unwrap();
    let single = super::super::reason_program(
        &program,
        prepare_reasoning_input(&source(rows.clone())).unwrap(),
        &domains,
    )
    .unwrap()
    .native_verdict()
    .unwrap();
    assert!(single.gaps.is_empty());
    rows.push(RdfQuad::new(
        RdfTerm::iri("urn:item"),
        "urn:value",
        RdfTerm::literal(RdfLiteral::typed("b", "urn:uninterpreted")),
    ));
    let pair = super::super::reason_program(
        &program,
        prepare_reasoning_input(&source(rows)).unwrap(),
        &domains,
    )
    .unwrap()
    .native_verdict()
    .unwrap();
    assert!(pair.inconsistencies.is_empty());
    assert!(
        !pair.gaps.is_empty(),
        "uninterpreted lexical inequality cannot establish consistency"
    );
}

#[test]
fn key_collision_uses_shared_numeric_value_identity() {
    let input = source(vec![
        resource(
            "urn:class",
            "http://www.w3.org/2002/07/owl#hasKey",
            "urn:list",
        ),
        resource(
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "urn:value",
        ),
        resource(
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ),
        resource("urn:a", TYPE, "urn:class"),
        resource("urn:b", TYPE, "urn:class"),
        resource(
            "urn:a",
            "http://www.w3.org/2002/07/owl#differentFrom",
            "urn:b",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:a"),
            "urn:value",
            RdfTerm::literal(RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:b"),
            "urn:value",
            RdfTerm::literal(RdfLiteral::typed(
                "1.0",
                "http://www.w3.org/2001/XMLSchema#decimal",
            )),
        ),
    ]);
    let result = super::super::reason_program(
        &copying_program(&[]),
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(clash(&result, "urn:a", super::super::rl::DEFAULT_WORLD));
}

fn copying_program(predicates: &[(&str, &str)]) -> LogicProgram {
    let formulas = predicates
        .iter()
        .map(|(source, target)| {
            let atom = |predicate: &str| {
                Formula::atom(
                    Term::iri(predicate).unwrap(),
                    vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
                )
                .unwrap()
            };
            Formula::Forall {
                vars: vec!["x".into(), "y".into()],
                body: Box::new(Formula::Implies(
                    Box::new(atom(source)),
                    Box::new(atom(target)),
                )),
            }
        })
        .collect();
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(formulas)
}

fn source(rows: Vec<RdfQuad>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(&row);
    }
    builder.freeze().unwrap()
}

fn resource(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
}

fn clash(result: &crate::result::ReasoningResult, subject: &str, world: &str) -> bool {
    result.inferred().iter().any(|row| {
        row.subject == subject
            && row.predicate == TYPE
            && crate::provenance::term_display(&row.object) == format!("<{NOTHING}>")
            && row.world == world
    })
}

#[test]
fn derived_literal_enters_the_functional_value_clash_index() {
    let program = copying_program(&[("urn:pending-value", "urn:value")]);
    let input = source(vec![
        resource(
            "urn:value",
            TYPE,
            "http://www.w3.org/2002/07/owl#FunctionalProperty",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:value",
            RdfTerm::literal(RdfLiteral::simple("first")),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:item"),
            "urn:pending-value",
            RdfTerm::literal(RdfLiteral::simple("second")),
        ),
    ]);
    let result = super::super::reason_program(
        &program,
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(clash(&result, "urn:item", super::super::rl::DEFAULT_WORLD));
    let witness = result
        .inferred()
        .iter()
        .find(|row| {
            row.subject == "urn:item"
                && crate::provenance::term_display(&row.object) == format!("<{NOTHING}>")
        })
        .unwrap();
    let derived = result
        .inferred()
        .iter()
        .find(|row| row.subject == "urn:item" && row.predicate == "urn:value" && !row.is_edb)
        .unwrap();
    assert_eq!(witness.world, derived.world);
    assert!(
        witness.premises.contains(&(
            derived.subject.clone(),
            derived.predicate.clone(),
            crate::provenance::term_display(&derived.object)
        )),
        "clash evidence must name the exact native consequence and its reifier identity"
    );
}

#[test]
fn derived_restriction_fields_reach_dl_without_crossing_worlds() {
    let program = copying_program(&[
        (
            "urn:property",
            "https://blackcatinformatics.ca/logic/onProperty",
        ),
        (
            "urn:filler",
            "https://blackcatinformatics.ca/logic/allValuesFrom",
        ),
    ]);
    let mut rows = vec![
        resource("urn:restriction", "urn:property", "urn:edge"),
        resource("urn:restriction", "urn:filler", NOTHING),
        resource("urn:item", TYPE, "urn:restriction"),
        resource("urn:item", "urn:edge", "urn:target"),
    ];
    for row in &mut rows {
        row.graph_name = Some(RdfTerm::iri("urn:world:a"));
    }
    let mut unrelated = resource("urn:other", "urn:edge", "urn:unrelated");
    unrelated.graph_name = Some(RdfTerm::iri("urn:world:b"));
    rows.push(unrelated);
    let result = super::super::reason_program(
        &program,
        prepare_reasoning_input(&source(rows)).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(clash(&result, "urn:target", "urn:world:a"));
    assert!(!clash(&result, "urn:unrelated", "urn:world:b"));
}

#[test]
fn derived_list_cells_reach_nominal_membership() {
    let program = copying_program(&[
        (
            "urn:first",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
        ),
        (
            "urn:rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
        ),
    ]);
    let input = source(vec![
        resource(
            "urn:class",
            "https://blackcatinformatics.ca/logic/oneOf",
            "urn:list",
        ),
        resource("urn:list", "urn:first", "urn:member"),
        resource(
            "urn:list",
            "urn:rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ),
    ]);
    let result = super::super::reason_program(
        &program,
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(
        result
            .inferred()
            .iter()
            .any(|row| row.subject == "urn:member"
                && row.predicate == TYPE
                && crate::provenance::term_display(&row.object) == "<urn:class>")
    );
}

#[test]
fn derived_qualified_maximum_is_enforced_by_the_dl_continuation() {
    let program = copying_program(&[(
        "urn:maximum",
        "https://blackcatinformatics.ca/logic/maxQualifiedCardinality",
    )]);
    let input = source(vec![
        resource(
            "urn:restriction",
            "https://blackcatinformatics.ca/logic/onProperty",
            "urn:edge",
        ),
        resource(
            "urn:restriction",
            "https://blackcatinformatics.ca/logic/onClass",
            "urn:class",
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:restriction"),
            "urn:maximum",
            RdfTerm::literal(RdfLiteral::typed(
                "0",
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            )),
        ),
        resource("urn:item", TYPE, "urn:restriction"),
        resource("urn:item", "urn:edge", "urn:target"),
        resource("urn:target", TYPE, "urn:class"),
    ]);
    let result = super::super::reason_program(
        &program,
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(clash(&result, "urn:item", super::super::rl::DEFAULT_WORLD));
}

#[test]
fn program_continuation_publishes_only_derived_native_rows() {
    let program = copying_program(&[("urn:edge", "urn:ready")]);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let mut rows: Vec<_> = (0..128)
        .map(|n| resource(&format!("urn:item:{n}"), "urn:metadata", "urn:value"))
        .collect();
    rows.push(resource("urn:subject", "urn:edge", "urn:object"));
    let input = source(rows);
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        closure.inferred.iter().filter(|row| row.is_edb).count(),
        129
    );
    let committed: Vec<_> = closure.inferred.iter().filter(|row| !row.is_edb).collect();
    assert_eq!(committed.len(), 1, "only the rule consequence is published");
    assert_eq!(committed[0].predicate, "urn:ready");
    assert!(
        closure
            .inferred
            .iter()
            .any(|row| row.predicate == "urn:ready")
    );
}

#[test]
fn derived_directional_literal_survives_the_public_closure_projection() {
    let program = copying_program(&[("urn:pending-label", "urn:label")]);
    let literal = RdfLiteral {
        lexical_form: "shared lexical form".into(),
        datatype: None,
        language: Some("ar".into()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let input = source(vec![RdfQuad::new(
        RdfTerm::iri("urn:item"),
        "urn:pending-label",
        RdfTerm::literal(literal),
    )]);
    let output = super::super::reason_program_closure_dataset(
        &program,
        prepare_reasoning_input(&input).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let row = output
        .owned_quads()
        .find(|row| row.predicate == "urn:label")
        .unwrap();
    let RdfTerm::Literal(value) = row.object else {
        panic!("derived literal");
    };
    assert_eq!(value.language.as_deref(), Some("ar"));
    assert_eq!(value.direction, Some(purrdf::RdfTextDirection::Rtl));
}
