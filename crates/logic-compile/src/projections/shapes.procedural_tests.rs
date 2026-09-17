// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{Formula, ShaclSeverity, Term};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const WIDGET: &str = "https://ex/Widget";

fn tvar(n: &str) -> Term {
    Term::Var(n.to_owned())
}
fn tiri(n: &str) -> Term {
    Term::Iri(n.to_owned())
}
fn atom(rel: &str, a: Term, b: Term) -> Formula {
    Formula::atom(tiri(rel), vec![a, b]).unwrap()
}
fn exists(v: &str, body: Formula) -> Formula {
    Formula::Exists {
        vars: vec![v.to_owned()],
        body: Box::new(body),
    }
}
fn forall(v: &str, body: Formula) -> Formula {
    Formula::Forall {
        vars: vec![v.to_owned()],
        body: Box::new(body),
    }
}

/// Wrap a per-focus condition `phi(this)` in the range-restricted guard
/// `∀ this. rdf:type(this, ex:Widget) → phi`, and build the constraint.
fn guarded(iri: &str, phi: Formula) -> ConstraintIr {
    let integrity = Formula::Forall {
        vars: vec!["this".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(atom(RDF_TYPE, tvar("this"), tiri(WIDGET))),
            Box::new(phi),
        )),
    };
    ConstraintIr::new(iri, integrity, ShaclSeverity::Violation, None).unwrap()
}

fn block(c: &ConstraintIr) -> String {
    let b = project_procedural_constraint(c);
    assert!(!b.is_empty(), "constraint {} must project a block", c.iri);
    b
}

