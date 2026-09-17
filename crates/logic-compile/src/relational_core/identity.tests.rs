// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn atom(subject: RcTerm, object: RcTerm) -> RcAtom {
    RcAtom {
        subject,
        predicate: "urn:predicate".to_owned(),
        object,
        negated: false,
    }
}

fn program() -> RelationalCoreProgram {
    let first = RcTerm::Blank("a".to_owned());
    let second = RcTerm::Blank("z".to_owned());
    RelationalCoreProgram {
        facts: vec![atom(first.clone(), second.clone())],
        rules: vec![RcRule {
            numeric: Vec::new(),
            head: atom(first.clone(), RcTerm::Var("?x".to_owned())),
            head_conjuncts: vec![atom(second.clone(), RcTerm::Var("?x".to_owned()))],
            body: vec![
                atom(first, RcTerm::Var("?x".to_owned())),
                atom(second, RcTerm::Iri("urn:constant".to_owned())),
            ],
            distinct_pairs: vec![],
        }],
        residue: vec![],
        source_iri: Some("urn:source".to_owned()),
    }
}

fn rename(term: &mut RcTerm) {
    if let RcTerm::Blank(label) = term {
        *label = if label == "a" { "zz" } else { "aa" }.to_owned();
    }
}

#[test]
fn program_identity_preserves_shared_blanks_under_renaming_and_reordering() {
    let original = program();
    let mut changed = original.clone();
    for atom in changed
        .facts
        .iter_mut()
        .chain(changed.rules.iter_mut().flat_map(|r| {
            std::iter::once(&mut r.head)
                .chain(&mut r.head_conjuncts)
                .chain(&mut r.body)
        }))
    {
        rename(&mut atom.subject);
        rename(&mut atom.object);
    }
    changed.rules[0].body.reverse();
    assert_eq!(
        original.content_key().unwrap(),
        changed.content_key().unwrap()
    );

    // Facts and rules each remain locally isomorphic, but the rule no longer
    // refers to the fact's subject. Independent per-part canonicalizers miss it.
    let mut detached = original.clone();
    let rule = &mut detached.rules[0];
    for atom in std::iter::once(&mut rule.head)
        .chain(&mut rule.head_conjuncts)
        .chain(&mut rule.body)
    {
        for term in [&mut atom.subject, &mut atom.object] {
            if let RcTerm::Blank(label) = term
                && label == "a"
            {
                *label = "detached".to_owned();
            }
        }
    }
    assert_ne!(
        original.content_key().unwrap(),
        detached.content_key().unwrap()
    );
}

#[test]
fn fact_polarity_and_source_presence_affect_identity() {
    let original = program();
    let mut changed = original.clone();
    changed.facts[0].negated = true;
    assert_ne!(
        original.content_key().unwrap(),
        changed.content_key().unwrap()
    );
    changed = original.clone();
    changed.source_iri = None;
    let absent = changed.content_key().unwrap();
    changed.source_iri = Some(String::new());
    assert_ne!(absent, changed.content_key().unwrap());
}

#[test]
fn native_literal_identity_does_not_collide_with_the_variable_envelope() {
    let mut value = program();
    value.facts[0].object = RcTerm::Var("?x".to_owned());
    let variable = value.content_key().unwrap();
    value.facts[0].object = RcTerm::Literal(RdfLiteral::typed("?x", format!("{NS}variable")));
    assert_ne!(variable, value.content_key().unwrap());
}

#[test]
fn malformed_native_input_returns_an_error_without_a_fallback_key() {
    let mut value = program();
    value.facts[0].predicate = "relative predicate".to_owned();
    assert!(value.content_key().is_err());
    value = program();
    value.facts[0].predicate = "urn:purrdf:rdfc:reifies".to_owned();
    assert!(
        value.content_key().is_err(),
        "canonicalization refusal must propagate"
    );
    value = program();
    value.facts[0].object = RcTerm::Literal(RdfLiteral {
        lexical_form: "x".to_owned(),
        datatype: None,
        language: None,
        direction: Some(purrdf::RdfTextDirection::Rtl),
    });
    assert!(value.content_key().is_err());
}

#[test]
fn framed_atom_and_rule_keys_separate_embedded_delimiters() {
    let left = atom(
        RcTerm::Var("?x\u{1e}urn:predicate".to_owned()),
        RcTerm::Var("?y".to_owned()),
    );
    let mut right = atom(RcTerm::Var("?x".to_owned()), RcTerm::Var("?y".to_owned()));
    right.predicate = "urn:predicate\u{1e}urn:predicate".to_owned();
    assert_ne!(left.key(), right.key());
    let mut first = program().rules.remove(0);
    first.distinct_pairs = vec![("?x\u{1f}?y".to_owned(), "?z".to_owned())];
    let mut second = first.clone();
    second.distinct_pairs = vec![("?x".to_owned(), "?y\u{1f}?z".to_owned())];
    assert_ne!(first.key(), second.key());
}
