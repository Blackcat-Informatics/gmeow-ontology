// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW scope and formula contracts at the native statement-table boundary.

use super::*;
use purrdf::{RdfDatasetBuilder, RdfLiteral};

fn quoted_axiom(builder: &mut RdfDatasetBuilder) -> (purrdf::TermId, purrdf::TermId) {
    let s = builder.intern_iri("https://example.org/Bird").unwrap();
    let p = builder.intern_iri(&logic_iri("subClassOf")).unwrap();
    let o = builder.intern_iri("https://example.org/Animal").unwrap();
    let triple = builder.intern_triple(s, p, o).unwrap();
    let reifier = builder.intern_iri("https://example.org/claim").unwrap();
    (reifier, triple)
}

#[test]
fn native_rule_projection_round_trips_typed_atoms_and_guards() {
    use crate::ir::{AtomicTerm, LogicRule};
    for object in [
        AtomicTerm::Var("?y".into()),
        AtomicTerm::Iri("urn:constant".into()),
        AtomicTerm::Literal(RdfLiteral::simple("?this-is-literal")),
        AtomicTerm::Literal(RdfLiteral::typed(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        )),
        AtomicTerm::Literal(RdfLiteral {
            lexical_form: "مرحبا".into(),
            datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into()),
            language: Some("ar".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        }),
    ] {
        let atom = |predicate: &str, object, negated| {
            let scope = ContextualScope::new(
                Some("urn:observer".into()),
                Some("2026-10-15".into()),
                Some(0.75),
                LogicModality::Epistemic,
                Some("urn:evidence".into()),
                Some("urn:module".into()),
            )
            .unwrap();
            LogicAxiom::new("?x", predicate, object, negated, scope).unwrap()
        };
        let rule = LogicRule::new(
            atom("urn:derived", object, false),
            vec![
                atom("urn:given", AtomicTerm::Var("?y".into()), false),
                atom("urn:blocked", AtomicTerm::Var("?y".into()), true),
            ],
            vec![("?x".into(), "?y".into())],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let projection =
            crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
        assert_eq!(projection.dataset.owned_reifiers().count(), 3);
        for quad in projection.dataset.owned_quads() {
            assert_ne!(
                quad.predicate, "urn:derived",
                "quotation never asserts the rule head"
            );
            assert_ne!(quad.object, purrdf::RdfTerm::iri(RDF_STATEMENT));
            assert!(![RDF_SUBJECT, RDF_PREDICATE, RDF_OBJECT].contains(&quad.predicate.as_str()));
        }
        let (restored, diagnostics) = parse_logic_dataset(&projection.dataset, None).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            serde_json::to_vec(&restored.rules).unwrap(),
            serde_json::to_vec(&program.rules).unwrap()
        );
        assert!(
            restored.axioms.is_empty(),
            "term carriers must remain structural: {:?}",
            restored.axioms
        );
        let wire = projection.into_text().unwrap();
        let (from_wire, diagnostics) = parse_logic_str(&wire.content, None).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(from_wire.rules, program.rules);
        assert!(from_wire.axioms.is_empty());
    }
}

