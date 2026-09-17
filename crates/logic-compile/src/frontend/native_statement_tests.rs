// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW scope and formula contracts at the native statement-table boundary.

use super::*;
use purrdf::{RdfDatasetBuilder, RdfLiteral};

fn quoted_axiom(builder: &mut RdfDatasetBuilder) -> (purrdf::TermId, purrdf::TermId) {
    let s = builder.intern_iri("https://example.org/Bird");
    let p = builder.intern_iri(&logic_iri("subClassOf"));
    let o = builder.intern_iri("https://example.org/Animal");
    let triple = builder.intern_triple(s, p, o);
    let reifier = builder.intern_iri("https://example.org/claim");
    (reifier, triple)
}

#[test]
fn native_scoped_axiom_retains_every_scope_dimension_without_global_assertion() {
    for duplicate in [false, true] {
        let mut builder = RdfDatasetBuilder::new();
        let (reifier, triple) = quoted_axiom(&mut builder);
        builder.push_reifier(reifier, triple);
        if duplicate {
            let predicate = builder.intern_iri(RDF_REIFIES);
            builder.push_quad(reifier, predicate, triple, None);
            builder.push_annotation(reifier, predicate, triple);
        }
        for (field, iri) in [
            ("standpoint", "https://example.org/observer"),
            ("inModule", "https://example.org/theory"),
            ("provenance", "https://example.org/source"),
            ("modality", "https://blackcatinformatics.ca/logic/epistemic"),
        ] {
            let p = builder.intern_iri(&logic_iri(field));
            let o = builder.intern_iri(iri);
            builder.push_annotation(reifier, p, o);
            if duplicate {
                builder.push_quad(reifier, p, o, None);
            }
        }
        for (field, lexical) in [("confidence", "0.75"), ("time", "2026-09-08")] {
            let p = builder.intern_iri(&logic_iri(field));
            let o = builder.intern_literal(RdfLiteral::simple(lexical.to_owned()));
            builder.push_annotation(reifier, p, o);
        }
        let source = builder.freeze().unwrap();
        let (program, diagnostics) = PreparedLogicSource::new(&source)
            .unwrap()
            .compile(None)
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let claims: Vec<_> = program
            .axioms
            .iter()
            .filter(|axiom| axiom.subject == "https://example.org/Bird")
            .collect();
        assert_eq!(
            claims.len(),
            1,
            "physical carriers must not multiply a claim"
        );
        assert_eq!(claims[0].obj.as_iri(), Some("https://example.org/Animal"));
        assert_eq!(
            claims[0].scope,
            ContextualScope::new(
                Some("https://example.org/observer".into()),
                Some("2026-09-08".into()),
                Some(0.75),
                LogicModality::Epistemic,
                Some("https://example.org/source".into()),
                Some("https://example.org/theory".into())
            )
            .unwrap()
        );
    }
}

#[test]
fn bare_reification_is_a_proposition_and_never_an_assertion() {
    for native in [false, true] {
        let mut builder = RdfDatasetBuilder::new();
        let (reifier, triple) = quoted_axiom(&mut builder);
        if native {
            builder.push_reifier(reifier, triple);
        } else {
            let predicate = builder.intern_iri(RDF_REIFIES);
            builder.push_quad(reifier, predicate, triple, None);
        }
        let source = builder.freeze().unwrap();
        let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            program.axioms.is_empty(),
            "quotation must not assert: {:?}",
            program.axioms
        );
    }
}

#[test]
fn named_graph_annotations_do_not_supply_default_graph_scope() {
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    builder.push_reifier(reifier, triple);
    let graph = builder.intern_iri("https://example.org/graph");
    let scope = builder.intern_iri(&logic_iri("standpoint"));
    let observer = builder.intern_iri("https://example.org/observer");
    builder.push_annotation_in_graph(reifier, scope, observer, Some(graph));
    builder.push_reifier_in_graph(reifier, triple, Some(graph));
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(program.axioms.is_empty(), "named graph scope must not leak");
}

#[test]
fn nested_scoped_proposition_has_an_explicit_lowering_error() {
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    let predicate = builder.intern_iri(&logic_iri("believes"));
    let nested = builder.intern_triple(reifier, predicate, triple);
    builder.push_reifier(reifier, nested);
    let scope = builder.intern_iri(&logic_iri("standpoint"));
    let observer = builder.intern_iri("https://example.org/observer");
    builder.push_annotation(reifier, scope, observer);
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "UNSUPPORTED_SCOPED_TERM");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(
        !program
            .axioms
            .iter()
            .any(|axiom| axiom.predicate == logic_iri("believes"))
    );
}

