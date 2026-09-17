// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn dag_leaf_free_and_parent_identity_survive_a_presentation_alias() {
    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: crate::display::RDF_LANG_STRING.to_owned(),
        language: None,
        direction: None,
    };
    assert_eq!(
        crate::display::term_display(&plain),
        crate::display::term_display(&langless)
    );
    let mut dag = TermDag::new();
    let a = dag.intern_leaf(plain.clone());
    let b = dag.intern_leaf(langless.clone());
    let fa = dag.intern_free(plain.clone());
    let fb = dag.intern_free(langless);
    assert_ne!(a, b);
    assert_ne!(fa, fb);
    assert_ne!(a, fa);
    assert_ne!(dag.key(a), dag.key(b));
    assert_eq!(a, dag.intern_leaf(plain));
    let pa = dag.intern_app(a, vec![a]);
    let pb = dag.intern_app(a, vec![b]);
    assert_ne!(pa, pb);
}
