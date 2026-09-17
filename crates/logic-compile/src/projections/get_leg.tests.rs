// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::projections::sparql::native;
use purrdf::sparql::{BaseDirection, Expression, Function, Literal, NamedNode};

fn native_literal() -> Expression {
    Expression::Literal(Literal::new_lang("hello", "en", Some(BaseDirection::Rtl)))
}

fn quoted(subject: &str, predicate: &str, object: Expression) -> Expression {
    Expression::FunctionCall(
        Function::Triple,
        vec![
            Expression::NamedNode(NamedNode::new(subject).unwrap()),
            Expression::NamedNode(NamedNode::new(predicate).unwrap()),
            object,
        ],
    )
}

#[test]
fn mapping_constant_read_retains_native_proposition_and_direction() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let owner = builder.intern_iri("urn:mapping");
    let field = builder.intern_iri(GM_BIND_EXPR);
    let subject = builder.intern_iri("urn:subject");
    let predicate = builder.intern_iri("urn:predicate");
    let object = builder.intern_literal(purrdf::RdfLiteral {
        lexical_form: "hello".into(),
        datatype: None,
        language: Some("en".into()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    });
    let triple_term = builder.intern_triple(subject, predicate, object);
    builder.push_annotation(owner, field, triple_term);
    let dataset = builder.freeze().unwrap();
    let view = DslView::new(&dataset);
    let value = view.first_object("urn:mapping", GM_BIND_EXPR).unwrap();
    let DslTerm::Triple { o, .. } = &value else {
        panic!("native proposition must not become an empty blank")
    };
    assert!(matches!(
        o.as_ref(),
        DslTerm::Literal {
            direction: Some(purrdf::RdfTextDirection::Rtl),
            ..
        }
    ));
    assert_eq!(
        view.objects_of("urn:mapping", GM_BIND_EXPR),
        vec![value.clone()]
    );
    let expression = parse_expr(&view, &value).unwrap();
    assert_eq!(
        native::expression(&expression).unwrap(),
        quoted("urn:subject", "urn:predicate", native_literal())
    );
}

#[test]
fn native_expression_constants_preserve_direction_and_nested_propositions() {
    let dataset = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let literal = DslTerm::Literal {
        lexical_form: "hello".into(),
        datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into(),
        language: Some("en".into()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let inner = DslTerm::Triple {
        s: Box::new(DslTerm::iri("urn:subject")),
        p: Box::new(DslTerm::iri("urn:predicate")),
        o: Box::new(literal.clone()),
    };
    let outer = DslTerm::Triple {
        s: Box::new(DslTerm::iri("urn:observer")),
        p: Box::new(DslTerm::iri("urn:reports")),
        o: Box::new(inner),
    };
    let expression = parse_expr(&DslView::new(&dataset), &outer).unwrap();
    assert!(matches!(&expression, Expr::ConstTerm(term) if term == &outer));
    assert_eq!(
        native::expression(&expression).unwrap(),
        quoted(
            "urn:observer",
            "urn:reports",
            quoted("urn:subject", "urn:predicate", native_literal())
        )
    );
    let literal_expression = parse_expr(&DslView::new(&dataset), &literal).unwrap();
    assert_eq!(
        native::expression(&literal_expression).unwrap(),
        native_literal()
    );
}

#[test]
fn source_scoped_proposition_constant_requires_explicit_binding() {
    let dataset = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let quoted = DslTerm::Triple {
        s: Box::new(DslTerm::Blank {
            label: "witness".into(),
            scope: purrdf::BlankScope(17),
        }),
        p: Box::new(DslTerm::iri("urn:predicate")),
        o: Box::new(DslTerm::iri("urn:value")),
    };
    let expression = parse_expr(&DslView::new(&dataset), &quoted).unwrap();
    assert!(matches!(&expression, Expr::ConstTerm(term) if term == &quoted));
    assert!(
        native::expression(&expression)
            .unwrap_err()
            .message()
            .contains("explicit query binding")
    );
}