#[test]
fn every_block_is_a_sparql_constraint_nodeshape_carrying_formalizes() {
    let c = guarded(
        "https://ex/c2",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    let b = block(&c);
    assert!(b.contains("a sh:NodeShape"), "{b}");
    assert!(b.contains("a sh:SPARQLConstraint"), "{b}");
    assert!(b.contains("sh:severity sh:Violation"), "{b}");
    // Self-identifies its canon: no explicit formalizes → the target class term.
    assert!(b.contains("logic:formalizes <https://ex/Widget>"), "{b}");
    assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
    assert!(b.contains("SELECT $this WHERE"), "{b}");
}

#[test]
fn explicit_formalizes_overrides_the_target_term() {
    let c = guarded(
        "https://ex/cF",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    )
    .with_formalizes("https://ex/gmeow/SomeAxiom")
    .unwrap();
    assert!(
        block(&c).contains("logic:formalizes <https://ex/gmeow/SomeAxiom>"),
        "{}",
        block(&c)
    );
}

#[test]
fn procedural_constraint_projects_failure_class_metadata() {
    let c = guarded(
        "https://ex/cFailure",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    )
    .with_failure_class("https://ex/Failure")
    .unwrap();
    assert!(
        block(&c).contains("gmeow:enforcesFailureClass <https://ex/Failure>"),
        "{}",
        block(&c)
    );
}

#[test]
fn p1_choice_group_xor_lowers_to_a_union_under_not_exists() {
    // φ = (∃A ∧ ¬∃B) ∨ (¬∃A ∧ ∃B); ¬φ (both-or-neither) = NOT EXISTS { left UNION right }.
    let ea = || exists("a", atom("https://ex/hasA", tvar("this"), tvar("a")));
    let eb = || exists("b", atom("https://ex/hasB", tvar("this"), tvar("b")));
    let left = Formula::And(vec![ea(), Formula::Not(Box::new(eb()))]);
    let right = Formula::And(vec![Formula::Not(Box::new(ea())), eb()]);
    let c = guarded("https://ex/c1", Formula::Or(vec![left, right]));
    let b = block(&c);
    assert!(b.contains("FILTER NOT EXISTS"), "{b}");
    assert!(b.contains("UNION"), "{b}");
    assert!(
        b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
        "{b}"
    );
}

/// A conjunction of PROHIBITIONS over MIXED argument positions lowers to one UNION arm
/// per forbidden position, each arm binding `$this`.
///
/// The shape a "never occupies any authority position" law takes:
/// `∀this. C(this) → ¬∃p. bars(p, this) ∧ ¬∃q. proves(this, q) ∧ ¬∃r. decides(r, this)`.
/// Its negation is a disjunction, and each disjunct is a bare positive triple that binds
/// the focus in a DIFFERENT slot — twice as the object, once as the subject. That mix is
/// the reason to pin it: an arm that lost its `$this` binding would select every node in
/// the graph rather than the ones the guard admits, so the law would condemn the corpus.
#[test]
fn a_conjunction_of_prohibitions_lowers_to_one_scoped_union_arm_per_position() {
    let bars = Formula::Not(Box::new(exists(
        "p",
        atom("https://ex/authorizes", tvar("p"), tvar("this")),
    )));
    let proves = Formula::Not(Box::new(exists(
        "q",
        atom("https://ex/establishes", tvar("this"), tvar("q")),
    )));
    let decides = Formula::Not(Box::new(exists(
        "r",
        atom("https://ex/decidedBy", tvar("r"), tvar("this")),
    )));
    let c = guarded(
        "https://ex/cNeverAuthority",
        Formula::And(vec![bars, proves, decides]),
    );
    let b = block(&c);
    assert_eq!(
        b.matches(" UNION ").count(),
        2,
        "three forbidden positions join as two UNION separators; block was: {b}"
    );
    for (pred, subj, obj) in [
        ("https://ex/authorizes", "?p", "$this"),
        ("https://ex/establishes", "$this", "?q"),
        ("https://ex/decidedBy", "?r", "$this"),
    ] {
        assert!(
            b.contains(&format!("{{ {subj} <{pred}> {obj} . }}")),
            "each position must be its own arm binding $this in the slot the law names \
                 ({subj} <{pred}> {obj}); block was: {b}"
        );
    }
    assert!(
        !b.contains("FILTER NOT EXISTS"),
        "every arm binds the focus through a positive triple, so none may degrade to an \
             unscoped FILTER; block was: {b}"
    );
}

#[test]
fn p2_guarded_implication_lowers_to_filter_not_exists_companion() {
    let c = guarded(
        "https://ex/c2",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    assert!(
        block(&c).contains("FILTER NOT EXISTS { $this <https://ex/companion> ?c . }"),
        "{}",
        block(&c)
    );
}

#[test]
fn p3_disjunctive_requiredness_lowers_to_not_exists_union() {
    // φ = ∃A ∨ ∃B; ¬φ = NOT EXISTS { {A} UNION {B} }.
    let c = guarded(
        "https://ex/c3",
        Formula::Or(vec![
            exists("a", atom("https://ex/hasA", tvar("this"), tvar("a"))),
            exists("b", atom("https://ex/hasB", tvar("this"), tvar("b"))),
        ]),
    );
    let b = block(&c);
    assert!(b.contains("FILTER NOT EXISTS {"), "{b}");
    assert!(b.contains("UNION"), "{b}");
}

#[test]
fn p4_path_value_type_membership_lowers_to_a_path_and_not_exists_type() {
    // φ = ∀v. part(this,v) → Part(v); ¬φ = this part ?v . NOT EXISTS { ?v a Part }.
    let c = guarded(
        "https://ex/c4",
        forall(
            "v",
            Formula::Implies(
                Box::new(atom("https://ex/part", tvar("this"), tvar("v"))),
                Box::new(atom(RDF_TYPE, tvar("v"), tiri("https://ex/Part"))),
            ),
        ),
    );
    let b = block(&c);
    assert!(b.contains("$this <https://ex/part> ?v ."), "{b}");
    assert!(
        b.contains(
            "FILTER NOT EXISTS { ?v a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* \
                 <https://ex/Part> . }"
        ),
        "{b}"
    );
}

#[test]
fn p5_cross_node_co_occurrence_lowers_to_a_path_and_nested_not_exists() {
    // φ = ∀o. linked(this,o) → ∃m. marker(o,m); ¬φ = this linked ?o . NOT EXISTS { ?o marker ?m }.
    let c = guarded(
        "https://ex/c5",
        forall(
            "o",
            Formula::Implies(
                Box::new(atom("https://ex/linked", tvar("this"), tvar("o"))),
                Box::new(exists("m", atom("https://ex/marker", tvar("o"), tvar("m")))),
            ),
        ),
    );
    let b = block(&c);
    assert!(b.contains("$this <https://ex/linked> ?o ."), "{b}");
    assert!(
        b.contains("FILTER NOT EXISTS { ?o <https://ex/marker> ?m . }"),
        "{b}"
    );
}

#[test]
fn p7_forbidden_pattern_lowers_to_a_positive_witness_triple() {
    // φ = ¬∃b. forbidden(this,b); ¬φ = ∃b. forbidden(this,b) = $this <forbidden> ?b .
    let c = guarded(
        "https://ex/c7",
        Formula::Not(Box::new(exists(
            "b",
            atom("https://ex/forbidden", tvar("this"), tvar("b")),
        ))),
    );
    assert!(
        block(&c).contains("$this <https://ex/forbidden> ?b ."),
        "{}",
        block(&c)
    );
}

#[test]
fn inverse_atom_places_the_focus_in_object_position() {
    // An inverse occurrence `rel(?v, $this)` (the focus in ARGUMENT-1 / object position)
    // lowers with `$this` as the triple object — the grounding "grounded BY a separate
    // observation" pattern (`observationResult(?obs, $this)`) needs no property-path term,
    // only the honest argument order.
    let c = guarded(
        "https://ex/cInv",
        exists(
            "obs",
            atom("https://ex/observationResult", tvar("obs"), tvar("this")),
        ),
    );
    assert!(
        block(&c).contains("FILTER NOT EXISTS { ?obs <https://ex/observationResult> $this . }"),
        "{}",
        block(&c)
    );
}

#[test]
fn term_distinct_with_a_literal_rhs_lowers_to_an_inequality_filter() {
    // A forbidden existential whose body pins a bound var to differ from a fixed literal:
    // φ = ¬∃q. (sigNeg(this,q) ∧ termDistinct(q, "0"^^integer));
    // ¬φ = ∃q. sigNeg(this,q) ∧ FILTER(?q != 0) — the metric-signature "q = 0" invariant.
    let xsd_int = "http://www.w3.org/2001/XMLSchema#integer";
    let body = Formula::And(vec![
        atom("https://ex/signatureNegative", tvar("this"), tvar("q")),
        Formula::atom(
            tiri(LOGIC_TERM_DISTINCT),
            vec![
                tvar("q"),
                Term::literal("0".to_owned(), Some(xsd_int.to_owned())).unwrap(),
            ],
        )
        .unwrap(),
    ]);
    let c = guarded("https://ex/cLit", Formula::Not(Box::new(exists("q", body))));
    let b = block(&c);
    assert!(
        b.contains("$this <https://ex/signatureNegative> ?q ."),
        "{b}"
    );
    assert!(
        b.contains(&format!("FILTER ( ?q != '0'^^<{xsd_int}> )")),
        "{b}"
    );
}

#[test]
fn term_in_forbidden_membership_lowers_to_an_in_filter() {
    // φ = ¬∃v. (leak(this,v) ∧ termIn(v, {ex:a, ex:b})); ¬φ = ∃v. leak(this,v) ∧ ?v IN (…).
    let body = Formula::And(vec![
        atom("https://ex/leak", tvar("this"), tvar("v")),
        Formula::atom(
            tiri(LOGIC_TERM_IN),
            vec![tvar("v"), tiri("https://ex/a"), tiri("https://ex/b")],
        )
        .unwrap(),
    ]);
    let c = guarded("https://ex/cIn", Formula::Not(Box::new(exists("v", body))));
    let b = block(&c);
    assert!(b.contains("$this <https://ex/leak> ?v ."), "{b}");
    assert!(
        b.contains("FILTER ( ?v IN (<https://ex/a>, <https://ex/b>) )"),
        "{b}"
    );
}

#[test]
fn term_in_required_membership_lowers_to_a_not_in_filter() {
    // φ = ∀v. tag(this,v) → termIn(v, {ex:a}); a value outside the set violates → ?v NOT IN (…).
    let inner = forall(
        "v",
        Formula::Implies(
            Box::new(atom("https://ex/tag", tvar("this"), tvar("v"))),
            Box::new(
                Formula::atom(tiri(LOGIC_TERM_IN), vec![tvar("v"), tiri("https://ex/a")]).unwrap(),
            ),
        ),
    );
    let c = guarded("https://ex/cReq", inner);
    let b = block(&c);
    assert!(b.contains("$this <https://ex/tag> ?v ."), "{b}");
    assert!(b.contains("FILTER ( ?v NOT IN (<https://ex/a>) )"), "{b}");
}

#[test]
fn term_str_starts_and_regex_lower_to_string_filters() {
    let lit = |s: &str| Term::literal(s.to_owned(), None).unwrap();
    // Forbidden prefix: ¬∃v. code(this,v) ∧ termStrStarts(v,"gmn:") → FILTER STRSTARTS.
    let body = Formula::And(vec![
        atom("https://ex/code", tvar("this"), tvar("v")),
        Formula::atom(tiri(LOGIC_TERM_STR_STARTS), vec![tvar("v"), lit("gmn:")]).unwrap(),
    ]);
    let c = guarded(
        "https://ex/cPrefix",
        Formula::Not(Box::new(exists("v", body))),
    );
    let b = block(&c);
    assert!(b.contains("FILTER ( STRSTARTS(STR(?v), 'gmn:') )"), "{b}");

    // Required regex: ∀v. code(this,v) → termRegex(v,"^[a-z]+$") → violation !REGEX.
    let inner = forall(
        "v",
        Formula::Implies(
            Box::new(atom("https://ex/code", tvar("this"), tvar("v"))),
            Box::new(
                Formula::atom(tiri(LOGIC_TERM_REGEX), vec![tvar("v"), lit("^[a-z]+$")]).unwrap(),
            ),
        ),
    );
    let c = guarded("https://ex/cRegex", inner);
    let b = block(&c);
    assert!(b.contains("FILTER ( !REGEX(STR(?v), '^[a-z]+$') )"), "{b}");
}

#[test]
fn constraint_free_program_yields_a_byte_stable_header_only_doc() {
    let a = LogicProgram::new(vec![], vec![], vec![], None);
    let b = LogicProgram::new(vec![], vec![], vec![], None);
    let da = project_procedural_constraints(&a);
    let db = project_procedural_constraints(&b);
    assert_eq!(da, db, "a constraint-free program must be byte-stable");
    assert!(da.starts_with("# GENERATED"), "{da}");
    assert!(!da.contains("a sh:NodeShape"), "no shapes expected: {da}");
    assert!(da.contains("@prefix sh:"), "{da}");
}

#[test]
fn whole_program_doc_is_iri_sorted_and_header_carrying() {
    let c_b = guarded(
        "https://ex/zeta",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    let c_a = guarded(
        "https://ex/alpha",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c_b, c_a]);
    let doc = project_procedural_constraints(&prog);
    assert!(doc.starts_with("# GENERATED"), "{doc}");
    let ai = doc
        .find("AlphaProceduralConstraintShape")
        .expect("alpha shape");
    let zi = doc
        .find("ZetaProceduralConstraintShape")
        .expect("zeta shape");
    assert!(ai < zi, "shapes must be emitted in IRI-sorted order");
}

#[test]
fn unsupported_integrity_is_carried_as_flagged_residue_not_emitted() {
    // A biconditional consequent exceeds the projectable NNF fragment.
    let c = guarded(
        "https://ex/cIff",
        Formula::Iff(
            Box::new(atom("https://ex/p", tvar("this"), tiri("https://ex/x"))),
            Box::new(atom("https://ex/q", tvar("this"), tiri("https://ex/y"))),
        ),
    );
    assert!(
        project_procedural_constraint(&c).is_empty(),
        "an unsupported constraint must not emit a block"
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let residue = procedural_constraint_residue(&prog);
    assert_eq!(residue.len(), 1, "{residue:?}");
    assert!(
        residue[0].contains("exceeds the range-restricted guarded SPARQL constraint fragment"),
        "{residue:?}"
    );
    // The whole-program doc drops it (header-only) — carried in the ledger, never emitted.
    assert!(
        !project_procedural_constraints(&prog).contains("a sh:NodeShape"),
        "the unsupported constraint must not reach the document"
    );
}

#[test]
fn coexisting_aggregate_satellites_hard_fail_at_projection() {
    // A constraint carrying BOTH a join_aggregate and an aggregate satellite would otherwise
    // have the projection dispatch's priority order silently drop the lower-priority
    // `aggregate` satellite. It must instead hard-fail — carried as flagged residue,
    // never silently projected with one satellite dropped.
    use crate::ir::{AggregateComparator, JoinLeg};

    let leg = JoinLeg::new(
        None,
        "https://ex/incidenceCoface",
        "https://ex/incidenceFace",
        "https://ex/incidenceSign",
    )
    .unwrap();
    let ja = JoinAggregate::new(
        "SUM",
        vec![leg.clone(), leg],
        AggregateComparator::Eq,
        "0",
        None,
    )
    .unwrap();
    let agg = AggregateComparison::new(
        "COUNT",
        false,
        "https://ex/part",
        AggregateComparator::Le,
        AggregateRhs::Literal {
            lexical: "10".into(),
            datatype: None,
        },
    )
    .unwrap();
    let c = guarded(
        "https://ex/cDualSatellite",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    )
    .with_join_aggregate(ja)
    .with_aggregate(agg);

    assert!(
        c.ensure_single_satellite().is_err(),
        "a constraint carrying two aggregate satellites must fail the guard directly"
    );
    assert!(
        project_procedural_constraint(&c).is_empty(),
        "a dual-satellite constraint must not emit a block"
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let residue = procedural_constraint_residue(&prog);
    assert_eq!(residue.len(), 1, "{residue:?}");
    assert!(
        residue[0].contains("aggregate") && residue[0].contains("join_aggregate"),
        "the residue must name the coexisting satellites: {residue:?}"
    );
}

#[test]
fn every_constraint_gets_a_blanket_shex_unsupported_note() {
    let c = guarded(
        "https://ex/c2",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let shex = procedural_constraint_shex_residue(&prog);
    assert_eq!(shex.len(), 1, "{shex:?}");
    assert!(
        shex[0].contains("ShEx has no SPARQL-constraint form"),
        "{shex:?}"
    );
}

#[test]
fn native_engine_flags_planted_violations_and_passes_clean_data() {
    use purrdf::shapes::engine::validate_graphs;

    // Two constraints over ex:Widget: (A) every widget must have a companion
    // (guarded implication), (B) a widget must not carry a `forbidden` edge.
    let a = guarded(
        "https://ex/mustHaveCompanion",
        exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
    );
    let b = guarded(
        "https://ex/noForbidden",
        Formula::Not(Box::new(exists(
            "b",
            atom("https://ex/forbidden", tvar("this"), tvar("b")),
        ))),
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![a, b]);
    let shapes_ttl = project_procedural_constraints(&prog);

    // N-Triples data: one clean widget, one violating A (no companion), one violating B.
    let data = "\
<https://ex/goodW> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/goodW> <https://ex/companion> <https://ex/c0> .\n\
<https://ex/badNoCompanion> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/badForbidden> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/badForbidden> <https://ex/companion> <https://ex/c1> .\n\
<https://ex/badForbidden> <https://ex/forbidden> <https://ex/f0> .\n";

    let report = validate_graphs(data, &shapes_ttl, None).expect("validate");
    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    for bad in ["badNoCompanion", "badForbidden"] {
        assert!(
            flagged.iter().any(|f| f.contains(bad)),
            "the {bad} violation must be flagged; flagged: {flagged:?}"
        );
    }
    assert!(
        !flagged.iter().any(|f| f.contains("goodW")),
        "the clean widget must NOT be flagged; flagged: {flagged:?}"
    );
}

// ── Constraint-sugar expansion (P1–P5, P7) + aggregate (P6) projection ────────

/// The prefix header for the sugar fixtures.
const SUGAR_PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <https://ex/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

/// Parse a sugar fixture (asserting no MALFORMED_CONSTRAINT) and project its single constraint
/// to a `sh:SPARQLConstraint` block.
fn project_sugar(ttl: &str) -> String {
    let src = format!("{SUGAR_PREFIXES}{ttl}");
    let (program, diags) =
        crate::frontend::parse_logic_str(&src, None).expect("sugar fixture must parse");
    assert!(
        !diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
        "unexpected MALFORMED_CONSTRAINT diagnostics: {diags:?}"
    );
    assert_eq!(
        program.constraints.len(),
        1,
        "expected exactly one constraint"
    );
    let block = project_procedural_constraint(&program.constraints[0]);
    assert!(
        !block.is_empty(),
        "the sugar constraint must project a block"
    );
    block
}

#[test]
fn value_set_membership_required_sugar_projects_a_not_in_filter() {
    let b = project_sugar(
        "ex:vsr a logic:ValueSetMembershipConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:register ;\n\
             logic:memberValue ex:alpha , ex:beta ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
    assert!(b.contains("$this <https://ex/register> ?v ."), "{b}");
    assert!(
        b.contains("FILTER ( ?v NOT IN (<https://ex/alpha>, <https://ex/beta>) )"),
        "{b}"
    );
}

#[test]
fn value_set_membership_forbidden_sugar_projects_an_in_filter() {
    let b = project_sugar(
        "ex:vsf a logic:ValueSetMembershipConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:leaks ;\n\
             logic:membershipMode \"forbidden\" ;\n\
             logic:memberValue ex:secret ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(b.contains("$this <https://ex/leaks> ?v ."), "{b}");
    assert!(b.contains("FILTER ( ?v IN (<https://ex/secret>) )"), "{b}");
}

#[test]
fn string_pattern_sugar_projects_regex_and_prefix_filters() {
    let req = project_sugar(
        "ex:spr a logic:StringPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:code ;\n\
             logic:stringOp \"regexRequired\" ;\n\
             logic:stringPattern \"^[A-Z]+$\" ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(
        req.contains("FILTER ( !REGEX(STR(?v), '^[A-Z]+$') )"),
        "{req}"
    );
    let forb = project_sugar(
        "ex:spf a logic:StringPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:label ;\n\
             logic:stringOp \"prefixForbidden\" ;\n\
             logic:stringPattern \"tmp:\" ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(
        forb.contains("FILTER ( STRSTARTS(STR(?v), 'tmp:') )"),
        "{forb}"
    );
}

#[test]
fn p1_choice_group_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c1 a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
    assert!(b.contains("logic:formalizes <https://ex/Widget>"), "{b}");
    // exactly-one → a UNION of per-predicate branches under FILTER NOT EXISTS.
    assert!(b.contains("UNION"), "{b}");
    assert!(
        b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
        "{b}"
    );
}

#[test]
fn sugar_failure_class_dedupes_identical_values() {
    let block = project_sugar(
        "ex:fc a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget ;\n\
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:Failure, ex:Failure .",
    );
    assert_eq!(
        block.matches("gmeow:enforcesFailureClass").count(),
        1,
        "{block}"
    );
}

#[test]
fn sugar_failure_class_rejects_distinct_values() {
    let src = format!(
        "{SUGAR_PREFIXES}ex:fc_bad a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget ;\n\
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:FailureA, ex:FailureB ."
    );
    let (program, diagnostics) =
        crate::frontend::parse_logic_str(&src, None).expect("fixture parses");
    assert!(program.constraints.is_empty());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MALFORMED_CONSTRAINT" && diagnostic.message.contains("distinct")
    }));
}

#[test]
fn p1_at_most_one_sugar_projects() {
    let b = project_sugar(
        "ex:c1b a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"at-most-one\" ;\n\
             logic:formalizes ex:Widget .",
    );
    // at-most-one over two predicates → the violation is the pair present together.
    assert!(
        b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
        "{b}"
    );
    assert!(b.contains("SELECT $this WHERE"), "{b}");
}

#[test]
fn p1_at_least_one_sugar_projects_a_missing_all_alternatives_guard() {
    let b = project_sugar(
        "ex:c1c a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"at-least-one\" ;\n\
             logic:formalizes ex:Widget .",
    );
    // at-least-one → the violation is EVERY alternative absent: both predicates appear
    // under the negative (missing) side of the lowering.
    assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
    assert!(
        b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
        "{b}"
    );
    assert!(b.contains("NOT EXISTS"), "{b}");
}

#[test]
fn p2_guarded_implication_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c2 a logic:GuardedImplicationConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:trigger ex:isActive ;\n\
             logic:requires ex:companion ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
    // Guard: the trigger predicate is a positive triple; the missing companion is the violation.
    assert!(b.contains("<https://ex/isActive>"), "{b}");
    assert!(
        b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/companion>"),
        "{b}"
    );
}

#[test]
fn p2_guarded_implication_with_trigger_value_projects() {
    let b = project_sugar(
        "ex:c2v a logic:GuardedImplicationConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:trigger ex:kind ;\n\
             logic:triggerValue ex:Special ;\n\
             logic:requires ex:companion ;\n\
             logic:formalizes ex:Widget .",
    );
    // The pinned trigger value appears in the guard triple's object position.
    assert!(b.contains("<https://ex/kind> <https://ex/Special>"), "{b}");
}

#[test]
fn predicate_presence_guarded_implication_targets_subjects_of_the_trigger() {
    // With NO logic:onClass the trigger predicate IS the range restriction: the guard is the
    // bare trigger atom, so the target derives as sh:targetSubjectsOf trigger (the grounding
    // "subjects of a claim predicate must carry a grounding field" pattern).
    let b = project_sugar(
        "ex:pp a logic:GuardedImplicationConstraint ;\n\
             logic:trigger ex:aboutReading ;\n\
             logic:requires ex:vantage ;\n\
             logic:formalizes ex:SomeShape .",
    );
    assert!(
        b.contains("sh:targetSubjectsOf <https://ex/aboutReading>"),
        "{b}"
    );
    assert!(b.contains("logic:formalizes <https://ex/SomeShape>"), "{b}");
    assert!(b.contains("$this <https://ex/aboutReading> ?t ."), "{b}");
    assert!(
        b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/vantage>"),
        "{b}"
    );
}

#[test]
fn p3_disjunctive_requiredness_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c3 a logic:DisjunctiveRequirednessConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:anyOf ex:hasA , ex:hasB ;\n\
             logic:formalizes ex:Widget .",
    );
    // ≥1 required → violation is NONE present: FILTER NOT EXISTS { {hasA} UNION {hasB} }.
    assert!(b.contains("FILTER NOT EXISTS"), "{b}");
    assert!(b.contains("UNION"), "{b}");
    assert!(
        b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
        "{b}"
    );
}

