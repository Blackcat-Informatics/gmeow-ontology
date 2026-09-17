// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed canonical formula matching through the production native program adapter.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTextDirection};

#[test]
fn equal_lexical_values_only_fire_the_rule_with_their_complete_identity() {
    let literals = [
        RdfLiteral::simple("1"),
        RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#integer"),
        RdfLiteral {
            lexical_form: "1".to_owned(),
            datatype: None,
            language: Some("fr".to_owned()),
            direction: Some(RdfTextDirection::Ltr),
        },
        RdfLiteral {
            lexical_form: "1".to_owned(),
            datatype: None,
            language: Some("fr".to_owned()),
            direction: Some(RdfTextDirection::Rtl),
        },
    ];
    let formulas = literals
        .iter()
        .enumerate()
        .map(|(index, literal)| Formula::Forall {
            vars: vec!["x".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::iri("urn:seed").unwrap(),
                        vec![
                            Term::var("x").unwrap(),
                            Term::rdf_literal(literal.clone()).unwrap(),
                        ],
                    )
                    .unwrap(),
                ),
                Box::new(
                    Formula::atom(
                        Term::iri("urn:selected").unwrap(),
                        vec![
                            Term::var("x").unwrap(),
                            Term::iri(format!("urn:case:{index}")).unwrap(),
                        ],
                    )
                    .unwrap(),
                ),
            )),
        })
        .collect();
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(formulas);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    assert!(prepared.preservation.unsupported_constructs.is_empty());
    for (index, literal) in literals.into_iter().enumerate() {
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::iri("urn:subject"),
            "urn:seed",
            RdfTerm::literal(literal),
        ));
        let result = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        )
        .unwrap();
        let matches: Vec<_> = result
            .inferred
            .iter()
            .filter(|fact| fact.predicate == "urn:selected")
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "only the admitted complete literal may match"
        );
        assert_eq!(
            matches[0].object,
            purrdf::TermValue::iri(format!("urn:case:{index}"))
        );
    }
}
