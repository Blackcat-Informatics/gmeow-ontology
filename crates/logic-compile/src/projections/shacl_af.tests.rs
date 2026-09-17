// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::super::super::ir::{
    AggregateSpec, AtomicTerm, ContextualScope, LogicAxiom, LogicProgram, LogicRule,
};
use super::*;

use crate::loss_ledger::LossLedger;

fn var_axiom(s: &str, p: &str, o: &str) -> LogicAxiom {
    LogicAxiom::new(
        s,
        p,
        crate::ir::AtomicTerm::resource(o),
        false,
        ContextualScope::default(),
    )
    .unwrap()
}

/// Run the projection with a fresh loss store and return both — the store is where every
/// per-run drop now lives (the `ProjectionResult` no longer carries `actual_drops`).
fn project(program: &LogicProgram) -> (ProjectionResult, LossLedger) {
    let mut loss = LossLedger::new();
    let result = project_shacl_af(program, &mut loss);
    (result, loss)
}

/// The per-run ACTUAL drop notes for `shacl-af`, recovered from the loss store with the
/// report's `actual: ` read-back prefix stripped — exactly the old `result.actual_drops`.
fn actual_drops(loss: &LossLedger) -> Vec<String> {
    loss.projection_drops_for("shacl-af")
        .iter()
        .filter_map(|d| d.strip_prefix("actual: ").map(str::to_owned))
        .collect()
}

/// A small program with one Horn derivation rule:
/// `gmeow:knowsAbout(?agent, ?subject) :- logic:assessedAgent(?a, ?agent),
///  logic:assessmentSubject(?a, ?subject).`
fn ladder_program() -> LogicProgram {
    let head = var_axiom(
        "?agent",
        "https://blackcatinformatics.ca/gmeow/knowsAbout",
        "?subject",
    );
    let body = vec![
        var_axiom(
            "?a",
            "https://blackcatinformatics.ca/logic/assessedAgent",
            "?agent",
        ),
        var_axiom(
            "?a",
            "https://blackcatinformatics.ca/logic/assessmentSubject",
            "?subject",
        ),
    ];
    let rule = LogicRule::new(head, body, vec![], ContextualScope::default());
    LogicProgram::new(vec![], vec![rule], vec![], None)
}

#[test]
fn projects_a_horn_rule_to_a_sparql_rule_node_shape() {
    let (result, _loss) = project(&ladder_program());
    let ttl = &result.content;
    // The doc declares its preservation honestly.
    assert_eq!(result.target, "shacl-af");
    assert_eq!(result.preservation.as_str(), "SoundUnderApproximation");
    // One NodeShape carrying a SHACL-AF SPARQLRule with a CONSTRUCT.
    assert!(ttl.contains("a sh:NodeShape"), "no NodeShape:\n{ttl}");
    assert!(ttl.contains("a sh:SPARQLRule"), "no SPARQLRule:\n{ttl}");
    assert!(
        ttl.contains("sh:construct"),
        "the rule must carry a CONSTRUCT:\n{ttl}"
    );
    // The head subject is projected as the focus node $this, the head predicate as an IRI.
    assert!(
        ttl.contains(
            "CONSTRUCT { $this <https://blackcatinformatics.ca/gmeow/knowsAbout> ?subject }"
        ),
        "head not lowered as a focus-node CONSTRUCT:\n{ttl}"
    );
    // The body atoms become graph patterns with the focus var bound to $this.
    assert!(
        ttl.contains("<https://blackcatinformatics.ca/logic/assessedAgent> $this"),
        "body atom not lowered with the focus var:\n{ttl}"
    );
    // The SPARQLTarget selects the focus nodes as ?this.
    assert!(
        ttl.contains("SELECT ?this WHERE"),
        "no SPARQLTarget select:\n{ttl}"
    );
}