#[test]
fn p4_path_value_type_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c4 a logic:PathValueTypeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:part ;\n\
             logic:valueClass ex:Part ;\n\
             logic:formalizes ex:part .",
    );
    assert!(b.contains("logic:formalizes <https://ex/part>"), "{b}");
    // Violation: ∃v. part(this,v) ∧ ¬ Part(v).
    assert!(b.contains("<https://ex/part>"), "{b}");
    assert!(
        b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/Part>"),
        "{b}"
    );
}

#[test]
fn p4_path_value_fixed_predicate_value_sugar_expands_and_projects() {
    // The fixed predicate=value variant: every value on the path must carry Q = o.
    // Violation: ∃v. inducedByForm(this,v) ∧ ¬ definiteness(v, positiveDefinite).
    let b = project_sugar(
        "ex:c4f a logic:PathValueTypeConstraint ;\n\
             logic:onClass ex:Norm ;\n\
             logic:valuePath ex:inducedByForm ;\n\
             logic:valuePredicate ex:definiteness ;\n\
             logic:valueObject ex:positiveDefinite ;\n\
             logic:formalizes ex:Norm .",
    );
    assert!(b.contains("$this <https://ex/inducedByForm> ?v ."), "{b}");
    assert!(
        b.contains(
            "FILTER NOT EXISTS { ?v <https://ex/definiteness> <https://ex/positiveDefinite> . }"
        ),
        "{b}"
    );
}

