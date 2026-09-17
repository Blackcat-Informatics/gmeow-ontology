// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{LogicProgram, Term};

fn formula(variable: &str, predicate: &str) -> Formula {
    let atom = |relation: &str| Formula::Atom {
        relation: Term::Iri(relation.into()),
        args: vec![
            Term::Var(variable.into()),
            Term::Iri("urn:analysis:object".into()),
        ],
    };
    Formula::Forall {
        vars: vec![variable.into()],
        body: Box::new(Formula::Implies(
            Box::new(atom("urn:analysis:input")),
            Box::new(atom(predicate)),
        )),
    }
}

#[test]
fn carrier_and_native_adapter_share_exact_source_clausification() {
    let source = formula("authored", "urn:analysis:output");
    let cache = Mutex::new(Cache::new());
    let first = analyze_cached(&source, &cache);
    assert!(Arc::ptr_eq(&first, &analyze_cached(&source, &cache)));
    let other = formula("renamed", "urn:analysis:output");
    assert_eq!(source.content_key(), other.content_key());
    let renamed = analyze_cached(&other, &cache);
    assert!(!Arc::ptr_eq(&first, &renamed));
    assert_ne!(first.rules, renamed.rules);
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![source]);
    let (rules, residue) = super::super::lower_formulas_to_rc(&program);
    assert_eq!(rules, first.rules);
    assert_eq!(residue, first.residue);
}

#[test]
fn oversized_lowering_is_recomputed_without_dropping_rules() {
    let source = Formula::And(
        (0..=MAX_RULES)
            .map(|index| formula("x", &format!("urn:analysis:large:{index}")))
            .collect(),
    );
    let first = analyze(&source);
    let second = analyze(&source);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.rules.len(), MAX_RULES + 1);
    assert_eq!(first.rules, second.rules);
    assert_eq!(first.residue, second.residue);
}