#[test]
fn annotation_only_modal_formula_preserves_authored_variable_and_constraint_ownership() {
    for duplicate in [false, true] {
        let mut builder = RdfDatasetBuilder::new();
        for (s, p, o, literal) in [
            ("law", "instanceOf", "Formula", false),
            ("law", "forall", "implication", false),
            ("implication", "instanceOf", "Formula", false),
            ("implication", "antecedent", "guard", false),
            ("implication", "consequent", "box", false),
            ("guard", "instanceOf", "Formula", false),
            ("guard", "relation", "member", false),
            ("guard", "argument", "x", false),
            ("guard", "argument", "memberOf", false),
            ("memberOf", "termIri", "Class", false),
            ("memberOf", "termIndex", "1", true),
            ("law", "quantifiedVariable", "x", false),
            ("box", "instanceOf", "Formula", false),
            ("box", "necessarily", "atom", false),
            ("box", "overAccessibility", "deonticallyIdeal", false),
            ("atom", "instanceOf", "Formula", false),
            ("atom", "relation", "relationP", false),
            ("atom", "argument", "x", false),
            ("x", "termVariable", "__w0", true),
            ("x", "termIndex", "0", true),
            ("constraint", "instanceOf", "Constraint", false),
            ("constraint", "integrity", "law", false),
        ] {
            let s = builder.intern_iri(&logic_iri(s));
            let p = builder.intern_iri(&logic_iri(p));
            let o = if literal {
                builder.intern_literal(RdfLiteral::simple(o.to_owned()))
            } else {
                builder.intern_iri(&logic_iri(o))
            };
            builder.push_annotation(s, p, o);
            if duplicate {
                builder.push_quad(s, p, o, None);
            }
        }
        let source = builder.freeze().unwrap();
        let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            program.formulas.is_empty(),
            "constraint is never asserted globally"
        );
        assert_eq!(program.constraints.len(), 1);
        let mut reader = FormulaReader::new(source.as_ref());
        let Formula::Forall { vars, body } = reader.read(&Subject::Iri(logic_iri("law"))).unwrap()
        else {
            panic!("authored quantifier")
        };
        assert_eq!(vars, vec!["__w0"]);
        let Formula::Implies(_, body) = *body else {
            panic!("constraint guard")
        };
        let Formula::Forall { vars: worlds, body } = *body else {
            panic!("modal quantifier")
        };
        assert_ne!(worlds, vars);
        let Formula::Implies(_, atom) = *body else {
            panic!("accessibility guard")
        };
        let Formula::Atom { args, .. } = *atom else {
            panic!("world-indexed predication")
        };
        assert_eq!(
            args,
            vec![Term::Var(worlds[0].clone()), Term::Var("__w0".into())]
        );
    }
}

#[test]
fn invalid_native_scope_never_emits_a_weaker_claim() {
    for kind in ["confidence", "modality", "multiplicity", "proposition"] {
        let mut builder = RdfDatasetBuilder::new();
        let (reifier, triple) = quoted_axiom(&mut builder);
        builder.push_reifier(reifier, triple);
        let standpoint = builder.intern_iri(&logic_iri("standpoint"));
        let observer = builder.intern_iri("https://example.org/observer");
        builder.push_annotation(reifier, standpoint, observer);
        let (field, object, expected) = match kind {
            "confidence" => (
                "confidence",
                builder.intern_literal(RdfLiteral::simple("2.5")),
                "INVALID_CONFIDENCE",
            ),
            "modality" => (
                "modality",
                builder.intern_iri(&logic_iri("UnknownModality")),
                "UNKNOWN_MODALITY",
            ),
            "multiplicity" => (
                "standpoint",
                builder.intern_iri("https://example.org/other"),
                "UNSUPPORTED_SCOPE_MULTIPLICITY",
            ),
            "proposition" => ("time", triple, "UNSUPPORTED_SCOPE_TERM"),
            _ => unreachable!(),
        };
        let predicate = builder.intern_iri(&logic_iri(field));
        // Deliberately split a multivalued coordinate across the physical tables.
        builder.push_quad(reifier, predicate, object, None);
        let source = builder.freeze().unwrap();
        let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == expected && d.severity == Severity::Error),
            "{kind}: {diagnostics:?}"
        );
        assert!(
            !program
                .axioms
                .iter()
                .any(|a| a.subject == "https://example.org/Bird"),
            "{kind}: {:?}",
            program.axioms
        );
    }
}

#[test]
fn separate_native_reifiers_preserve_coexisting_standpoints() {
    let mut builder = RdfDatasetBuilder::new();
    let (first, triple) = quoted_axiom(&mut builder);
    let second = builder.intern_iri("https://example.org/otherClaim");
    let predicate = builder.intern_iri(&logic_iri("standpoint"));
    for (reifier, observer) in [
        (first, "https://example.org/observer"),
        (second, "https://example.org/other"),
    ] {
        builder.push_reifier(reifier, triple);
        let observer = builder.intern_iri(observer);
        builder.push_annotation(reifier, predicate, observer);
    }
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let observers: BTreeSet<_> = program
        .axioms
        .iter()
        .filter(|a| a.subject == "https://example.org/Bird")
        .map(|a| a.scope.standpoint.as_deref())
        .collect();
    assert_eq!(
        observers,
        BTreeSet::from([
            Some("https://example.org/observer"),
            Some("https://example.org/other")
        ])
    );
}
