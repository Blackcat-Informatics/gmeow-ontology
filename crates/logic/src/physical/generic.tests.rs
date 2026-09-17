// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// ── A small inline OWL-RL-shaped rule corpus (exercises the generic evaluator) ──
// The RL rule table was cut over to purrdf's `entail` chase (`reason/rl.rs`), so
// this evaluator no longer has a production RL table to borrow. These tests are
// about the GENERIC EVALUATOR — variable-predicate joins, RDF-list recursion,
// multi-atom bodies — not about OWL-RL completeness, so they carry their own
// corpus: exactly the clause families the tests below fire, in the same
// `triple(?s,?p,?o,?w)` / `list_member(?l,?x,?w)` encoding.

fn var(name: &str) -> EvalTerm {
    EvalTerm::Var(name.to_owned())
}
fn named(iri: &str) -> EvalTerm {
    EvalTerm::ConstNamed(iri.to_owned())
}
fn atom(relation: &str, args: Vec<EvalTerm>) -> GenericAtom {
    GenericAtom {
        relation: relation.to_owned(),
        args,
    }
}
fn rule(rule_iri: &str, head: GenericAtom, body: Vec<GenericAtom>) -> GenericRule {
    GenericRule {
        head,
        body,
        rule_iri: rule_iri.to_owned(),
    }
}
/// A `triple(?s, ?p, ?o, ?w)` atom from four terms.
fn tr(s: EvalTerm, p: EvalTerm, o: EvalTerm, w: EvalTerm) -> GenericAtom {
    atom("triple", vec![s, p, o, w])
}
/// A `list_member(?l, ?x, ?w)` atom from three terms.
fn lm(l: EvalTerm, x: EvalTerm, w: EvalTerm) -> GenericAtom {
    atom("list_member", vec![l, x, w])
}

/// The clause families the generic-evaluator tests below exercise.
fn rl_exercise_rules() -> Vec<GenericRule> {
    vec![
        // cax-sco: x a C1, C1 ⊑ C2 ⇒ x a C2.
        rule(
            "rl:cax-sco",
            tr(var("?x"), named(TYPE), var("?c2"), var("?w")),
            vec![
                tr(var("?x"), named(TYPE), var("?c1"), var("?w")),
                tr(var("?c1"), named(SUBCLASS), var("?c2"), var("?w")),
            ],
        ),
        // scm-sco: C1 ⊑ C2, C2 ⊑ C3 ⇒ C1 ⊑ C3.
        rule(
            "rl:scm-sco",
            tr(var("?c1"), named(SUBCLASS), var("?c3"), var("?w")),
            vec![
                tr(var("?c1"), named(SUBCLASS), var("?c2"), var("?w")),
                tr(var("?c2"), named(SUBCLASS), var("?c3"), var("?w")),
            ],
        ),
        // prp-spo1: P1 ⊑ P2, x P1 y ⇒ x P2 y (variable predicate position).
        rule(
            "rl:prp-spo1",
            tr(var("?x"), var("?p2"), var("?y"), var("?w")),
            vec![
                tr(var("?p1"), named(SUBPROP), var("?p2"), var("?w")),
                tr(var("?x"), var("?p1"), var("?y"), var("?w")),
            ],
        ),
        // prp-dom: P domain C, x P y ⇒ x a C.
        rule(
            "rl:prp-dom",
            tr(var("?x"), named(TYPE), var("?c"), var("?w")),
            vec![
                tr(var("?p"), named(DOMAIN), var("?c"), var("?w")),
                tr(var("?x"), var("?p"), var("?y"), var("?w")),
            ],
        ),
        // prp-rng: P range C, x P y ⇒ y a C.
        rule(
            "rl:prp-rng",
            tr(var("?y"), named(TYPE), var("?c"), var("?w")),
            vec![
                tr(var("?p"), named(RANGE), var("?c"), var("?w")),
                tr(var("?x"), var("?p"), var("?y"), var("?w")),
            ],
        ),
        // prp-trp: P transitive, x P y, y P z ⇒ x P z.
        rule(
            "rl:prp-trp",
            tr(var("?x"), var("?p"), var("?z"), var("?w")),
            vec![
                tr(var("?p"), named(TYPE), named(TRANSITIVE), var("?w")),
                tr(var("?x"), var("?p"), var("?y"), var("?w")),
                tr(var("?y"), var("?p"), var("?z"), var("?w")),
            ],
        ),
        // prp-symp: P symmetric, x P y ⇒ y P x.
        rule(
            "rl:prp-symp",
            tr(var("?y"), var("?p"), var("?x"), var("?w")),
            vec![
                tr(var("?p"), named(TYPE), named(SYMMETRIC), var("?w")),
                tr(var("?x"), var("?p"), var("?y"), var("?w")),
            ],
        ),
        // prp-inv1: P1 inverseOf P2, x P1 y ⇒ y P2 x.
        rule(
            "rl:prp-inv1",
            tr(var("?y"), var("?p2"), var("?x"), var("?w")),
            vec![
                tr(var("?p1"), named(INVERSE_OF), var("?p2"), var("?w")),
                tr(var("?x"), var("?p1"), var("?y"), var("?w")),
            ],
        ),
        // list_member (head + tail) over an RDF list.
        rule(
            "rl:list-member-head",
            lm(var("?l"), var("?x"), var("?w")),
            vec![tr(var("?l"), named(FIRST), var("?x"), var("?w"))],
        ),
        rule(
            "rl:list-member-tail",
            lm(var("?l"), var("?x"), var("?w")),
            vec![
                tr(var("?l"), named(REST), var("?r"), var("?w")),
                lm(var("?r"), var("?x"), var("?w")),
            ],
        ),
        // cls-oneOf: C oneOf L, x ∈ L ⇒ x a C.
        rule(
            "rl:cls-oneOf",
            tr(var("?x"), named(TYPE), var("?c"), var("?w")),
            vec![
                tr(var("?c"), named(ONE_OF), var("?l"), var("?w")),
                lm(var("?l"), var("?x"), var("?w")),
            ],
        ),
        // cls-union-member: C unionOf L, m ∈ L ⇒ m ⊑ C.
        rule(
            "rl:cls-union-member",
            tr(var("?m"), named(SUBCLASS), var("?c"), var("?w")),
            vec![
                tr(var("?c"), named(UNION_OF), var("?l"), var("?w")),
                lm(var("?l"), var("?m"), var("?w")),
            ],
        ),
        // cls-hv1: R onProperty P, R hasValue V, x a R ⇒ x P V.
        rule(
            "rl:cls-hv1",
            tr(var("?x"), var("?p"), var("?v"), var("?w")),
            vec![
                tr(var("?x"), named(TYPE), var("?r"), var("?w")),
                tr(var("?r"), named(ON_PROPERTY), var("?p"), var("?w")),
                tr(var("?r"), named(HAS_VALUE), var("?v"), var("?w")),
            ],
        ),
        // cls-hv2: R onProperty P, R hasValue V, z P V ⇒ z a R.
        rule(
            "rl:cls-hv2",
            tr(var("?x"), named(TYPE), var("?r"), var("?w")),
            vec![
                tr(var("?r"), named(ON_PROPERTY), var("?p"), var("?w")),
                tr(var("?r"), named(HAS_VALUE), var("?v"), var("?w")),
                tr(var("?x"), var("?p"), var("?v"), var("?w")),
            ],
        ),
    ]
}

