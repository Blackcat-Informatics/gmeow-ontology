// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_fact_dedup_negation_and_model_keys_preserve_datatype_identity() {
    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".into(),
        datatype: gmeow_term_arena::engine::RDF_LANG_STRING.into(),
        language: None,
        direction: None,
    };
    let fact = |object| Fact {
        subject: TermValue::iri("urn:s"),
        predicate: "urn:p".into(),
        object,
    };
    let mut store = FactStore::new();
    assert_eq!(store.insert(fact(plain.clone())), Some(0));
    assert_eq!(store.insert(fact(plain.clone())), None);
    assert!(!store.contains_key(&fact(langless.clone()).key()));
    let atom = EvalAtom::positive(
        EvalTerm::named("urn:s"),
        "urn:p",
        EvalTerm::ConstLit(langless.clone()),
    );
    assert!(!negated_atom_satisfied(
        &atom,
        &Solution {
            bindings: vec![],
            source_facts: vec![]
        },
        &store
    ));
    assert_eq!(store.insert(fact(langless.clone())), Some(1));
    assert!(negated_atom_satisfied(
        &atom,
        &Solution {
            bindings: vec![],
            source_facts: vec![]
        },
        &store
    ));
    assert_eq!(store.key_set().len(), 2);
    let cloned = store.clone();
    assert_eq!(cloned.row_index(&fact(langless).key()), Some(1));
    assert_eq!(cloned.row_index(&fact(plain).key()), Some(0));
}
