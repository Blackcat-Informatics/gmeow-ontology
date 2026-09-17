// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::parse_logic_str;

/// The `logic:` namespace prefix + rdf, used by every authored-RDF fixture below.
const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <https://ex/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

/// Parse a `logic:` Turtle fixture and return its constraints (asserting no parse error).
fn constraints_of(turtle: &str) -> Vec<ConstraintIr> {
    let src = format!("{PREFIXES}{turtle}");
    let (program, diagnostics) = parse_logic_str(&src, None).expect("fixture must parse");
    // A malformed-constraint fixture would surface a MALFORMED_CONSTRAINT warning; the
    // seven pattern fixtures are all well-formed, so none is expected.
    assert!(
        !diagnostics.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
        "unexpected MALFORMED_CONSTRAINT diagnostics: {diagnostics:?}"
    );
    program.constraints
}

/// A guarded `∀ this. rdf:type(this, ex:Widget) → <body>` scaffold, so each pattern
/// fixture only has to author its per-focus condition `<body>`.
fn guarded(iri: &str, body_ttl: &str, body_node: &str) -> String {
    format!(
        "\
{iri} a logic:Constraint ;
  logic:severity \"Violation\" ;
  logic:integrity {iri}_all .

{iri}_all a logic:Formula ;
  logic:forall {iri}_impl ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"this\" ] .

{iri}_impl a logic:Formula ;
  logic:antecedent {iri}_guard ;
  logic:consequent {body_node} .

{iri}_guard a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Widget ] .

{body_ttl}
"
    )
}

#[test]
fn p1_choice_group_exactly_one_round_trips() {
    // ∀ this. Widget(this) → ((∃a. hasA(this,a) ∧ ¬∃b. hasB(this,b))
    //                        ∨ (¬∃a. hasA(this,a) ∧ ∃b. hasB(this,b)))
    let body = "\
ex:c1_body a logic:Formula ;
  logic:or ex:c1_left , ex:c1_right .

ex:c1_left a logic:Formula ;
  logic:and ex:c1_a , ex:c1_notb .
ex:c1_right a logic:Formula ;
  logic:and ex:c1_nota , ex:c1_b .

ex:c1_a a logic:Formula ;
  logic:exists ex:c1_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c1_b a logic:Formula ;
  logic:exists ex:c1_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c1_notb a logic:Formula ; logic:not ex:c1_b .
ex:c1_nota a logic:Formula ; logic:not ex:c1_a .

ex:c1_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c1_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
    let cs = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    // The key is stable across a re-parse of the identical source.
    let again = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
    assert_eq!(cs[0].content_key(), again[0].content_key());
}

#[test]
fn p2_guarded_implication_round_trips() {
    // ∀ this. Widget(this) → ∃c. companion(this, c)
    let body = "\
ex:c2_body a logic:Formula ;
  logic:exists ex:c2_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:c2_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
    let cs = constraints_of(&guarded("ex:c2", body, "ex:c2_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    assert!(cs[0].content_key().contains("class="));
}

#[test]
fn authored_constraint_failure_class_dedupes_identical_values() {
    let body = "\
ex:fc_body a logic:Formula ;
  logic:exists ex:fc_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
    let turtle = format!(
        "{}\nex:fc <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:Failure, ex:Failure .",
        guarded("ex:fc", body, "ex:fc_body")
    );
    let cs = constraints_of(&turtle);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].failure_class.as_deref(), Some("https://ex/Failure"));
}

#[test]
fn authored_constraint_failure_class_rejects_distinct_values() {
    let body = "\
ex:fc_bad_body a logic:Formula ;
  logic:exists ex:fc_bad_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_bad_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
    let turtle = format!(
        "{PREFIXES}{}\nex:fc_bad <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:FailureA, ex:FailureB .",
        guarded("ex:fc_bad", body, "ex:fc_bad_body")
    );
    let (program, diagnostics) = parse_logic_str(&turtle, None).expect("fixture must parse");
    assert!(program.constraints.is_empty());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MALFORMED_CONSTRAINT" && diagnostic.message.contains("distinct")
    }));
}

#[test]
fn authored_constraint_failure_class_rejects_literal_value() {
    let body = "\
ex:fc_literal_body a logic:Formula ;
  logic:exists ex:fc_literal_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_literal_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
    let turtle = format!(
        "{PREFIXES}{}\nex:fc_literal <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> \"Failure\" .",
        guarded("ex:fc_literal", body, "ex:fc_literal_body")
    );
    let (program, diagnostics) = parse_logic_str(&turtle, None).expect("fixture must parse");
    assert!(program.constraints.is_empty());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MALFORMED_CONSTRAINT" && diagnostic.message.contains("must be an IRI")
    }));
}