#[test]
fn native_scoped_axiom_retains_every_scope_dimension_without_global_assertion() {
    for duplicate in [false, true] {
        let mut builder = RdfDatasetBuilder::new();
        let (reifier, triple) = quoted_axiom(&mut builder);
        builder.push_reifier(reifier, triple).unwrap();
        if duplicate {
            let predicate = builder.intern_iri(RDF_REIFIES).unwrap();
            builder.push_quad(reifier, predicate, triple, None).unwrap();
            builder.push_annotation(reifier, predicate, triple).unwrap();
        }
        for (field, iri) in [
            ("standpoint", "https://example.org/observer"),
            ("inModule", "https://example.org/theory"),
            ("provenance", "https://example.org/source"),
            ("modality", "https://blackcatinformatics.ca/logic/epistemic"),
        ] {
            let p = builder.intern_iri(&logic_iri(field)).unwrap();
            let o = builder.intern_iri(iri).unwrap();
            builder.push_annotation(reifier, p, o).unwrap();
            if duplicate {
                builder.push_quad(reifier, p, o, None).unwrap();
            }
        }
        for (field, lexical) in [("confidence", "0.75"), ("time", "2026-09-08")] {
            let p = builder.intern_iri(&logic_iri(field)).unwrap();
            let o = builder
                .intern_literal(RdfLiteral::simple(lexical.to_owned()))
                .unwrap();
            builder.push_annotation(reifier, p, o).unwrap();
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
            builder.push_reifier(reifier, triple).unwrap();
        } else {
            let predicate = builder.intern_iri(RDF_REIFIES).unwrap();
            builder.push_quad(reifier, predicate, triple, None).unwrap();
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
    builder.push_reifier(reifier, triple).unwrap();
    let graph = builder.intern_iri("https://example.org/graph").unwrap();
    let scope = builder.intern_iri(&logic_iri("standpoint")).unwrap();
    let observer = builder.intern_iri("https://example.org/observer").unwrap();
    builder
        .push_annotation_in_graph(reifier, scope, observer, Some(graph))
        .unwrap();
    builder
        .push_reifier_in_graph(reifier, triple, Some(graph))
        .unwrap();
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(program.axioms.is_empty(), "named graph scope must not leak");
}

#[test]
fn nested_scoped_proposition_has_an_explicit_lowering_error() {
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    let predicate = builder.intern_iri(&logic_iri("believes")).unwrap();
    let nested = builder.intern_triple(reifier, predicate, triple).unwrap();
    builder.push_reifier(reifier, nested).unwrap();
    let scope = builder.intern_iri(&logic_iri("standpoint")).unwrap();
    let observer = builder.intern_iri("https://example.org/observer").unwrap();
    builder.push_annotation(reifier, scope, observer).unwrap();
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
            let s = builder.intern_iri(&logic_iri(s)).unwrap();
            let p = builder.intern_iri(&logic_iri(p)).unwrap();
            let o = if literal {
                builder
                    .intern_literal(RdfLiteral::simple(o.to_owned()))
                    .unwrap()
            } else {
                builder.intern_iri(&logic_iri(o)).unwrap()
            };
            builder.push_annotation(s, p, o).unwrap();
            if duplicate {
                builder.push_quad(s, p, o, None).unwrap();
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
            vec![
                Term::Var(worlds[0].clone()),
                Term::Iri(logic_iri("relationP")),
                Term::Var("__w0".into())
            ]
        );
    }
}

#[test]
fn invalid_native_scope_never_emits_a_weaker_claim() {
    for kind in ["confidence", "modality", "proposition"] {
        let mut builder = RdfDatasetBuilder::new();
        let (reifier, triple) = quoted_axiom(&mut builder);
        builder.push_reifier(reifier, triple).unwrap();
        let standpoint = builder.intern_iri(&logic_iri("standpoint")).unwrap();
        let observer = builder.intern_iri("https://example.org/observer").unwrap();
        builder
            .push_annotation(reifier, standpoint, observer)
            .unwrap();
        let (field, object, expected) = match kind {
            "confidence" => (
                "confidence",
                builder.intern_literal(RdfLiteral::simple("2.5")).unwrap(),
                "INVALID_CONFIDENCE",
            ),
            "modality" => (
                "modality",
                builder.intern_iri(&logic_iri("UnknownModality")).unwrap(),
                "UNKNOWN_MODALITY",
            ),
            "proposition" => ("time", triple, "UNSUPPORTED_SCOPE_TERM"),
            _ => unreachable!(),
        };
        let predicate = builder.intern_iri(&logic_iri(field)).unwrap();
        // Deliberately split a multivalued coordinate across the physical tables.
        builder.push_quad(reifier, predicate, object, None).unwrap();
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
    let second = builder
        .intern_iri("https://example.org/otherClaim")
        .unwrap();
    let predicate = builder.intern_iri(&logic_iri("standpoint")).unwrap();
    for (reifier, observer) in [
        (first, "https://example.org/observer"),
        (second, "https://example.org/other"),
    ] {
        builder.push_reifier(reifier, triple).unwrap();
        let observer = builder.intern_iri(observer).unwrap();
        builder
            .push_annotation(reifier, predicate, observer)
            .unwrap();
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

/// A scope value as authored on a claim: an IRI or a plain literal.
#[derive(Clone, Copy)]
enum Authored {
    Iri(&'static str),
    Text(&'static str),
}

/// Compile one quoted claim carrying the given `logic:` scope values, in the order
/// given, and return the scopes of the resulting `ex:Bird` axioms with the
/// diagnostic codes.
fn lower_claim(values: &[(&str, Authored)]) -> (Vec<ContextualScope>, Vec<String>) {
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    builder.push_reifier(reifier, triple).unwrap();
    for (field, value) in values {
        let predicate = builder.intern_iri(&logic_iri(field)).unwrap();
        let object = match value {
            Authored::Iri(iri) => builder.intern_iri(iri).unwrap(),
            Authored::Text(text) => builder.intern_literal(RdfLiteral::simple(*text)).unwrap(),
        };
        builder.push_annotation(reifier, predicate, object).unwrap();
    }
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    let scopes = program
        .axioms
        .iter()
        .filter(|a| a.subject == "https://example.org/Bird")
        .map(|a| a.scope.clone())
        .collect();
    (scopes, diagnostics.into_iter().map(|d| d.code).collect())
}

#[test]
fn several_values_of_one_scope_coordinate_lower_to_one_claim_per_value() {
    use Authored::{Iri, Text};
    let cases: [(&str, Authored, Authored); 6] = [
        (
            "standpoint",
            Iri("https://example.org/a"),
            Iri("https://example.org/b"),
        ),
        ("time", Text("2026-01-01"), Text("2026-02-01")),
        ("confidence", Text("0.25"), Text("0.75")),
        (
            "modality",
            Iri("https://blackcatinformatics.ca/logic/alethic"),
            Iri("https://blackcatinformatics.ca/logic/epistemic"),
        ),
        (
            "provenance",
            Iri("https://example.org/a"),
            Iri("https://example.org/b"),
        ),
        (
            "inModule",
            Iri("https://example.org/a"),
            Iri("https://example.org/b"),
        ),
    ];
    for (field, first, second) in cases {
        let (forward, codes) = lower_claim(&[(field, first), (field, second)]);
        let (backward, _) = lower_claim(&[(field, second), (field, first)]);
        assert!(codes.is_empty(), "{field}: {codes:?}");
        assert_eq!(forward.len(), 2, "{field}");
        assert_ne!(
            forward[0], forward[1],
            "{field}: the two claims are distinct"
        );
        assert_eq!(
            forward, backward,
            "{field}: authoring order changed the result"
        );
    }
}

#[test]
fn fanning_out_one_coordinate_shares_every_other_coordinate() {
    use Authored::{Iri, Text};
    let (scopes, codes) = lower_claim(&[
        ("standpoint", Iri("https://example.org/shared")),
        ("confidence", Text("0.5")),
        ("provenance", Iri("https://example.org/a")),
        ("provenance", Iri("https://example.org/b")),
    ]);
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(scopes.len(), 2);
    for scope in &scopes {
        assert_eq!(
            scope.standpoint.as_deref(),
            Some("https://example.org/shared")
        );
        assert_eq!(scope.confidence, Some(0.5));
    }
}

#[test]
fn two_provenance_values_yield_two_claims_in_canonical_order() {
    use Authored::Iri;
    let (scopes, codes) = lower_claim(&[
        ("provenance", Iri("https://example.org/zulu")),
        ("provenance", Iri("https://example.org/alfa")),
    ]);
    assert!(codes.is_empty(), "{codes:?}");
    let provenance: Vec<_> = scopes.iter().map(|s| s.provenance.as_deref()).collect();
    assert_eq!(
        provenance,
        [
            Some("https://example.org/alfa"),
            Some("https://example.org/zulu")
        ]
    );
}

#[test]
fn several_values_of_two_scope_coordinates_withhold_the_claim_naming_both() {
    use Authored::Iri;
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    builder.push_reifier(reifier, triple).unwrap();
    for (field, value) in [
        ("provenance", "https://example.org/a"),
        ("provenance", "https://example.org/b"),
        ("standpoint", "https://example.org/c"),
        ("standpoint", "https://example.org/d"),
    ] {
        let predicate = builder.intern_iri(&logic_iri(field)).unwrap();
        let object = builder.intern_iri(value).unwrap();
        builder.push_annotation(reifier, predicate, object).unwrap();
    }
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    let refusal = diagnostics
        .iter()
        .find(|d| d.code == "AMBIGUOUS_SCOPE_COORDINATES")
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    assert_eq!(refusal.severity, Severity::Error);
    assert_eq!(
        refusal.subject.as_deref(),
        Some("https://example.org/claim")
    );
    assert!(refusal.message.contains("logic:provenance"));
    assert!(refusal.message.contains("logic:standpoint"));
    assert!(
        !program
            .axioms
            .iter()
            .any(|a| a.subject == "https://example.org/Bird")
    );
    // A neighbouring node with one multi-valued coordinate is admitted.
    let (admitted, codes) = lower_claim(&[
        ("provenance", Iri("https://example.org/a")),
        ("provenance", Iri("https://example.org/b")),
    ]);
    assert_eq!((admitted.len(), codes.is_empty()), (2, true));
}

#[test]
fn one_invalid_value_among_several_withholds_every_claim() {
    use Authored::{Iri, Text};
    for (values, expected) in [
        (
            vec![("confidence", Text("0.5")), ("confidence", Text("2.5"))],
            "INVALID_CONFIDENCE",
        ),
        (
            vec![
                (
                    "modality",
                    Iri("https://blackcatinformatics.ca/logic/alethic"),
                ),
                (
                    "modality",
                    Iri("https://blackcatinformatics.ca/logic/Unknown"),
                ),
            ],
            "UNKNOWN_MODALITY",
        ),
    ] {
        let (scopes, codes) = lower_claim(&values);
        assert!(scopes.is_empty(), "{expected}: {scopes:?}");
        assert!(codes.iter().any(|c| c == expected), "{expected}: {codes:?}");
    }
    // A proposition-valued value among IRIs.
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    builder.push_reifier(reifier, triple).unwrap();
    let predicate = builder.intern_iri(&logic_iri("provenance")).unwrap();
    let source_iri = builder.intern_iri("https://example.org/a").unwrap();
    builder
        .push_annotation(reifier, predicate, source_iri)
        .unwrap();
    builder.push_annotation(reifier, predicate, triple).unwrap();
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "UNSUPPORTED_SCOPE_TERM")
    );
    assert!(
        program
            .axioms
            .iter()
            .all(|a| a.subject != "https://example.org/Bird")
    );
}

fn parse_turtle(text: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(
        format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
             @prefix ex: <https://example.org/> .
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . {text}"
        )
        .as_bytes(),
        "application/trig",
        None,
    )
    .unwrap()
}

#[test]
fn a_rule_atom_with_several_values_of_one_coordinate_withholds_the_whole_rule() {
    let dataset = parse_turtle(
        "ex:rule a logic:Rule ;
             logic:head [ rdf:reifies <<( ex:Bird ex:p ex:v )>> ] ;
             logic:body [ rdf:reifies <<( ex:Bird ex:q ex:v )>> ;
                          logic:provenance ex:a, ex:b ] .",
    );
    let (program, diagnostics) = parse_logic_dataset(&dataset, None).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "AMBIGUOUS_RULE_ATOM_SCOPE" && d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    assert!(program.rules.is_empty(), "{:?}", program.rules);
}

fn admission_of(text: &str) -> SourceAdmission {
    let theory = PreparedLogicSource::new(&parse_turtle(text))
        .unwrap()
        .into_compiled(None)
        .unwrap();
    SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory))
}

#[test]
fn rules_lowered_from_one_rule_node_are_one_admission_fact_with_unchanged_id() {
    // A context coordinate blocks every lowered rule alike, so they are one fact
    // about the authored node. (Evidence coordinates admit them as separate units.)
    let rule = |standpoint: &str| {
        format!(
            "ex:rule a logic:Rule ;
                 logic:standpoint {standpoint} ;
                 logic:head [ rdf:reifies <<( ex:Bird ex:p ex:v )>> ] ."
        )
    };
    let fanned = admission_of(&rule("ex:a, ex:b"));
    let single = admission_of(&rule("ex:a"));
    let rule_units = |admission: &SourceAdmission| {
        admission
            .units
            .iter()
            .filter(|unit| unit.kind == SemanticUnitKind::Rule)
            .cloned()
            .collect::<Vec<_>>()
    };
    let (fanned_rules, single_rules) = (rule_units(&fanned), rule_units(&single));
    assert_eq!(fanned_rules.len(), 1, "{fanned_rules:?}");
    assert!(matches!(
        &fanned_rules[0].disposition,
        SemanticUnitDisposition::Blocked { code, .. } if code == "CONTEXT_ERASURE_FORBIDDEN"
    ));
    // Same kind, anchor, documents and disposition: the authored rule keeps the
    // id it has with a single provenance value.
    assert_eq!(fanned_rules, single_rules);
    let ids: BTreeSet<_> = fanned.units.iter().map(|unit| &unit.id).collect();
    assert_eq!(ids.len(), fanned.units.len(), "{:?}", fanned.units);
}

#[test]
fn axioms_lowered_from_one_reifier_get_distinct_assertion_units() {
    let admission = admission_of(
        "ex:claim rdf:reifies <<( ex:Bird ex:p ex:v )>> ;
             logic:provenance ex:a, ex:b .",
    );
    let assertions: Vec<_> = admission
        .units
        .iter()
        .filter(|unit| unit.kind == SemanticUnitKind::Assertion)
        .collect();
    assert_eq!(assertions.len(), 2, "{assertions:?}");
    assert_ne!(assertions[0].id, assertions[1].id);
    assert_ne!(assertions[0].anchor, assertions[1].anchor);
}

#[test]
fn spellings_of_one_confidence_or_modality_count_as_one_value() {
    use Authored::{Iri, Text};
    let provenance = [
        ("provenance", Iri("https://example.org/a")),
        ("provenance", Iri("https://example.org/b")),
    ];
    for (field, first, second) in [
        ("confidence", Text("0.5"), Text("0.50")),
        ("confidence", Text("0.5"), Text("5e-1")),
        (
            "modality",
            Iri("https://blackcatinformatics.ca/logic/alethic"),
            Iri("https://blackcatinformatics.ca/logic/Alethic"),
        ),
    ] {
        let (one, codes) = lower_claim(&[(field, first), (field, second)]);
        assert!(codes.is_empty(), "{field}: {codes:?}");
        assert_eq!(one.len(), 1, "{field}: one value, one claim");
        let mut with_sources = vec![(field, first), (field, second)];
        with_sources.extend(provenance);
        let (two, codes) = lower_claim(&with_sources);
        assert!(codes.is_empty(), "{field}: {codes:?}");
        assert_eq!(two.len(), 2, "{field}: two sources, two claims");
    }
}

#[test]
fn distinct_confidences_fan_out_in_numeric_order_whatever_the_authored_order() {
    use Authored::Text;
    let spellings = [
        ("confidence", Text("0.5")),
        ("confidence", Text("0.50")),
        ("confidence", Text("0.25")),
    ];
    let mut reversed = spellings;
    reversed.reverse();
    for authored in [spellings, reversed] {
        let (scopes, codes) = lower_claim(&authored);
        assert!(codes.is_empty(), "{codes:?}");
        let confidences: Vec<_> = scopes.iter().map(|s| s.confidence).collect();
        assert_eq!(confidences, [Some(0.25), Some(0.5)]);
    }
}

fn rule_text(coordinates: &str) -> String {
    format!(
        "ex:rule a logic:Rule ;
             {coordinates}
             logic:head [ rdf:reifies <<( ex:Bird ex:p ex:v )>> ] ."
    )
}

fn rule_scopes(coordinates: &str) -> (Vec<ContextualScope>, Vec<String>) {
    let (program, diagnostics) =
        parse_logic_dataset(&parse_turtle(&rule_text(coordinates)), None).unwrap();
    (
        program
            .rules
            .iter()
            .map(|rule| rule.scope.clone())
            .collect(),
        diagnostics.into_iter().map(|d| d.code).collect(),
    )
}

#[test]
fn a_rule_node_with_two_provenance_values_lowers_to_two_rules_one_source() {
    let dataset = parse_turtle(&rule_text("logic:provenance ex:zulu, ex:alfa ;"));
    let theory = PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    assert!(
        theory.diagnostics().is_empty(),
        "{:?}",
        theory.diagnostics()
    );
    let authored = SourceNode {
        term: theory
            .source()
            .dataset()
            .term_id_by_iri("https://example.org/rule")
            .unwrap(),
        graph: None,
    };
    let lowerings: Vec<_> = theory
        .owner_lowerings()
        .iter()
        .filter(|owner| owner.family == OwnerFamily::Rule)
        .collect();
    assert_eq!(lowerings.len(), 2, "{lowerings:?}");
    assert!(lowerings.iter().all(|owner| owner.source == authored));
    let provenance: Vec<_> = lowerings
        .iter()
        .map(|owner| {
            let OwnerDisposition::Emitted { index } = owner.disposition else {
                panic!("rule not emitted: {owner:?}")
            };
            theory.program().rules[index]
                .scope
                .provenance
                .clone()
                .unwrap()
        })
        .collect();
    let mut indices: Vec<_> = lowerings
        .iter()
        .map(|owner| owner.disposition.clone())
        .collect();
    indices.dedup();
    assert_eq!(
        indices.len(),
        2,
        "two lowerings carry two different indices"
    );
    let mut sorted = provenance.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        ["https://example.org/alfa", "https://example.org/zulu"]
    );
}

