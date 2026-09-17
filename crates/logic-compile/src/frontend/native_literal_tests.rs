// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete literal identity in GMEOW formula/relational projection and restoration.

use super::*;
use crate::projections::rdf::project_canonical_rdf12_dataset;
use crate::relational_core::{
    lower_program_with_formulas, parse_relational_core, project_relational_core,
};
use purrdf::{RdfLiteral, RdfTextDirection};

mod acceptance;

fn directional(direction: RdfTextDirection) -> RdfLiteral {
    RdfLiteral {
        lexical_form: "same".to_owned(),
        datatype: None,
        language: Some("fr".to_owned()),
        direction: Some(direction),
    }
}

#[test]
fn native_formula_and_relational_roundtrip_retain_literal_identity() {
    let literals = [
        RdfLiteral::typed("same", "urn:datatype:a"),
        RdfLiteral::typed("same", "urn:datatype:b"),
        RdfLiteral::typed("same", format!("{}Variable", crate::ir::LOGIC_NAMESPACE)),
        directional(RdfTextDirection::Ltr),
        directional(RdfTextDirection::Rtl),
    ];
    let mut identities = std::collections::BTreeSet::new();
    for literal in literals {
        let formula = Formula::atom(
            Term::iri("urn:predicate").unwrap(),
            vec![
                Term::iri("urn:subject").unwrap(),
                Term::rdf_literal(literal.clone()).unwrap(),
            ],
        )
        .unwrap();
        let axiom = formula
            .as_horn_axiom()
            .expect("the native literal fits the compact IR");
        let program = LogicProgram::new(vec![axiom.clone()], vec![], vec![], None);
        let projection = project_canonical_rdf12_dataset(&program).unwrap();
        let (restored, diagnostics) = parse_logic_dataset(&projection.dataset, None).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(restored.formulas.is_empty());
        assert_eq!(restored.axioms, vec![axiom]);
        let lowered = lower_program_with_formulas(&restored);
        assert!(lowered.residue.is_empty());
        assert_eq!(
            lowered.facts[0].object,
            crate::relational_core::RcTerm::Literal(literal)
        );
        assert!(
            identities.insert(lowered.content_key().unwrap()),
            "literal identities cannot collapse"
        );
        let wire = project_relational_core(&lowered);
        let graph = purrdf::parse_dataset(wire.as_bytes(), "application/n-triples", None).unwrap();
        assert_eq!(
            parse_relational_core(&graph)
                .unwrap()
                .content_key()
                .unwrap(),
            lowered.content_key().unwrap()
        );
        let cache = serde_json::to_vec(&restored.axioms).unwrap();
        let cached: Vec<LogicAxiom> = serde_json::from_slice(&cache).unwrap();
        assert_eq!(cached, restored.axioms);
    }
}

#[test]
fn declared_datatype_cannot_override_native_literal_identity() {
    let source = r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <https://example.org/> .
        ex:f a logic:Formula; logic:relation ex:p;
          logic:argument [ logic:termIndex 0; logic:termLiteral "same"@fr--rtl;
                           logic:termLiteralDatatype <http://www.w3.org/2001/XMLSchema#string> ].
    "#;
    let (_, diagnostics) = parse_logic_str(source, None).unwrap();
    assert!(
        diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"
            && d.message.contains("disagrees with the native literal")),
        "{diagnostics:?}"
    );
}

#[test]
fn common_logic_views_preserve_directional_formula_terms() {
    let formula = Formula::Forall {
        vars: vec!["x".to_owned()],
        body: Box::new(
            Formula::atom(
                Term::iri("urn:predicate").unwrap(),
                vec![
                    Term::var("x").unwrap(),
                    Term::rdf_literal(directional(RdfTextDirection::Rtl)).unwrap(),
                ],
            )
            .unwrap(),
        ),
    };
    let program =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula.clone()]);
    let clif = crate::clif::project_clif(&program).unwrap().content;
    let cgif = crate::cgif::project_cgif(&program).unwrap().content;
    let xcl = crate::xcl::project_xcl(&program).unwrap().content;
    for (restored, diagnostics) in [
        crate::clif::parse_clif_str(&clif, None).unwrap(),
        crate::cgif::parse_cgif_str(&cgif, None).unwrap(),
        crate::xcl::parse_xcl_str(&xcl, None).unwrap(),
    ] {
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.code.contains("MALFORMED")),
            "{diagnostics:?}"
        );
        assert_eq!(restored.formulas, vec![formula.clone()]);
    }
}