#[test]
fn p3_disjunctive_requiredness_round_trips() {
    // ∀ this. Widget(this) → (∃a. hasA(this,a) ∨ ∃b. hasB(this,b))
    let body = "\
ex:c3_body a logic:Formula ;
  logic:or ex:c3_a , ex:c3_b .
ex:c3_a a logic:Formula ;
  logic:exists ex:c3_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c3_b a logic:Formula ;
  logic:exists ex:c3_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c3_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c3_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
    let cs = constraints_of(&guarded("ex:c3", body, "ex:c3_body"));
    assert_eq!(cs.len(), 1);
    // Disjunctive body ⇒ the integrity formula carries the Disjunctive shape tag.
    assert!(
        cs[0]
            .integrity
            .shape_tags()
            .contains(&crate::ir::FormulaShape::Disjunctive)
    );
}

#[test]
fn p4_path_value_type_membership_round_trips() {
    // ∀ this. Widget(this) → ∀v. part(this, v) → Part(v)
    let body = "\
ex:c4_body a logic:Formula ;
  logic:forall ex:c4_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"v\" ] .
ex:c4_inner a logic:Formula ;
  logic:antecedent ex:c4_path ;
  logic:consequent ex:c4_type .
ex:c4_path a logic:Formula ;
  logic:relation ex:part ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"v\" ] .
ex:c4_type a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"v\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Part ] .";
    let cs = constraints_of(&guarded("ex:c4", body, "ex:c4_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
}

#[test]
fn p5_cross_node_co_occurrence_round_trips() {
    // ∀ this. Widget(this) → ∀o. linked(this, o) → ∃m. marker(o, m)
    let body = "\
ex:c5_body a logic:Formula ;
  logic:forall ex:c5_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"o\" ] .
ex:c5_inner a logic:Formula ;
  logic:antecedent ex:c5_link ;
  logic:consequent ex:c5_ex .
ex:c5_link a logic:Formula ;
  logic:relation ex:linked ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"o\" ] .
ex:c5_ex a logic:Formula ;
  logic:exists ex:c5_marker ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"m\" ] .
ex:c5_marker a logic:Formula ;
  logic:relation ex:marker ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"o\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"m\" ] .";
    let cs = constraints_of(&guarded("ex:c5", body, "ex:c5_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
}

#[test]
fn p6_aggregate_comparison_round_trips() {
    // ∀ this. Widget(this) → ∃n. (partCount(this, n) ∧ atMost(n, "10"^^xsd:integer))
    //
    // NOTE (P6 aggregation finding): the realized FOL `Formula` core has NO aggregate /
    // reduce node — `AggregateSpec` is a `LogicRule`-only construct with no formula-level
    // analogue. An aggregate comparison is therefore authored the ONLY honest FOL way: as
    // an atomic predication over a reified aggregate relation (`partCount(this, n)`) plus a
    // comparison atom (`atMost(n, 10)`). This is a genuine FOL encoding, not a stub — it
    // round-trips with a stable key like every other pattern.
    let body = "\
ex:c6_body a logic:Formula ;
  logic:exists ex:c6_conj ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"n\" ] .
ex:c6_conj a logic:Formula ;
  logic:and ex:c6_count , ex:c6_cmp .
ex:c6_count a logic:Formula ;
  logic:relation ex:partCount ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"n\" ] .
ex:c6_cmp a logic:Formula ;
  logic:relation ex:atMost ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"n\" ] ,
                 [ logic:termIndex 1 ; logic:termLiteral \"10\" ;
                   logic:termLiteralDatatype xsd:integer ] .";
    let cs = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    let again = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
    assert_eq!(cs[0].content_key(), again[0].content_key());
}

#[test]
fn p7_forbidden_pattern_round_trips() {
    // ∀ this. Widget(this) → ¬∃b. forbidden(this, b)
    let body = "\
ex:c7_body a logic:Formula ; logic:not ex:c7_ex .
ex:c7_ex a logic:Formula ;
  logic:exists ex:c7_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c7_atom a logic:Formula ;
  logic:relation ex:forbidden ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
    let cs = constraints_of(&guarded("ex:c7", body, "ex:c7_body"));
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    assert!(
        cs[0]
            .integrity
            .shape_tags()
            .contains(&crate::ir::FormulaShape::StrongNegation)
    );
}

#[test]
fn subjects_of_and_objects_of_targets_are_derived_from_the_guard() {
    // A predicate-guard `P(this, _)` ⇒ SubjectsOf; `P(_, this)` ⇒ ObjectsOf.
    let this = Term::Var("this".into());
    let other = Term::Var("y".into());
    let pred = Term::Iri("https://ex/P".into());
    let guard_subj = Formula::atom(pred.clone(), vec![this.clone(), other.clone()]).unwrap();
    let guard_obj = Formula::atom(pred.clone(), vec![other, this.clone()]).unwrap();
    let cond = Formula::atom(Term::Iri("https://ex/ok".into()), vec![this.clone()]).unwrap();
    let mk = |guard: Formula| Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(Box::new(guard), Box::new(cond.clone()))),
    };
    let subj = ConstraintIr::new(
        "https://ex/cs",
        mk(guard_subj),
        ShaclSeverity::Violation,
        None,
    )
    .unwrap();
    assert_eq!(subj.target, ShapeTarget::SubjectsOf("https://ex/P".into()));
    let obj = ConstraintIr::new(
        "https://ex/co",
        mk(guard_obj),
        ShaclSeverity::Violation,
        None,
    )
    .unwrap();
    assert_eq!(obj.target, ShapeTarget::ObjectsOf("https://ex/P".into()));
}

#[test]
fn message_is_excluded_from_content_key() {
    let this = Term::Var("this".into());
    let integrity = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::Iri(RDF_TYPE.into()),
                    vec![this.clone(), Term::Iri("https://ex/W".into())],
                )
                .unwrap(),
            ),
            Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
        )),
    };
    let a = ConstraintIr::new(
        "https://ex/c",
        integrity.clone(),
        ShaclSeverity::Violation,
        Some("first message".into()),
    )
    .unwrap();
    let b = ConstraintIr::new(
        "https://ex/c",
        integrity,
        ShaclSeverity::Violation,
        Some("a completely different message".into()),
    )
    .unwrap();
    assert_eq!(
        a.content_key(),
        b.content_key(),
        "message must not affect the content key"
    );
    // Formalizes is likewise annotation-level and excluded.
    let c = a.clone().with_formalizes("https://ex/gmeow/Term").unwrap();
    assert_eq!(a.content_key(), c.content_key());
    let typed = a.clone().with_failure_class("https://ex/Failure").unwrap();
    assert_eq!(a.content_key(), typed.content_key());
    let err = typed
        .with_failure_class("https://ex/OtherFailure")
        .unwrap_err();
    assert!(err.message().contains("duplicate"));
}