#[test]
fn p5_cross_node_co_occur_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c5 a logic:CrossNodeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:roleA ex:left ;\n\
             logic:roleB ex:right ;\n\
             logic:crossMode \"co-occur\" ;\n\
             logic:formalizes ex:Widget .",
    );
    assert!(
        b.contains("<https://ex/left>") && b.contains("<https://ex/right>"),
        "{b}"
    );
    assert!(b.contains("UNION"), "{b}");
}

#[test]
fn p5_cross_node_differ_sugar_projects_an_equality_filter() {
    let b = project_sugar(
        "ex:c5d a logic:CrossNodeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:roleA ex:left ;\n\
             logic:roleB ex:right ;\n\
             logic:crossMode \"differ\" ;\n\
             logic:formalizes ex:Widget .",
    );
    // Violation of "roles must differ" = the two roles bind an EQUAL value.
    assert!(b.contains("FILTER ( ?a = ?b )"), "{b}");
    assert!(
        b.contains("<https://ex/left>") && b.contains("<https://ex/right>"),
        "{b}"
    );
}

#[test]
fn p7_forbidden_pattern_sugar_expands_and_projects() {
    let b = project_sugar(
        "ex:c7 a logic:ForbiddenPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:forbiddenPredicate ex:forbidden ;\n\
             logic:formalizes ex:Widget .",
    );
    // A forbidden-pattern violation is the PRESENCE of the pattern: the SHACL
    // sh:select returns the focus nodes that HAVE the forbidden predicate (a
    // positive BGP match), not a FILTER NOT EXISTS over its absence.
    assert!(
        b.contains("<https://ex/forbidden> ?") && !b.contains("NOT EXISTS"),
        "{b}"
    );
}

#[test]
fn p6_aggregate_count_distinct_property_rhs_projects_group_by_having() {
    let b = project_sugar(
        "ex:agg1 a logic:AggregateConstraint ;\n\
             logic:onClass ex:Dimensional ;\n\
             logic:aggFunction \"COUNT\" ;\n\
             logic:aggDistinct true ;\n\
             logic:aggPath ex:hasAxis ;\n\
             logic:aggComparator \"=\" ;\n\
             logic:aggCompareTo ex:dimensionCount ;\n\
             logic:formalizes ex:Dimensional .",
    );
    assert!(b.contains("sh:targetClass <https://ex/Dimensional>"), "{b}");
    assert!(b.contains("COUNT(DISTINCT ?value)"), "{b}");
    // A property RHS binds ?rhs and joins it into the GROUP BY.
    assert!(b.contains("$this <https://ex/dimensionCount> ?rhs"), "{b}");
    assert!(b.contains("GROUP BY $this ?rhs"), "{b}");
    // The invariant is `=`, so the violation-selecting HAVING uses the negation `!=`.
    assert!(
        b.contains("HAVING ( COUNT(DISTINCT ?value) != ?rhs )"),
        "{b}"
    );
}

#[test]
fn aggregate_balance_sugar_projects_partitioned_group_by_having() {
    let b = project_sugar(
        "ex:bal a logic:AggregateBalanceConstraint ;\n\
             logic:onClass ex:JournalEntry ;\n\
             logic:balancePostingPredicate ex:posting ;\n\
             logic:balancePartitionPredicate ex:direction ;\n\
             logic:balanceDebitValue ex:debit ;\n\
             logic:balanceCreditValue ex:credit ;\n\
             logic:balanceAmountNodePredicate ex:amount ;\n\
             logic:balanceValuePredicate ex:value ;\n\
             logic:balanceGroupPredicate ex:currency ;\n\
             logic:formalizes ex:JournalEntry .",
    );
    assert!(
        b.contains("sh:targetClass <https://ex/JournalEntry>"),
        "{b}"
    );
    assert!(
        b.contains(
            "SELECT $this ?group (SUM(?debitVal) AS ?sumDebits) (SUM(?creditVal) AS ?sumCredits)"
        ),
        "{b}"
    );
    assert!(b.contains("$this <https://ex/posting> ?posting ."), "{b}");
    assert!(
        b.contains("?amount <https://ex/value> ?val ; <https://ex/currency> ?group ."),
        "{b}"
    );
    assert!(
        b.contains("BIND(IF(?direction = <https://ex/debit>, ?val, 0) AS ?debitVal)"),
        "{b}"
    );
    assert!(b.contains("GROUP BY $this ?group"), "{b}");
    assert!(b.contains("FILTER(?sumDebits != ?sumCredits)"), "{b}");
}

