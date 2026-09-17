// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::query_ir::{QBuiltin, QTerm};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};
use purrdf::{BlankScope, RdfTextDirection, TermValue};

#[test]
fn retained_native_rule_keeps_polarity_builtin_tags_and_complete_value_identity() {
    let literal = TermValue::Literal {
        lexical_form: "rendered \"text\"".to_owned(),
        datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned(),
        language: Some("ar".to_owned()),
        direction: Some(RdfTextDirection::Rtl),
    };
    let rule = EvalRule {
        numeric: Vec::new(),
        head: EvalAtom::positive(
            EvalTerm::Var("?record".to_owned()),
            "urn:violated-law",
            EvalTerm::ConstNamed("urn:law".to_owned()),
        ),
        body: vec![EvalAtom {
            subject: EvalTerm::Var("?record".to_owned()),
            predicate: "urn:evidence".to_owned(),
            negated: true,
            object: EvalTerm::ConstLit(TermValue::Triple {
                s: Box::new(TermValue::Blank {
                    label: "same-label".to_owned(),
                    scope: BlankScope(17),
                }),
                p: Box::new(TermValue::iri("urn:states")),
                o: Box::new(TermValue::Triple {
                    s: Box::new(TermValue::Blank {
                        label: "same-label".to_owned(),
                        scope: BlankScope(19),
                    }),
                    p: Box::new(TermValue::iri("urn:quote")),
                    o: Box::new(literal),
                }),
            }),
        }],
        rule_iri: "urn:law-rule".to_owned(),
        distinct_pairs: vec![("?x".to_owned(), "?y".to_owned())],
        builtins: vec![QBuiltin::DimEqual {
            d1: QTerm::Var("?x".to_owned()),
            d2: QTerm::Const("<urn:dimension>".to_owned()),
        }],
        reduction: None,
        constraint_tag: Some("urn:law".to_owned()),
    };
    let bytes = serde_json::to_vec(&rule).unwrap();
    let restored: EvalRule = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        restored, rule,
        "the executable native law must survive its fixture transport exactly"
    );
}