#[test]
fn target_extraction_hard_fails_on_a_non_guarded_formula() {
    // A bare atom (no ∀) is not a range-restricted constraint.
    let bare = Formula::atom(
        Term::Iri("https://ex/p".into()),
        vec![Term::Var("x".into()), Term::Var("y".into())],
    )
    .unwrap();
    let err = ConstraintIr::new("https://ex/c", bare, ShaclSeverity::Violation, None).unwrap_err();
    assert!(
        err.message().contains("range-restricted universal"),
        "got: {err}"
    );

    // A ∀ whose body is not an implication (no guard) also fails.
    let unguarded = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(
            Formula::atom(
                Term::Iri("https://ex/p".into()),
                vec![Term::Var("this".into())],
            )
            .unwrap(),
        ),
    };
    let err =
        ConstraintIr::new("https://ex/c", unguarded, ShaclSeverity::Violation, None).unwrap_err();
    assert!(err.message().contains("guarded implication"), "got: {err}");
}

#[test]
fn aggregate_satellite_participates_in_the_content_key() {
    // Two constraints identical but for their aggregate satellite must have distinct identities,
    // and an aggregate-free peer must keep the byte-identical historical key (append-only).
    let this = Term::Var("this".into());
    let integrity = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::Iri(RDF_TYPE.into()),
                    vec![this.clone(), Term::Iri("https://ex/W".into())],
                )
                .unwrap(),
            ),
            Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
        )),
    };
    let base =
        ConstraintIr::new("https://ex/c", integrity, ShaclSeverity::Violation, None).unwrap();
    let eq = base.clone().with_aggregate(
        AggregateComparison::new(
            "COUNT",
            true,
            "https://ex/hasAxis",
            AggregateComparator::Eq,
            AggregateRhs::Property("https://ex/dimensionCount".into()),
        )
        .unwrap(),
    );
    // A different comparator ⇒ a different identity.
    let ne = base.clone().with_aggregate(
        AggregateComparison::new(
            "COUNT",
            true,
            "https://ex/hasAxis",
            AggregateComparator::Ne,
            AggregateRhs::Property("https://ex/dimensionCount".into()),
        )
        .unwrap(),
    );
    assert_ne!(base.content_key(), eq.content_key());
    assert_ne!(eq.content_key(), ne.content_key());
    assert!(eq.content_key().contains("agg="));
    assert!(!base.content_key().contains("agg="));
}