#[test]
fn term_lang_matches_and_has_lang_project_language_tag_filters() {
    // A hand-authored language-tag gate: a sparqlTarget-focused constraint whose body forbids a
    // gmeow:-namespaced tagged literal whose LANGUAGE TAG is not under the x-gmeow- prefix.
    let src = format!(
        "{SUGAR_PREFIXES}\
ex:ilt a logic:Constraint ;\n\
  logic:formalizes ex:LangShape ;\n\
  logic:severity \"Violation\" ;\n\
  logic:integrity ex:iltForall .\n\
ex:iltForall a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"this\" ] ; logic:forall ex:iltImpl .\n\
ex:iltImpl a logic:Formula ; logic:antecedent ex:iltTarget ; logic:consequent ex:iltOk .\n\
ex:iltTarget a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/sparqlTarget> ;\n\
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] , [ logic:termIndex 1 ; logic:termLiteral \"SELECT DISTINCT ?this WHERE {{ ?this ?p ?value . FILTER(isLiteral(?value)) }}\" ] .\n\
ex:iltOk a logic:Formula ; logic:not ex:iltBad .\n\
ex:iltBad a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"p\" ] , [ logic:termIndex 1 ; logic:termVariable \"value\" ] ; logic:exists ex:iltBody .\n\
ex:iltBody a logic:Formula ; logic:and ex:iltLink , ex:iltLit , ex:iltHasLang , ex:iltNotOk .\n\
ex:iltLink a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/linkVia> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] , [ logic:termIndex 1 ; logic:termVariable \"p\" ] , [ logic:termIndex 2 ; logic:termVariable \"value\" ] .\n\
ex:iltLit a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termIsLiteral> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] .\n\
ex:iltHasLang a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termHasLang> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] .\n\
ex:iltNotOk a logic:Formula ; logic:not ex:iltMatch .\n\
ex:iltMatch a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termLangMatches> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] , [ logic:termIndex 1 ; logic:termLiteral \"^x-gmeow-[a-z0-9-]+$\" ] .\n"
    );
    let (program, diags) =
        crate::frontend::parse_logic_str(&src, None).expect("fixture must parse");
    assert!(
        !diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
        "unexpected diagnostics: {diags:?}"
    );
    assert_eq!(program.constraints.len(), 1);
    let b = project_procedural_constraint(&program.constraints[0]);
    assert!(!b.is_empty(), "must project a block: {b}");
    // The tagged-literal presence check and the case-insensitive negated language-tag regex.
    assert!(b.contains(r#"LANG(?value) != """#), "{b}");
    assert!(
        b.contains("!REGEX(LANG(?value), '^x-gmeow-[a-z0-9-]+$', 'i')"),
        "{b}"
    );
    // The raw sparqlTarget is carried verbatim as the sh:target select.
    assert!(b.contains("a sh:SPARQLTarget"), "{b}");
}

#[test]
fn p6_aggregate_sum_literal_rhs_projects_group_by_having() {
    let b = project_sugar(
        "ex:agg2 a logic:AggregateConstraint ;\n\
             logic:onClass ex:Portfolio ;\n\
             logic:aggFunction \"SUM\" ;\n\
             logic:aggPath ex:weight ;\n\
             logic:aggComparator \"=\" ;\n\
             logic:aggCompareTo \"1\"^^xsd:integer ;\n\
             logic:formalizes ex:Portfolio .",
    );
    assert!(b.contains("SUM(?value)"), "{b}");
    assert!(b.contains("$this <https://ex/weight> ?value"), "{b}");
    assert!(b.contains("GROUP BY $this"), "{b}");
    // The literal RHS keeps its datatype in the HAVING comparison.
    assert!(
        b.contains("HAVING ( SUM(?value) != '1'^^<http://www.w3.org/2001/XMLSchema#integer> )"),
        "{b}"
    );
}

#[test]
fn p6_aggregate_count_distinct_validates_against_a_graph() {
    // End-to-end: the generated GROUP BY/HAVING SELECT must be valid SPARQL and flag the right
    // focus nodes. `good` has 2 distinct axes and dimensionCount 2 (conforms); `bad` has 2
    // distinct axes and dimensionCount 3 (violates COUNT(DISTINCT) = dimensionCount).
    use purrdf::shapes::engine::validate_graphs;
    let src = format!(
        "{SUGAR_PREFIXES}\
ex:aggv a logic:AggregateConstraint ;\n\
  logic:onClass ex:Dimensional ;\n\
  logic:aggFunction \"COUNT\" ;\n\
  logic:aggDistinct true ;\n\
  logic:aggPath ex:hasAxis ;\n\
  logic:aggComparator \"=\" ;\n\
  logic:aggCompareTo ex:dimensionCount ;\n\
  logic:formalizes ex:Dimensional ."
    );
    let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
    let shapes_ttl = project_procedural_constraints(&program);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let data = format!(
        "<https://ex/good> {ty} <https://ex/Dimensional> .\n\
<https://ex/good> <https://ex/hasAxis> <https://ex/x> .\n\
<https://ex/good> <https://ex/hasAxis> <https://ex/y> .\n\
<https://ex/good> <https://ex/dimensionCount> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> {ty} <https://ex/Dimensional> .\n\
<https://ex/bad> <https://ex/hasAxis> <https://ex/x> .\n\
<https://ex/bad> <https://ex/hasAxis> <https://ex/y> .\n\
<https://ex/bad> <https://ex/dimensionCount> \"3\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n"
    );
    let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    assert!(
        flagged.iter().any(|f| f.contains("bad")),
        "the count-mismatch node must be flagged; flagged: {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|f| f.contains("good")),
        "the conforming node must NOT be flagged; flagged: {flagged:?}"
    );
}

/// The two-leg boundary-square-zero (∂²=0) join-aggregate demonstrator: a coface hops via an
/// incidence record to an intermediate cell, then via a second incidence record to a far face,
/// and the SUM of the incidence-sign PRODUCT over the intermediate cells must be 0 per
/// (coface, far-face) group. Reused by the projection and end-to-end tests.
const JOIN_AGG_SUGAR: &str = "ex:boundarySquareZero a logic:JoinAggregateConstraint ;\n\
         logic:onClass ex:TopCell ;\n\
         logic:aggFunction \"SUM\" ;\n\
         logic:aggComparator \"=\" ;\n\
         logic:aggThreshold 0 ;\n\
         logic:joinPath (\n\
           [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
         ) ;\n\
         logic:formalizes ex:BoundaryOperator .";

#[test]
fn join_aggregate_multi_hop_projects_deterministic_group_by_having() {
    let b = project_sugar(JOIN_AGG_SUGAR);
    // The focus is the coface (the top cell); the join is anchored on $this.
    assert!(b.contains("sh:targetClass <https://ex/TopCell>"), "{b}");
    // Leg 1 anchors on the bound focus $this via the source predicate, then binds the
    // intermediate endpoint ?j1 and the leaf value ?v1 (index-friendly ordering).
    assert!(
            b.contains("?r1 <https://ex/incidenceCoface> $this . ?r1 <https://ex/incidenceFace> ?j1 . ?r1 <https://ex/incidenceSign> ?v1 . ?r1 a <https://ex/Incidence> ."),
            "leg 1 must anchor on $this and bind ?j1/?v1: {b}"
        );
    // Leg 2 re-binds the SHARED join variable ?j1 as its source (the multi-hop join), binds the
    // far endpoint ?j2, and the second leaf value ?v2.
    assert!(
            b.contains("?r2 <https://ex/incidenceCoface> ?j1 . ?r2 <https://ex/incidenceFace> ?j2 . ?r2 <https://ex/incidenceSign> ?v2 . ?r2 a <https://ex/Incidence> ."),
            "leg 2 must re-bind ?j1 and bind ?j2/?v2: {b}"
        );
    // The group key is (focus, far endpoint); the aggregate is the SUM of the sign PRODUCT.
    assert!(b.contains("SELECT $this ?j2 WHERE"), "{b}");
    assert!(b.contains("GROUP BY $this ?j2"), "{b}");
    // Invariant is `=` 0, so the violation-selecting HAVING negates it to `!=`.
    assert!(
        b.contains("HAVING ( SUM(?v1 * ?v2) != '0'^^<http://www.w3.org/2001/XMLSchema#integer> )"),
        "{b}"
    );
    // No cartesian product over cells: every triple pattern is a record-anchored join, so no
    // FILTER-cross or bare cell×cell pattern is emitted.
    assert!(!b.contains("NOT EXISTS"), "{b}");
}

#[test]
fn join_aggregate_projection_is_byte_deterministic() {
    // Two independent parses of the same source produce byte-identical SPARQL (stable variable
    // names + clause order), so regeneration is stable.
    let a = project_sugar(JOIN_AGG_SUGAR);
    let b = project_sugar(JOIN_AGG_SUGAR);
    assert_eq!(a, b, "join-aggregate projection must be byte-deterministic");
}

#[test]
fn join_aggregate_boundary_square_zero_validates_against_a_graph() {
    // End-to-end: the generated multi-hop-join GROUP BY/HAVING SELECT must be valid SPARQL and
    // flag exactly the top cell whose ∂² ≠ 0. `good` is a triangle whose signed incidences make
    // every (coface, far-vertex) group sum to 0; `bad` has one flipped sign so a group sums to
    // -2 ≠ 0.
    use purrdf::shapes::engine::validate_graphs;
    let src = format!("{SUGAR_PREFIXES}{JOIN_AGG_SUGAR}");
    let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
    let shapes_ttl = project_procedural_constraints(&program);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let int = "<http://www.w3.org/2001/XMLSchema#integer>";
    // A signed incidence record `coface → face` with sign `s`, as four N-Triples over a fresh
    // labeled blank node (the data graph is parsed as N-Triples, so no `[]`/`;` sugar).
    let mut rec = 0u32;
    let mut inc = |coface: &str, face: &str, s: i32| {
        rec += 1;
        let r = format!("_:rec{rec}");
        format!(
            "{r} {ty} <https://ex/Incidence> .\n\
                 {r} <https://ex/incidenceCoface> <{coface}> .\n\
                 {r} <https://ex/incidenceFace> <{face}> .\n\
                 {r} <https://ex/incidenceSign> \"{s}\"^^{int} .\n"
        )
    };
    let mut data = String::new();
    // GOOD triangle: T over edges a,b,c over vertices p,q,r; ∂² = 0 in every group.
    data.push_str(&format!("<https://ex/T> {ty} <https://ex/TopCell> .\n"));
    data.push_str(&inc("https://ex/T", "https://ex/a", 1));
    data.push_str(&inc("https://ex/T", "https://ex/b", 1));
    data.push_str(&inc("https://ex/T", "https://ex/c", 1));
    data.push_str(&inc("https://ex/a", "https://ex/p", -1));
    data.push_str(&inc("https://ex/a", "https://ex/q", 1));
    data.push_str(&inc("https://ex/b", "https://ex/q", -1));
    data.push_str(&inc("https://ex/b", "https://ex/r", 1));
    data.push_str(&inc("https://ex/c", "https://ex/r", -1));
    data.push_str(&inc("https://ex/c", "https://ex/p", 1));
    // BAD triangle: same shape but the c→p sign is flipped, so group (Tb, pb) sums to -2 ≠ 0.
    data.push_str(&format!("<https://ex/Tb> {ty} <https://ex/TopCell> .\n"));
    data.push_str(&inc("https://ex/Tb", "https://ex/ab", 1));
    data.push_str(&inc("https://ex/Tb", "https://ex/bb", 1));
    data.push_str(&inc("https://ex/Tb", "https://ex/cb", 1));
    data.push_str(&inc("https://ex/ab", "https://ex/pb", -1));
    data.push_str(&inc("https://ex/ab", "https://ex/qb", 1));
    data.push_str(&inc("https://ex/bb", "https://ex/qb", -1));
    data.push_str(&inc("https://ex/bb", "https://ex/rb", 1));
    data.push_str(&inc("https://ex/cb", "https://ex/rb", -1));
    data.push_str(&inc("https://ex/cb", "https://ex/pb", -1)); // flipped: should be +1
    let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    assert!(
        flagged.iter().any(|f| f.contains("/Tb")),
        "the ∂²≠0 top cell must be flagged; flagged: {flagged:?}"
    );
    assert!(
        !flagged
            .iter()
            .any(|f| f.contains("/T>") || f.ends_with("/T")),
        "the ∂²=0 top cell must NOT be flagged; flagged: {flagged:?}"
    );
}

#[test]
fn comparison_constraint_lowers_to_an_ordering_filter() {
    let b = project_sugar(
        "ex:cmp a logic:ComparisonConstraint ;\n\
             logic:onClass ex:ScoreScale ;\n\
             logic:leftPath ex:scaleMin ;\n\
             logic:rightPath ex:scaleMax ;\n\
             logic:compareOp \">=\" ;\n\
             logic:formalizes ex:ScoreScale .",
    );
    assert!(b.contains("sh:targetClass <https://ex/ScoreScale>"), "{b}");
    assert!(b.contains("$this <https://ex/scaleMin> ?l ."), "{b}");
    assert!(b.contains("$this <https://ex/scaleMax> ?r ."), "{b}");
    // The FORBIDDEN relation (min >= max) is the violation-selecting FILTER.
    assert!(b.contains("FILTER ( ?l >= ?r )"), "{b}");
    assert!(!b.contains("NOT EXISTS"), "{b}");
}

#[test]
fn path_node_kind_iri_lowers_to_a_negated_isiri_filter() {
    // With no onClass the value-path is the range restriction → sh:targetSubjectsOf.
    let b = project_sugar(
        "ex:nk a logic:PathNodeKindConstraint ;\n\
             logic:valuePath ex:hasAboutness ;\n\
             logic:nodeKind \"IRI\" ;\n\
             logic:formalizes ex:AboutnessTargetShape .",
    );
    assert!(
        b.contains("sh:targetSubjectsOf <https://ex/hasAboutness>"),
        "{b}"
    );
    assert!(b.contains("$this <https://ex/hasAboutness> ?v ."), "{b}");
    // The violation is a value that is NOT an IRI.
    assert!(b.contains("FILTER ( !isIRI(?v) )"), "{b}");
}

#[test]
fn path_node_kind_blank_or_iri_on_a_class_target() {
    let b = project_sugar(
        "ex:nk2 a logic:PathNodeKindConstraint ;\n\
             logic:onClass ex:SetBuilderExpression ;\n\
             logic:valuePath ex:memberCondition ;\n\
             logic:nodeKind \"BlankNodeOrIRI\" ;\n\
             logic:formalizes ex:SetBuilderExpression .",
    );
    assert!(
        b.contains("sh:targetClass <https://ex/SetBuilderExpression>"),
        "{b}"
    );
    assert!(
        b.contains("FILTER ( !( isIRI(?v) || isBlank(?v) ) )"),
        "{b}"
    );
}

#[test]
fn self_join_uniqueness_lowers_to_a_shared_value_self_join() {
    let b = project_sugar(
        "ex:sj a logic:SelfJoinUniquenessConstraint ;\n\
             logic:siblingPredicate ex:argumentSlot ;\n\
             logic:sharedPredicate ex:slotIndex ;\n\
             logic:formalizes ex:SlotIndexUniquenessShape .",
    );
    assert!(
        b.contains("sh:targetSubjectsOf <https://ex/argumentSlot>"),
        "{b}"
    );
    assert!(b.contains("$this <https://ex/argumentSlot> ?s1 ."), "{b}");
    assert!(b.contains("$this <https://ex/argumentSlot> ?s2 ."), "{b}");
    assert!(b.contains("?s1 <https://ex/slotIndex> ?i ."), "{b}");
    assert!(b.contains("?s2 <https://ex/slotIndex> ?i ."), "{b}");
    assert!(b.contains("FILTER ( ?s1 != ?s2 )"), "{b}");
}

#[test]
fn inverse_existence_lowers_to_a_typed_inverse_not_exists() {
    let b = project_sugar(
        "ex:inv a logic:InverseExistenceConstraint ;\n\
             logic:onClass ex:FeatureValue ;\n\
             logic:inversePredicate ex:denotationTarget ;\n\
             logic:subjectClass ex:Denotation ;\n\
             logic:formalizes ex:FeatureValue .",
    );
    assert!(
        b.contains("sh:targetClass <https://ex/FeatureValue>"),
        "{b}"
    );
    // Violation = no typed Denotation points back at $this.
    assert!(b.contains("FILTER NOT EXISTS"), "{b}");
    assert!(
        b.contains(
            "?s a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* <https://ex/Denotation> ."
        ),
        "{b}"
    );
    assert!(
        b.contains("?s <https://ex/denotationTarget> $this ."),
        "{b}"
    );
}

#[test]
fn transitive_reachability_lowers_to_a_subclass_property_path() {
    let b = project_sugar(
        "ex:tr a logic:TransitiveReachabilityConstraint ;\n\
             logic:onClass ex:FlagshipScenario ;\n\
             logic:viaPredicate ex:enforcesFailureClass ;\n\
             logic:pathPredicate ex:subClassOf ;\n\
             logic:reachTarget ex:ConformanceFailure ;\n\
             logic:formalizes ex:FlagshipScenario .",
    );
    assert!(
        b.contains("sh:targetClass <https://ex/FlagshipScenario>"),
        "{b}"
    );
    assert!(
        b.contains("$this <https://ex/enforcesFailureClass> ?v ."),
        "{b}"
    );
    // Violation = a failure class that does NOT transitively subclass the root.
    assert!(
        b.contains(
            "FILTER NOT EXISTS { ?v <https://ex/subClassOf>+ <https://ex/ConformanceFailure> . }"
        ),
        "{b}"
    );
}

#[test]
fn acyclic_constraint_lowers_to_a_self_reaching_property_path() {
    let b = project_sugar(
        "ex:ac a logic:AcyclicConstraint ;\n\
             logic:onClass ex:FormSlot ;\n\
             logic:pathPredicate ex:dependsOn ;\n\
             logic:formalizes ex:FormSlot .",
    );
    assert!(b.contains("sh:targetClass <https://ex/FormSlot>"), "{b}");
    // Violation = the focus reaches itself along one-or-more dependsOn hops.
    assert!(b.contains("$this <https://ex/dependsOn>+ $this ."), "{b}");
    assert!(!b.contains("NOT EXISTS"), "{b}");
}

#[test]
fn comparison_and_node_kind_constraints_validate_against_a_graph() {
    use purrdf::shapes::engine::validate_graphs;
    let src = format!(
        "{SUGAR_PREFIXES}\
ex:scaleCmp a logic:ComparisonConstraint ;\n\
  logic:onClass ex:ScoreScale ;\n\
  logic:leftPath ex:scaleMin ;\n\
  logic:rightPath ex:scaleMax ;\n\
  logic:compareOp \">=\" ;\n\
  logic:formalizes ex:ScoreScale ."
    );
    let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
    let shapes_ttl = project_procedural_constraints(&program);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let dec = "^^<http://www.w3.org/2001/XMLSchema#decimal>";
    let data = format!(
        "<https://ex/good> {ty} <https://ex/ScoreScale> .\n\
<https://ex/good> <https://ex/scaleMin> \"0.0\"{dec} .\n\
<https://ex/good> <https://ex/scaleMax> \"1.0\"{dec} .\n\
<https://ex/bad> {ty} <https://ex/ScoreScale> .\n\
<https://ex/bad> <https://ex/scaleMin> \"1.0\"{dec} .\n\
<https://ex/bad> <https://ex/scaleMax> \"1.0\"{dec} .\n"
    );
    let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    assert!(
        flagged.iter().any(|f| f.contains("bad")),
        "the min>=max scale must be flagged; flagged: {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|f| f.contains("good")),
        "the well-formed scale must NOT be flagged; flagged: {flagged:?}"
    );
}

// ── procedural-constraint capabilities (arithmetic BIND, variable predicate,
//    direct-instance target, filter disjunction) ─────────────────────────────────

/// Collect the focus nodes a projected constraint document flags over `data` (N-Triples).
fn flagged_over(shapes_ttl: &str, data: &str) -> Vec<String> {
    use purrdf::shapes::engine::validate_graphs;
    let report = validate_graphs(data, shapes_ttl, None).expect("validate");
    report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect()
}

#[test]
fn arithmetic_sum_bind_lowers_and_flags_a_dimension_mismatch() {
    // ∀this:Widget. ¬∃(p,q,d,s). sigPos(this,p) ∧ sigNeg(this,q) ∧ dim(this,d) ∧
    //   termSum(s,p,q) ∧ termDistinct(s,d)  — the p+q ≠ dimensionCount invariant.
    let sum = Formula::atom(tiri(LOGIC_TERM_SUM), vec![tvar("s"), tvar("p"), tvar("q")]).unwrap();
    let distinct = Formula::atom(tiri(LOGIC_TERM_DISTINCT), vec![tvar("s"), tvar("d")]).unwrap();
    let body = Formula::And(vec![
        atom("https://ex/sigPos", tvar("this"), tvar("p")),
        atom("https://ex/sigNeg", tvar("this"), tvar("q")),
        atom("https://ex/dim", tvar("this"), tvar("d")),
        sum,
        distinct,
    ]);
    let c = guarded("https://ex/cSum", Formula::Not(Box::new(exists("p", body))));
    let b = block(&c);
    assert!(b.contains("BIND ( ( ?p + ?q ) AS ?s )"), "{b}");
    // The BIND must precede the FILTER that reads ?s.
    assert!(
        b.find("BIND").unwrap() < b.find("FILTER ( ?s != ?d )").unwrap(),
        "BIND must precede its FILTER: {b}"
    );
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let doc = project_procedural_constraints(&prog);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let data = format!(
        "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/sigPos> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> <https://ex/sigNeg> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> <https://ex/dim> \"5\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/sigPos> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> <https://ex/sigNeg> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> <https://ex/dim> \"3\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n"
    );
    let flagged = flagged_over(&doc, &data);
    assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
    assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
}

#[test]
fn variable_predicate_link_lowers_and_flags_any_edge_to_a_typed_object() {
    // ∀this:Widget. ¬∃(link,c). linkVia(this,link,c) ∧ type(c, ex:Bad) — any predicate
    // linking the focus to a Bad-typed object is forbidden.
    let link = Formula::atom(
        tiri(LOGIC_LINK_VIA),
        vec![tvar("this"), tvar("link"), tvar("c")],
    )
    .unwrap();
    let body = Formula::And(vec![
        link,
        atom(RDF_TYPE, tvar("c"), tiri("https://ex/Bad")),
    ]);
    let c = guarded(
        "https://ex/cLink",
        Formula::Not(Box::new(exists("c", body))),
    );
    let b = block(&c);
    assert!(b.contains("$this ?link ?c ."), "{b}");
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let doc = project_procedural_constraints(&prog);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let data = format!(
        "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/anyEdge> <https://ex/x> .\n\
<https://ex/x> {ty} <https://ex/Bad> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/anyEdge> <https://ex/y> .\n"
    );
    let flagged = flagged_over(&doc, &data);
    assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
    assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
}

#[test]
fn direct_instance_target_excludes_subclass_typed_nodes() {
    // ∀this. directType(this, ex:Base) → ¬∃v. required(this, v).
    let integrity = Formula::Forall {
        vars: vec!["this".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    tiri(LOGIC_DIRECT_TYPE),
                    vec![tvar("this"), tiri("https://ex/Base")],
                )
                .unwrap(),
            ),
            Box::new(exists(
                "v",
                atom("https://ex/required", tvar("this"), tvar("v")),
            )),
        )),
    };
    let c = ConstraintIr::new(
        "https://ex/cDirect",
        integrity,
        ShaclSeverity::Violation,
        None,
    )
    .unwrap();
    assert!(matches!(&c.target, ShapeTarget::DirectClass(x) if x == "https://ex/Base"));
    let b = block(&c);
    assert!(b.contains("a sh:SPARQLTarget"), "{b}");
    assert!(b.contains("rdf-schema#subClassOf>+"), "{b}");
    // The directType marker must NOT leak into the WHERE body as a triple.
    assert!(!b.contains("directType"), "{b}");
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let doc = project_procedural_constraints(&prog);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let data = format!(
        "<https://ex/Sub> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://ex/Base> .\n\
<https://ex/directBad> {ty} <https://ex/Base> .\n\
<https://ex/subExcluded> {ty} <https://ex/Base> .\n\
<https://ex/subExcluded> {ty} <https://ex/Sub> .\n"
    );
    let flagged = flagged_over(&doc, &data);
    // directBad is a bare Base instance missing `required` → flagged.
    assert!(
        flagged.iter().any(|f| f.contains("directBad")),
        "{flagged:?}"
    );
    // subExcluded is ALSO a Sub instance → excluded by the direct-instance target.
    assert!(
        !flagged.iter().any(|f| f.contains("subExcluded")),
        "a subclass-typed node must be excluded: {flagged:?}"
    );
}