// ── generic-triple EDB helpers (the RL predicate-as-data encoding) ──────────

const RL_W: &str = "urn:world:rl-generic";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
const LIT_SURROGATE: &str = "urn:gmeow-rl-lit:0";

/// A tiny generic-triple EDB builder in the `triple(?s,?p,?o,?w)` encoding.
#[derive(Default)]
struct Edb {
    facts: TypedFactSet,
}

impl Edb {
    /// Push `triple(subject, predicate, object, world)` with IRI terms.
    fn t(&mut self, s: &str, p: &str, o: &str) -> &mut Self {
        self.push(s, p, TermValue::iri(o));
        self
    }

    /// Push a triple whose OBJECT is an already-interned literal-surrogate IRI —
    /// the shape [`crate::reason::rl::encode_generic_edb`] produces for a literal
    /// object (RL never inspects the literal value, only the surrogate IRI).
    fn t_lit(&mut self, s: &str, p: &str) -> &mut Self {
        self.push(s, p, TermValue::iri(LIT_SURROGATE));
        self
    }

    fn push(&mut self, s: &str, p: &str, o: TermValue) {
        let s = self.facts.intern(&TermValue::iri(s));
        let p = self.facts.intern(&TermValue::iri(p));
        let o = self.facts.intern(&o);
        let w = self.facts.intern(&TermValue::simple_literal(RL_W));
        self.facts.push_fact("triple", vec![s, p, o, w]);
    }

    /// Run the inline exercise corpus over this EDB, returning the derived triples.
    fn close(&self) -> TypedChaseResult {
        let rules = rl_exercise_rules();
        materialize_generic(&self.facts, &rules).expect("generic materialize")
    }
}

fn edb() -> Edb {
    Edb::default()
}

/// Whether the closure carries `triple(s, p, o)` (object an IRI) in any world.
fn has(closure: &TypedChaseResult, s: &str, p: &str, o: &str) -> bool {
    has_obj(closure, s, p, &format!("<{o}>"))
}

/// Whether the closure carries `triple(s, p, <object-surface>)`.
fn has_obj(closure: &TypedChaseResult, s: &str, p: &str, obj_surface: &str) -> bool {
    closure.rows.iter().any(|(row, _)| {
        row.predicate == "triple"
            && row.args.len() == 4
            && term_display(&row.args[0]) == format!("<{s}>")
            && term_display(&row.args[1]) == format!("<{p}>")
            && term_display(&row.args[2]) == obj_surface
    })
}

