// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_observation_preserves_literal_components_and_polarity() {
    let literal = purrdf::RdfLiteral {
        lexical_form: "directional".to_owned(),
        datatype: None,
        language: Some("ar".to_owned()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let atom = EvalAtom {
        subject: EvalTerm::Var("?s".to_owned()),
        predicate: "urn:p".to_owned(),
        object: EvalTerm::ConstLit(crate::rule_ir::literal_value(&literal)),
        negated: true,
    };
    let observed = inspect_atom(&atom).unwrap();
    let bytes = serde_json::to_vec(&observed).unwrap();
    let restored: RcAtom = serde_json::from_slice(&bytes).unwrap();
    assert!(restored.negated);
    assert_eq!(restored.subject, RcTerm::Var("?s".to_owned()));
    assert_eq!(restored.object, RcTerm::Literal(literal));
}

#[test]
fn observation_refuses_a_nonliteral_native_literal_slot() {
    let invalid = EvalTerm::ConstLit(purrdf::TermValue::Iri("urn:not-a-literal".to_owned()));
    assert!(inspect_term(&invalid).is_err());
}