#[test]
fn filter_disjunction_lowers_to_one_combined_filter_not_a_union() {
    // ∀this:Widget. ¬∃(b,d). band(this,b) ∧ dec(this,d) ∧
    //   ( (b = ex:certain ∧ d < 0.9) ∨ (b = ex:unspecified) )
    let dec_lit = |n: &str| {
        Term::literal(
            n.to_owned(),
            Some("http://www.w3.org/2001/XMLSchema#decimal".to_owned()),
        )
        .unwrap()
    };
    let arm1 = Formula::And(vec![
        Formula::atom(
            tiri(LOGIC_TERM_EQUAL),
            vec![tvar("b"), tiri("https://ex/certain")],
        )
        .unwrap(),
        Formula::atom(tiri(LOGIC_TERM_LESS), vec![tvar("d"), dec_lit("0.9")]).unwrap(),
    ]);
    let arm2 = Formula::atom(
        tiri(LOGIC_TERM_EQUAL),
        vec![tvar("b"), tiri("https://ex/unspecified")],
    )
    .unwrap();
    let bad = Formula::Or(vec![arm1, arm2]);
    let body = Formula::And(vec![
        atom("https://ex/band", tvar("this"), tvar("b")),
        atom("https://ex/dec", tvar("this"), tvar("d")),
        bad,
    ]);
    let c = guarded(
        "https://ex/cBand",
        Formula::Not(Box::new(exists("b", body))),
    );
    let b = block(&c);
    assert!(b.contains("FILTER ("), "{b}");
    assert!(
        b.contains(" || "),
        "the disjunction must be one FILTER, not a UNION: {b}"
    );
    assert!(!b.contains("UNION"), "{b}");
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
    let doc = project_procedural_constraints(&prog);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let data = format!(
        "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/band> <https://ex/certain> .\n\
<https://ex/bad> <https://ex/dec> \"0.3\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/band> <https://ex/certain> .\n\
<https://ex/good> <https://ex/dec> \"0.95\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n"
    );
    let flagged = flagged_over(&doc, &data);
    assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
    assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
}