const A: &str = "http://ex/A";
const B: &str = "http://ex/B";
const C: &str = "http://ex/C";
const P: &str = "http://ex/p";
const P1: &str = "http://ex/p1";
const P2: &str = "http://ex/p2";
const X: &str = "http://ex/x";
const Y: &str = "http://ex/y";
const Z: &str = "http://ex/z";

#[test]
fn generic_cax_sco_propagates_type_through_subclass() {
    let mut e = edb();
    e.t(X, TYPE, A).t(A, SUBCLASS, B);
    let c = e.close();
    assert!(has(&c, X, TYPE, B), "x a B via cax-sco");
    // Echoed EDB present too.
    assert!(has(&c, X, TYPE, A), "asserted x a A echoed");
}

#[test]
fn generic_scm_sco_is_transitive() {
    let mut e = edb();
    e.t(A, SUBCLASS, B).t(B, SUBCLASS, C);
    let c = e.close();
    assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via scm-sco transitivity");
}

#[test]
fn generic_prp_spo1_binds_a_variable_predicate() {
    // The load-bearing case: prp-spo1 quantifies over the PROPERTY position.
    let mut e = edb();
    e.t(P1, SUBPROP, P2).t(X, P1, Y);
    let c = e.close();
    assert!(
        has(&c, X, P2, Y),
        "x p2 y via prp-spo1 (variable predicate)"
    );
}

#[test]
fn generic_prp_spo1_carries_a_literal_surrogate_object() {
    // prp-spo1 propagating a literal object (interned to a surrogate IRI): the
    // surrogate rides through the variable-predicate join unchanged.
    let mut e = edb();
    e.t(P1, SUBPROP, P2).t_lit(X, P1);
    let c = e.close();
    assert!(
        has_obj(&c, X, P2, &format!("<{LIT_SURROGATE}>")),
        "x p2 <lit-surrogate> via prp-spo1"
    );
}

#[test]
fn generic_prp_trp_closes_a_transitive_chain() {
    let mut e = edb();
    e.t(P, TYPE, TRANSITIVE).t(X, P, Y).t(Y, P, Z);
    let c = e.close();
    assert!(has(&c, X, P, Z), "x p z via prp-trp");
}

#[test]
fn generic_prp_symp_mirrors_a_symmetric_edge() {
    let mut e = edb();
    e.t(P, TYPE, SYMMETRIC).t(X, P, Y);
    let c = e.close();
    assert!(has(&c, Y, P, X), "y p x via prp-symp");
}

#[test]
fn generic_prp_inv_derives_both_directions() {
    let mut e = edb();
    e.t(P1, INVERSE_OF, P2).t(X, P1, Y);
    let c = e.close();
    assert!(has(&c, Y, P2, X), "y p2 x via prp-inv1");
}

#[test]
fn generic_prp_dom_and_rng_derive_types() {
    let mut e = edb();
    e.t(P, DOMAIN, A).t(P, RANGE, B).t(X, P, Y);
    let c = e.close();
    assert!(has(&c, X, TYPE, A), "x a A via prp-dom");
    assert!(has(&c, Y, TYPE, B), "y a B via prp-rng");
}

#[test]
fn generic_cls_oneof_over_a_list_and_list_member() {
    // C oneOf ( x y ) ⇒ x a C, y a C — exercises list_member recursion + cls-oneOf.
    let l0 = "http://ex/l0";
    let l1 = "http://ex/l1";
    let mut e = edb();
    e.t(C, ONE_OF, l0)
        .t(l0, FIRST, X)
        .t(l0, REST, l1)
        .t(l1, FIRST, Y)
        .t(l1, REST, NIL);
    let c = e.close();
    assert!(has(&c, X, TYPE, C), "x a C via cls-oneOf");
    assert!(has(&c, Y, TYPE, C), "y a C via cls-oneOf");
}

#[test]
fn generic_cls_union_member_subclasses_each_member() {
    let l0 = "http://ex/l0";
    let l1 = "http://ex/l1";
    let mut e = edb();
    e.t(C, UNION_OF, l0)
        .t(l0, FIRST, A)
        .t(l0, REST, l1)
        .t(l1, FIRST, B)
        .t(l1, REST, NIL);
    let c = e.close();
    assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via cls-union-member");
    assert!(has(&c, B, SUBCLASS, C), "B ⊑ C via cls-union-member");
}

#[test]
fn generic_cls_hasvalue_asserts_and_recognizes() {
    // R onProperty P ; R hasValue V ; x a R ⇒ x P V (cls-hv1);
    //                               z P V ⇒ z a R (cls-hv2).
    let r = "http://ex/R";
    let v = "http://ex/v";
    let mut e = edb();
    e.t(r, ON_PROPERTY, P)
        .t(r, HAS_VALUE, v)
        .t(X, TYPE, r)
        .t(Z, P, v);
    let c = e.close();
    assert!(has(&c, X, P, v), "x P V via cls-hv1");
    assert!(has(&c, Z, TYPE, r), "z a R via cls-hv2");
}