#[test]
fn lowered_claims_keep_the_authored_reifier_as_their_only_source() {
    let dataset = parse_turtle(
        "ex:claim rdf:reifies <<( ex:Bird ex:p ex:v )>> ; logic:provenance ex:a, ex:b .",
    );
    let theory = PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    assert!(
        theory.diagnostics().is_empty(),
        "{:?}",
        theory.diagnostics()
    );
    let authored = SourceNode {
        term: theory
            .source()
            .dataset()
            .term_id_by_iri("https://example.org/claim")
            .unwrap(),
        graph: None,
    };
    let claims: Vec<_> = theory
        .program()
        .axioms
        .iter()
        .enumerate()
        .filter(|(_, axiom)| axiom.subject == "https://example.org/Bird")
        .collect();
    assert_eq!(claims.len(), 2);
    for (index, _) in claims {
        assert_eq!(
            theory.axiom_sources()[index],
            [AxiomSource::Reification(authored)]
        );
    }
}

#[test]
fn the_classic_statement_carrier_fans_out_like_a_reifier() {
    let dataset = parse_turtle(
        "ex:statement a rdf:Statement ;
             rdf:subject ex:Bird ; rdf:predicate ex:p ; rdf:object ex:v ;
             logic:provenance ex:a, ex:b .",
    );
    let (program, diagnostics) = parse_logic_dataset(&dataset, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let provenance: Vec<_> = program
        .axioms
        .iter()
        .filter(|a| a.subject == "https://example.org/Bird")
        .map(|a| a.scope.provenance.as_deref())
        .collect();
    assert_eq!(
        provenance,
        [Some("https://example.org/a"), Some("https://example.org/b")]
    );
}

#[test]
fn a_rule_node_with_two_multi_valued_coordinates_is_refused_whole() {
    let (rules, codes) = rule_scopes("logic:provenance ex:a, ex:b ; logic:standpoint ex:c, ex:d ;");
    assert!(rules.is_empty(), "{rules:?}");
    assert!(
        codes.iter().any(|c| c == "AMBIGUOUS_SCOPE_COORDINATES"),
        "{codes:?}"
    );
}

#[test]
fn one_invalid_value_among_several_withholds_the_whole_rule() {
    let (rules, codes) = rule_scopes("logic:confidence 0.5, 2.5 ;");
    assert!(rules.is_empty(), "{rules:?}");
    assert!(codes.iter().any(|c| c == "INVALID_CONFIDENCE"), "{codes:?}");
}

#[test]
fn a_rule_node_fans_out_on_a_coordinate_other_than_provenance() {
    let (rules, codes) = rule_scopes("logic:standpoint ex:b, ex:a ; logic:confidence 0.5 ;");
    assert!(codes.is_empty(), "{codes:?}");
    let standpoints: Vec<_> = rules.iter().map(|s| s.standpoint.as_deref()).collect();
    assert_eq!(
        standpoints,
        [Some("https://example.org/a"), Some("https://example.org/b")]
    );
    assert!(rules.iter().all(|s| s.confidence == Some(0.5)));
}

#[test]
fn rule_fan_out_is_the_same_in_both_authoring_orders() {
    let forward = rule_scopes("logic:provenance ex:a, ex:b ;");
    let backward = rule_scopes("logic:provenance ex:b, ex:a ;");
    assert_eq!(forward.0.len(), 2);
    assert_eq!(forward, backward);
}

#[test]
fn modality_none_beside_another_modality_yields_only_the_other_claim_but_both_rules() {
    // A claim whose only scope is the default stays an unasserted quotation, so
    // only the epistemic claim is emitted.
    let dataset = parse_turtle(
        "ex:claim rdf:reifies <<( ex:Bird ex:p ex:v )>> ;
             logic:modality logic:none, logic:epistemic .",
    );
    let (program, diagnostics) = parse_logic_dataset(&dataset, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let modalities: Vec<_> = program
        .axioms
        .iter()
        .filter(|a| a.subject == "https://example.org/Bird")
        .map(|a| a.scope.modality)
        .collect();
    assert_eq!(modalities, [LogicModality::Epistemic]);
    // A rule is asserted without any scope, so the `none` rule is the unscoped one.
    let (rules, codes) = rule_scopes("logic:modality logic:none, logic:epistemic ;");
    assert!(codes.is_empty(), "{codes:?}");
    let modalities: Vec<_> = rules.iter().map(|s| s.modality).collect();
    assert_eq!(modalities, [LogicModality::None, LogicModality::Epistemic]);
}

#[test]
fn values_split_across_physical_tables_fan_out_the_same_way() {
    let mut builder = RdfDatasetBuilder::new();
    let (reifier, triple) = quoted_axiom(&mut builder);
    builder.push_reifier(reifier, triple).unwrap();
    let predicate = builder.intern_iri(&logic_iri("provenance")).unwrap();
    let first = builder.intern_iri("https://example.org/a").unwrap();
    let second = builder.intern_iri("https://example.org/b").unwrap();
    builder.push_annotation(reifier, predicate, first).unwrap();
    builder.push_quad(reifier, predicate, second, None).unwrap();
    let source = builder.freeze().unwrap();
    let (program, diagnostics) = parse_logic_dataset(&source, None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let provenance: Vec<_> = program
        .axioms
        .iter()
        .filter(|a| a.subject == "https://example.org/Bird")
        .map(|a| a.scope.provenance.as_deref())
        .collect();
    assert_eq!(
        provenance,
        [Some("https://example.org/a"), Some("https://example.org/b")]
    );
}

#[test]
fn distinct_terms_with_the_same_text_are_distinct_values() {
    // An IRI and a literal spelling the same text are two provenance values, so
    // beside two standpoints the node is ambiguous whichever the store returns first.
    let (scopes, codes) = lower_claim(&[
        ("provenance", Authored::Iri("https://example.org/a")),
        ("provenance", Authored::Text("https://example.org/a")),
        ("standpoint", Authored::Iri("https://example.org/c")),
        ("standpoint", Authored::Iri("https://example.org/d")),
    ]);
    assert!(scopes.is_empty(), "{scopes:?}");
    assert!(
        codes.iter().any(|c| c == "AMBIGUOUS_SCOPE_COORDINATES"),
        "{codes:?}"
    );
}