#[test]
fn naf_and_distinct_lower_to_filters() {
    let head = var_axiom("?x", "https://blackcatinformatics.ca/gmeow/derived", "?y");
    let mut neg = var_axiom("?x", "https://blackcatinformatics.ca/logic/blocked", "?y");
    neg.negated = true;
    let pos = var_axiom("?x", "https://blackcatinformatics.ca/logic/links", "?y");
    let rule = LogicRule::new(
        head,
        vec![pos, neg],
        vec![("?x".to_owned(), "?y".to_owned())],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let ttl = project(&program).0.content;
    assert!(
        ttl.contains("FILTER NOT EXISTS"),
        "negation-as-failure must lower to FILTER NOT EXISTS:\n{ttl}"
    );
    assert!(
        ttl.contains("FILTER ( $this != ?y )") || ttl.contains("FILTER ( ?y != $this )"),
        "the inequality guard must lower to a FILTER:\n{ttl}"
    );
}

#[test]
fn modal_scoped_rule_is_carried_not_emitted() {
    let scope = ContextualScope {
        standpoint: Some("https://blackcatinformatics.ca/gmeow/someStandpoint".to_owned()),
        ..ContextualScope::default()
    };
    let head = var_axiom(
        "?x",
        "https://blackcatinformatics.ca/gmeow/scopedDerived",
        "?y",
    );
    let body = vec![var_axiom(
        "?x",
        "https://blackcatinformatics.ca/logic/links",
        "?y",
    )];
    let rule = LogicRule::new(head, body, vec![], scope);
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let (result, loss) = project(&program);
    // A context-scoped rule is NOT emitted as a SPARQLRule …
    assert!(
        !result.content.contains("scopedDerived"),
        "a context-scoped rule must not be emitted:\n{}",
        result.content
    );
    // … and the drop is disclosed, never silent.
    let drops = actual_drops(&loss);
    assert!(
        drops.iter().any(|d| d.contains("context-scoped")),
        "the skipped modal rule must be recorded as a drop: {drops:?}"
    );
}

#[test]
fn head_vars_bound_only_as_body_objects_still_emit() {
    // A ladder rule binds both head variables only in body-object positions.
    // The source reader resolves variable syntax before this typed projection runs.
    let head = var_axiom("?a", "https://blackcatinformatics.ca/gmeow/knows", "?b");
    let body = vec![
        // ?a and ?b appear only as explicitly typed object variables.
        var_axiom(
            "?p",
            "https://blackcatinformatics.ca/logic/assessedAgent",
            "?a",
        ),
        var_axiom("?p", "https://blackcatinformatics.ca/logic/subject", "?b"),
    ];
    let rule = LogicRule::new(head, body, vec![], ContextualScope::default());
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let (result, loss) = project(&program);
    assert!(
        result.content.contains("a sh:SPARQLRule"),
        "a rule whose head vars are bound as body objects must emit:\n{}",
        result.content
    );
    let drops = actual_drops(&loss);
    assert!(
        drops.is_empty(),
        "no drop expected for a fully-bound rule: {drops:?}"
    );
}

#[test]
fn question_mark_literals_do_not_bind_rule_head_variables() {
    let mut program = ladder_program();
    for atom in &mut program.rules[0].body {
        let name = atom.obj.as_variable().unwrap().to_owned();
        atom.obj = AtomicTerm::Literal(purrdf::RdfLiteral::simple(name));
    }
    let (result, loss) = project(&program);
    assert!(!result.content.contains("a sh:SPARQLRule"));
    assert!(
        actual_drops(&loss)
            .iter()
            .any(|drop| drop.contains("not positively bound"))
    );
}

#[test]
fn head_object_bound_only_by_negation_is_carried_not_emitted() {
    // Head: gmeow:derived(?x, ?y). Body binds ?x positively, but ?y appears ONLY inside a
    // negated atom (FILTER NOT EXISTS), so ?y is out of scope in the surrounding SPARQL.
    // Emitting would produce an unbound CONSTRUCT object — the rule must be carried, not emitted.
    let head = var_axiom("?x", "https://blackcatinformatics.ca/gmeow/derived", "?y");
    let pos = var_axiom("?x", "https://blackcatinformatics.ca/logic/links", "?z");
    let mut neg = var_axiom("?x", "https://blackcatinformatics.ca/logic/blocked", "?y");
    neg.negated = true;
    let rule = LogicRule::new(head, vec![pos, neg], vec![], ContextualScope::default());
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let (result, loss) = project(&program);
    assert!(
        !result.content.contains("a sh:SPARQLRule"),
        "a rule with a negation-only head object must NOT be emitted:\n{}",
        result.content
    );
    let drops = actual_drops(&loss);
    assert!(
        drops
            .iter()
            .any(|d| d.contains("existential") && d.contains("head object")),
        "the unbound head object must be a ledgered drop, never silent: {drops:?}"
    );
}

#[test]
fn head_subject_bound_only_by_negation_is_carried_not_emitted() {
    // Head subject ?x is a variable but appears ONLY inside a negated body atom, so it is not
    // positively bound: the target SELECT ?this would be unbound. Carry, do not emit.
    let head = var_axiom("?x", "https://blackcatinformatics.ca/gmeow/derived", "?y");
    let pos = var_axiom("?w", "https://blackcatinformatics.ca/logic/links", "?y");
    let mut neg = var_axiom("?x", "https://blackcatinformatics.ca/logic/blocked", "?y");
    neg.negated = true;
    let rule = LogicRule::new(head, vec![pos, neg], vec![], ContextualScope::default());
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let (result, loss) = project(&program);
    assert!(
        !result.content.contains("a sh:SPARQLRule"),
        "a rule with a negation-only head subject must NOT be emitted:\n{}",
        result.content
    );
    let drops = actual_drops(&loss);
    assert!(
        drops
            .iter()
            .any(|d| d.contains("head subject") && d.contains("not positively bound")),
        "the unbound head subject must be a ledgered drop, never silent: {drops:?}"
    );
}

#[test]
fn sparql_literal_escapes_both_the_sparql_and_turtle_layers() {
    // A nasty value: a triple-quote (would end the Turtle long string), a real newline (illegal
    // raw in a single-quoted SPARQL string), a backslash, and a single quote.
    let rendered = sparql_literal("a\"\"\"b\nc\\d'e");
    // No raw triple-quote can terminate the enclosing Turtle """…""".
    assert!(
        !rendered.contains("\"\"\""),
        "raw triple-quote leaks into the Turtle carrier: {rendered}"
    );
    // No raw control character survives (the SPARQL single-quoted string forbids it).
    assert!(
        !rendered.contains('\n'),
        "raw newline leaks into the SPARQL literal: {rendered:?}"
    );
    // The newline is carried as a doubly-escaped sequence: Turtle un-escapes `\\n` → `\n`,
    // which the SPARQL layer then reads as a newline.
    assert!(
        rendered.contains("\\\\n"),
        "newline not double-escaped for the two layers: {rendered}"
    );
    // Each double-quote is Turtle-escaped so it cannot start a `"""`.
    assert!(
        rendered.contains("\\\""),
        "double-quote not Turtle-escaped: {rendered}"
    );
}

#[test]
fn rule_with_special_char_literal_object_round_trips_safely() {
    // A rule whose head object is a literal containing a quote + newline must still produce a
    // single, well-formed CONSTRUCT with no premature Turtle long-string termination.
    let head = LogicAxiom::new(
        "?x",
        "https://blackcatinformatics.ca/gmeow/note",
        crate::ir::AtomicTerm::Literal(purrdf::RdfLiteral::simple("line1\nsays \"\"\"hi\"\"\"")),
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let body = vec![var_axiom(
        "?x",
        "https://blackcatinformatics.ca/logic/links",
        "?y",
    )];
    let rule = LogicRule::new(head, body, vec![], ContextualScope::default());
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let ttl = project(&program).0.content;
    // Exactly one opening and one closing triple-quote per embedded SPARQL string (select +
    // construct = 4 total); a leaked `"""` from the literal would push this higher.
    assert_eq!(
        ttl.matches("\"\"\"").count(),
        4,
        "literal special chars broke the Turtle long-string boundary:\n{ttl}"
    );
}

#[test]
fn reduce_rule_projects_to_an_aggregating_sparql_rule_with_group_by() {
    // ?g gmeow:total ?sum :- ?g gmeow:hasItem ?x  [ SUM(?x) AS ?sum GROUP BY ?g ]
    let head = var_axiom("?g", "https://blackcatinformatics.ca/gmeow/total", "?sum");
    let body = vec![var_axiom(
        "?g",
        "https://blackcatinformatics.ca/gmeow/hasItem",
        "?x",
    )];
    let rule = LogicRule::new(head, body, vec![], ContextualScope::default()).with_aggregation(
        AggregateSpec::new("SUM", "?x", "?sum", vec!["?g".to_owned()]),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let ttl = project(&program).0.content;
    assert!(
        ttl.contains("a sh:SPARQLRule"),
        "the reduce rule must project to a SPARQLRule:\n{ttl}"
    );
    assert!(
        ttl.contains("SUM(?x) AS ?sum"),
        "the aggregate function must lower to a SPARQL aggregate:\n{ttl}"
    );
    assert!(
        ttl.contains("GROUP BY $this"),
        "the reduce must carry a GROUP BY over the focus group key:\n{ttl}"
    );
    assert!(
        ttl.contains("CONSTRUCT { $this <https://blackcatinformatics.ca/gmeow/total> ?sum }"),
        "the head must derive the aggregate result per group:\n{ttl}"
    );
}

#[test]
fn every_rule_and_axiom_is_emitted_or_ledgered_never_silent() {
    // The no-silent-drop contract for the SoundUnderApproximation surface: each input rule and
    // axiom is EITHER projected to a shape OR recorded as a ledger drop — never silently lost.
    // A projectable rule, a modal rule (skip), a projectable subClassOf axiom, and a type
    // axiom (skip): exactly 2 emitted shapes and 2 ledger drops, accounting for all 4 inputs.
    let proj_rule = {
        let head = var_axiom("?a", "https://blackcatinformatics.ca/gmeow/knows", "?b");
        let body = vec![var_axiom(
            "?a",
            "https://blackcatinformatics.ca/logic/links",
            "?b",
        )];
        LogicRule::new(head, body, vec![], ContextualScope::default())
    };
    let modal_rule = {
        let scope = ContextualScope {
            standpoint: Some("https://blackcatinformatics.ca/gmeow/sp".to_owned()),
            ..ContextualScope::default()
        };
        let head = var_axiom("?x", "https://blackcatinformatics.ca/gmeow/scoped", "?y");
        let body = vec![var_axiom(
            "?x",
            "https://blackcatinformatics.ca/logic/links",
            "?y",
        )];
        LogicRule::new(head, body, vec![], scope)
    };
    let subclass = LogicAxiom::ground(
        "https://example.org/A",
        "https://blackcatinformatics.ca/logic/subClassOf",
        crate::ir::AtomicTerm::resource("https://example.org/B"),
    )
    .unwrap();
    let type_axiom = LogicAxiom::ground(
        "https://example.org/A",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        crate::ir::AtomicTerm::resource("https://blackcatinformatics.ca/logic/Kind"),
    )
    .unwrap();
    let program = LogicProgram::new(
        vec![subclass, type_axiom],
        vec![proj_rule, modal_rule],
        vec![],
        None,
    );
    let (result, loss) = project(&program);
    // 2 inputs are projectable → 2 NodeShapes.
    assert_eq!(
        result.content.matches("a sh:NodeShape").count(),
        2,
        "expected exactly two emitted shapes:\n{}",
        result.content
    );
    // The other 2 inputs are carried → 2 ledger drops (no contracts/formulas here, so the
    // ledger holds only the skip notes).
    let drops = actual_drops(&loss);
    assert_eq!(
        drops.len(),
        2,
        "every non-projected input must be a ledgered drop, never silent: {drops:?}"
    );
}

#[test]
fn subclass_axiom_projects_to_a_subsumption_rule() {
    // A ground subClassOf axiom is projected to a cax-sco sh:SPARQLRule that materializes the
    // subsumption — maximal utility, the SHACL-AF surface actually computes the closure.
    let axiom = LogicAxiom::ground(
        "https://example.org/HonorsStudent",
        "https://blackcatinformatics.ca/logic/subClassOf",
        crate::ir::AtomicTerm::resource("https://example.org/Student"),
    )
    .unwrap();
    let program = LogicProgram::new(vec![axiom], vec![], vec![], None);
    let (result, _loss) = project(&program);
    let ttl = &result.content;
    assert!(
        ttl.contains("a sh:SPARQLRule"),
        "the subClassOf axiom must project to a SPARQLRule:\n{ttl}"
    );
    assert!(
        ttl.contains("CONSTRUCT { $this a <https://example.org/Student> }"),
        "cax-sco must derive the superclass type:\n{ttl}"
    );
    assert!(
        ttl.contains("$this a <https://example.org/HonorsStudent> ."),
        "the rule must trigger on the subclass type:\n{ttl}"
    );
}

#[test]
fn non_subsumption_axiom_is_carried_not_silently_dropped() {
    // A ground metamodel/type axiom (not a subsumption) has no derivation form: it must be a
    // ledgered drop, never a silent disappearance.
    let axiom = LogicAxiom::ground(
        "https://example.org/Student",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        crate::ir::AtomicTerm::resource("https://blackcatinformatics.ca/logic/Role"),
    )
    .unwrap();
    let program = LogicProgram::new(vec![axiom], vec![], vec![], None);
    let (result, loss) = project(&program);
    assert!(
        !result.content.contains("a sh:SPARQLRule"),
        "a non-subsumption ground axiom must NOT be projected as a rule:\n{}",
        result.content
    );
    let drops = actual_drops(&loss);
    assert!(
        drops
            .iter()
            .any(|d| d.contains("ground axiom") && d.contains("asserted fact")),
        "the non-projected ground axiom must be a ledgered drop, never silent: {drops:?}"
    );
}

/// Shift-left for the A-Box annotation contract (`gmeow-errors::abox`): every
/// `sh:NodeShape` [`emit_rule_shape`] mints carries all four mandatory annotations
/// (`rdfs:label`, `skos:definition`, `rdfs:isDefinedBy`, `gmeow:graphBoxRole`); the
/// label/definition literals carry the `x-gmeow-english` carrier tag (never bare
/// `en`); `isDefinedBy` points at [`SHACL_AF_GRAPH_IRI`]; `graphBoxRole` is
/// `gmeow:boxABox`.
///
/// `gmeow-logic-compile` has zero dependency on `gmeow-validate` (the reverse
/// dependency would cycle: `gmeow-validate` depends on this crate), so this parses
/// the emitted Turtle directly and asserts on it, rather than driving
/// `gmeow_validate::lint::structural_lint_dataset` as the pipeline-level
/// frame-shapes/result-shapes tests do.
#[test]
fn rule_shapes_carry_the_full_abox_annotation_contract() {
    use crate::graphutil::{Node, Subject, nn, objects};
    use gmeow_errors::abox::{
        BOX_ABOX, GRAPH_BOX_ROLE, RDFS_IS_DEFINED_BY, RDFS_LABEL, SKOS_DEFINITION, X_GMEOW_ENGLISH,
    };

    let (result, _loss) = project(&ladder_program());
    let ttl = &result.content;
    let dataset =
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse shacl-af");
    let ds = dataset.as_ref();

    let subject = Subject::Iri(format!("{GMEOW_NS}GenComputeRule_knowsAbout_r0"));

    let labels = objects(ds, &subject, &nn(RDFS_LABEL));
    assert_eq!(labels.len(), 1, "exactly one rdfs:label: {labels:?}");
    match &labels[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(
                lexical,
                "SHACL-AF projection of the logic: rule deriving \
                     <https://blackcatinformatics.ca/gmeow/knowsAbout> (generated)"
            );
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("rdfs:label must be a literal: {other:?}"),
    }

    let definitions = objects(ds, &subject, &nn(SKOS_DEFINITION));
    assert_eq!(
        definitions.len(),
        1,
        "exactly one skos:definition: {definitions:?}"
    );
    match &definitions[0] {
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            language: lang,
            ..
        }) => {
            assert_eq!(
                lexical,
                "SHACL-AF rule shape deriving <https://blackcatinformatics.ca/gmeow/knowsAbout>."
            );
            assert_eq!(lang.as_deref(), Some(X_GMEOW_ENGLISH));
        }
        other => panic!("skos:definition must be a literal: {other:?}"),
    }

    assert_eq!(
        objects(ds, &subject, &nn(RDFS_IS_DEFINED_BY)),
        vec![Node::iri(SHACL_AF_GRAPH_IRI)],
        "rdfs:isDefinedBy must point at the shacl-af document graph identity"
    );
    assert_eq!(
        objects(ds, &subject, &nn(GRAPH_BOX_ROLE)),
        vec![Node::iri(BOX_ABOX)],
        "graphBoxRole must be gmeow:boxABox"
    );
}