#[test]
fn consent_exactly_one_and_at_least_one_lowers_without_aggregates() {
    // The RightsStatement consent invariant, encoded in first-order form (no COUNT): a
    // consent-governing RightsStatement (a permission whose ruleAction is
    // processPersonalData) must have EXACTLY ONE data subject and AT LEAST ONE data
    // controller. "Exactly one subject" is the nested-negation form ∃s. subj(s) ∧
    // ¬∃s2.(subj(s2) ∧ s2≠s); SHACL pre-binds $this, so the UNION-of-negations arms correlate.
    let rs = "https://ex/RightsStatement";
    let action = "https://ex/processPersonalData";
    let one_subject = exists(
        "s",
        Formula::And(vec![
            atom("https://ex/hasDataSubject", tvar("this"), tvar("s")),
            Formula::Not(Box::new(exists(
                "s2",
                Formula::And(vec![
                    atom("https://ex/hasDataSubject", tvar("this"), tvar("s2")),
                    Formula::atom(tiri(LOGIC_TERM_DISTINCT), vec![tvar("s2"), tvar("s")]).unwrap(),
                ]),
            ))),
        ]),
    );
    let at_least_one_controller = exists(
        "c",
        atom("https://ex/hasDataController", tvar("this"), tvar("c")),
    );
    let guard = || {
        Formula::And(vec![
            atom(RDF_TYPE, tvar("this"), tiri(rs)),
            atom("https://ex/hasPermission", tvar("this"), tvar("perm")),
            atom("https://ex/ruleAction", tvar("perm"), tiri(action)),
        ])
    };
    // Two peer constraints (both formalizing the same RightsStatement source-term): one guards
    // the exactly-one-subject condition, one the at-least-one-controller condition. Splitting a
    // compound `∧` invariant into single-`FILTER NOT EXISTS` peers keeps each violation body a
    // guarded conjunction (no top-level UNION-of-filters whose arms bind no focus).
    let mk = |iri: &str, phi: Formula| {
        let integrity = Formula::Forall {
            vars: vec!["this".to_owned()],
            body: Box::new(Formula::Implies(Box::new(guard()), Box::new(phi))),
        };
        ConstraintIr::new(iri, integrity, ShaclSeverity::Warning, None)
            .unwrap()
            .with_formalizes("https://ex/ConsentWellformednessShape")
            .unwrap()
    };
    let c_subj = mk("https://ex/cConsentSubject", one_subject);
    let c_ctrl = mk("https://ex/cConsentController", at_least_one_controller);
    assert!(matches!(&c_subj.target, ShapeTarget::Class(x) if x == rs));
    let prog =
        LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c_subj, c_ctrl]);
    let doc = project_procedural_constraints(&prog);
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    // rs2: 1 subject, 0 controller (violates); rs3: 0 subject, 1 controller (violates);
    // rs4: 2 subjects, 1 controller (violates); rsGood: 1 subject, 1 controller (ok);
    // rsPlain: NOT consent-governing (perm has no processPersonalData action) — must not fire.
    let data = format!(
        "<https://ex/rs2> {ty} <{rs}> .\n\
<https://ex/rs2> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rs2> <https://ex/hasPermission> <https://ex/perm2> .\n\
<https://ex/perm2> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rs3> {ty} <{rs}> .\n\
<https://ex/rs3> <https://ex/hasDataController> <https://ex/alice> .\n\
<https://ex/rs3> <https://ex/hasPermission> <https://ex/perm3> .\n\
<https://ex/perm3> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rs4> {ty} <{rs}> .\n\
<https://ex/rs4> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rs4> <https://ex/hasDataSubject> <https://ex/bob> .\n\
<https://ex/rs4> <https://ex/hasDataController> <https://ex/alice> .\n\
<https://ex/rs4> <https://ex/hasPermission> <https://ex/perm4> .\n\
<https://ex/perm4> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rsGood> {ty} <{rs}> .\n\
<https://ex/rsGood> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rsGood> <https://ex/hasDataController> <https://ex/acme> .\n\
<https://ex/rsGood> <https://ex/hasPermission> <https://ex/permG> .\n\
<https://ex/permG> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rsPlain> {ty} <{rs}> .\n\
<https://ex/rsPlain> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rsPlain> <https://ex/hasDataSubject> <https://ex/bob> .\n\
<https://ex/rsPlain> <https://ex/hasPermission> <https://ex/permP> .\n"
    );
    let flagged = flagged_over(&doc, &data);
    for bad in ["rs2", "rs3", "rs4"] {
        assert!(
            flagged.iter().any(|f| f.contains(bad)),
            "{bad} must be flagged: {flagged:?}"
        );
    }
    assert!(
        !flagged.iter().any(|f| f.contains("rsGood")),
        "the well-formed consent statement must NOT fire: {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|f| f.contains("rsPlain")),
        "a non-consent statement must NOT fire: {flagged:?}"
    );
}