#[test]
fn join_aggregate_satellite_participates_in_the_content_key_and_is_append_only() {
    // A join-aggregate satellite gives a distinct identity; an aggregate/join-free peer keeps
    // the byte-identical historical key (append-only), and two satellites differing only in a
    // leg's value predicate differ in identity.
    let this = Term::Var("this".into());
    let integrity = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::Iri(RDF_TYPE.into()),
                    vec![this.clone(), Term::Iri("https://ex/TopCell".into())],
                )
                .unwrap(),
            ),
            Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
        )),
    };
    let base =
        ConstraintIr::new("https://ex/bsq", integrity, ShaclSeverity::Violation, None).unwrap();
    let leg = |value: &str| {
        JoinLeg::new(
            Some("https://ex/Incidence".to_owned()),
            "https://ex/incidenceCoface",
            "https://ex/incidenceFace",
            value,
        )
        .unwrap()
    };
    let ja = JoinAggregate::new(
        "SUM",
        vec![
            leg("https://ex/incidenceSign"),
            leg("https://ex/incidenceSign"),
        ],
        AggregateComparator::Eq,
        "0",
        Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
    )
    .unwrap();
    let with_ja = base.clone().with_join_aggregate(ja);
    assert!(!base.content_key().contains("joinagg="));
    assert!(with_ja.content_key().contains("joinagg="));
    assert_ne!(base.content_key(), with_ja.content_key());

    // A leg-value difference changes identity.
    let ja_alt = JoinAggregate::new(
        "SUM",
        vec![leg("https://ex/incidenceSign"), leg("https://ex/otherSign")],
        AggregateComparator::Eq,
        "0",
        Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
    )
    .unwrap();
    assert_ne!(
        with_ja.content_key(),
        base.clone().with_join_aggregate(ja_alt).content_key()
    );

    // A single hop is not a JOIN.
    assert!(
        JoinAggregate::new(
            "SUM",
            vec![leg("https://ex/incidenceSign")],
            AggregateComparator::Eq,
            "0",
            None,
        )
        .is_err()
    );
}

#[test]
fn aggregate_comparator_negation_and_symbol_parsing() {
    assert_eq!(AggregateComparator::Eq.negated(), AggregateComparator::Ne);
    assert_eq!(AggregateComparator::Lt.negated(), AggregateComparator::Ge);
    assert_eq!(
        AggregateComparator::from_symbol("≥"),
        Some(AggregateComparator::Ge)
    );
    assert_eq!(
        AggregateComparator::from_symbol("!="),
        Some(AggregateComparator::Ne)
    );
    assert_eq!(AggregateComparator::from_symbol("~"), None);
    assert!(
        AggregateComparison::new(
            "MEAN",
            false,
            "https://ex/p",
            AggregateComparator::Eq,
            AggregateRhs::Literal {
                lexical: "1".into(),
                datatype: None
            },
        )
        .is_err()
    );
}

#[test]
fn empty_constraints_program_content_key_is_byte_identical() {
    // A program with no constraints must fold to the exact same canonical key as one
    // constructed before the constraints field existed — the append-only guarantee.
    use crate::ir::{LogicAxiom, LogicProgram};
    let ax = LogicAxiom::ground(
        "https://ex/s",
        "https://ex/p",
        crate::ir::AtomicTerm::resource("https://ex/o"),
    )
    .unwrap();
    let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    let with_empty = LogicProgram::new(vec![ax], vec![], vec![], None).with_constraints(vec![]);
    assert_eq!(
        base.canonical_key(),
        with_empty.canonical_key(),
        "an empty-constraints program must keep the byte-identical historical key"
    );
    assert!(!base.canonical_key().contains("CONSTRAINTS"));
}

#[test]
fn non_empty_constraints_perturb_the_program_key() {
    use crate::ir::LogicProgram;
    let this = Term::Var("this".into());
    let integrity = Formula::Forall {
        vars: vec!["this".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::Iri(RDF_TYPE.into()),
                    vec![this.clone(), Term::Iri("https://ex/W".into())],
                )
                .unwrap(),
            ),
            Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
        )),
    };
    let c = ConstraintIr::new("https://ex/c", integrity, ShaclSeverity::Violation, None).unwrap();
    let base = LogicProgram::new(vec![], vec![], vec![], None);
    let with_c = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    assert_ne!(base.canonical_key(), with_c.canonical_key());
    assert!(with_c.canonical_key().contains("CONSTRAINTS"));
}
