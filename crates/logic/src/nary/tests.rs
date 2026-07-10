// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit + native↔Nemo parity coverage for the reified n-ary lowering.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::{
    NaryArg, NaryAtom, NaryRule, NaryTuple, certify_nary_termination, lower_nary_fact,
    lower_nary_rules, run_native_nary_forward,
};
use crate::physical::ChaseAdmission;
use crate::provenance::{instance_of_iri, mint_nary_reifier, nary_arg_predicate, term_display};

// ── The n-ary multi-head existential demonstrator program ─────────────────────
//
// An arity-4 EDB relation `m0` and ONE multi-head TGD that invents TWO n-ary tuples per
// body binding — `m1` (arity 3) and `m2` (arity 2) — sharing a SINGLE existential null
// `?e` across both heads (the ChaseBench/kr2024 family shape):
//
//     m1(?a, ?e, ?c) ∧ m2(?e, ?d)  ←  m0(?a, ?b, ?c, ?d)
//
// `?e` is a genuine restricted-chase shared null: not bound by the body, shared across the
// two invented tuples. Each invented tuple gets its OWN reifier existential, minted by
// tuple identity — so this exercises the multi-reifier generalization of `reified_nary_head`.

const M0: &str = "http://ex/nary/m0";
const M1: &str = "http://ex/nary/m1";
const M2: &str = "http://ex/nary/m2";
const RULE: &str = "http://ex/nary/rules/split";

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

/// The two EDB tuples `m0(a{i}, b{i}, c{i}, d{i})`.
fn demo_edb() -> Vec<NaryTuple> {
    (0..2)
        .map(|i| NaryTuple {
            relation: M0.to_owned(),
            args: vec![
                iri(&format!("http://ex/a{i}")),
                iri(&format!("http://ex/b{i}")),
                iri(&format!("http://ex/c{i}")),
                iri(&format!("http://ex/d{i}")),
            ],
        })
        .collect()
}

fn v(name: &str) -> NaryArg {
    NaryArg::Var(name.to_owned())
}

/// The multi-head, shared-null n-ary TGD.
fn demo_rules() -> Vec<NaryRule> {
    vec![NaryRule {
        name: RULE.to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![
            NaryAtom {
                relation: M1.to_owned(),
                args: vec![v("?a"), v("?e"), v("?c")],
            },
            NaryAtom {
                relation: M2.to_owned(),
                args: vec![v("?e"), v("?d")],
            },
        ],
    }]
}

/// The ORIGINAL n-ary `.rls` for the Nemo side — `!e` is Nemo's existential surface.
fn demo_rls() -> String {
    format!(
        "#[name(\"{RULE}\")]\n\
         <{M1}>(?a, !e, ?c), <{M2}>(!e, ?d) :- <{M0}>(?a, ?b, ?c, ?d) .\n"
    )
}

// ── Fact-lowering unit coverage ───────────────────────────────────────────────

#[test]
fn lower_nary_fact_reifies_onto_the_content_addressed_node() {
    let args = vec![iri("http://ex/x"), iri("http://ex/y"), iri("http://ex/z")];
    let facts = lower_nary_fact("http://ex/rel", &args).expect("ground reification");
    let reifier = mint_nary_reifier("http://ex/rel", &args).expect("mint");

    // Exactly one instanceOf typing atom + one naryArg{i} per argument, all on the reifier.
    assert_eq!(facts.len(), args.len() + 1);
    let typing = facts
        .iter()
        .find(|f| f.predicate == instance_of_iri())
        .expect("a typing atom");
    assert_eq!(term_display(&typing.subject), format!("<{reifier}>"));
    assert_eq!(typing.object, iri("http://ex/rel"));
    for (i, arg) in args.iter().enumerate() {
        let a = facts
            .iter()
            .find(|f| f.predicate == nary_arg_predicate(i))
            .expect("a positional argument atom");
        assert_eq!(term_display(&a.subject), format!("<{reifier}>"));
        assert_eq!(&a.object, arg);
    }
}

// ── Doctrinal refusals ────────────────────────────────────────────────────────

#[test]
fn lower_refuses_a_non_range_restricted_unshared_head_argument() {
    // `?e` occurs in a SINGLE head atom and no body atom — it can never be a shared null,
    // so it is a Skolem-function obligation, refused rather than mis-lowered as exact.
    let rules = vec![NaryRule {
        name: "http://ex/bad".to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![NaryAtom {
            relation: M1.to_owned(),
            args: vec![v("?a"), v("?e"), v("?c")],
        }],
    }];
    let err = lower_nary_rules(&rules).expect_err("a non-range-restricted arg must be refused");
    assert!(
        err.message().contains("not range-restricted") && err.message().contains("Skolem-function"),
        "the refusal must name the Skolem-function obligation it protects: {err}"
    );
    assert!(
        !err.message().contains('#'),
        "no process refs in the refusal message: {err}"
    );
}

#[test]
fn lower_refuses_an_empty_head() {
    let rules = vec![NaryRule {
        name: "http://ex/empty".to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![],
    }];
    let err = lower_nary_rules(&rules).expect_err("an empty head must be refused");
    assert!(err.message().contains("empty head"), "{err}");
}

// ── Termination certificate ───────────────────────────────────────────────────

#[test]
fn demo_program_is_certified_weakly_acyclic() {
    // The relation-qualified certifier must certify the fresh-head, non-recursive program —
    // the canonical shared-`naryArg` certifier would spuriously see a cycle, which is
    // exactly why `certify_nary_termination` qualifies by relation.
    let admission = certify_nary_termination(&demo_rules()).expect("certify");
    assert!(
        matches!(admission, ChaseAdmission::WeaklyAcyclic { .. }),
        "the multi-head n-ary demonstrator must certify weakly acyclic, got {admission:?}"
    );
}

// ── native↔Nemo parity: the whole point ───────────────────────────────────────

#[test]
fn native_reified_nary_forward_agrees_with_nemo_on_a_multi_head_program() {
    let edb = demo_edb();
    let rules = demo_rules();

    // NATIVE: lower to reified binary, chase, de-reify back to n-ary tuples.
    let native = run_native_nary_forward(&edb, &rules).expect("native n-ary forward");

    // NEMO: the SAME tuples as a typed n-ary EDB, the ORIGINAL n-ary `.rls`, facts-only.
    let nemo = nemo_nary_forward(&edb, &demo_rls());

    // Structural non-vacuity: the native closure carries the invented m1 + m2 tuples, and
    // the shared null is ACTUALLY shared (m1's arg1 equals m2's arg0 for each firing).
    assert_shared_null_structure(&native);

    // PARITY: the de-reified native tuple set EQUALS Nemo's, null-blind (invented nulls are
    // named per-engine — native mints a Skolem IRI, Nemo a labeled null — so they compare
    // up to a consistent, structure-respecting renaming via colour refinement).
    let native_ms = canonical_multiset(&native);
    let nemo_ms = canonical_multiset(&nemo);
    assert_eq!(
        native_ms, nemo_ms,
        "native reified n-ary chase must AGREE with Nemo's n-ary chase null-blind.\n\
         native (canonical): {native_ms:#?}\n\
         nemo   (canonical): {nemo_ms:#?}"
    );

    // The parity is non-trivial: both sides carry the 2 EDB + derived tuples across 3 relations.
    let relations: BTreeSet<&str> = native_ms.keys().map(|(r, _)| r.as_str()).collect();
    assert!(
        relations.contains(M0) && relations.contains(M1) && relations.contains(M2),
        "the agreed closure must span all three relations: {relations:?}"
    );

    // Determinism: a second native run is byte-identical.
    let native_again = run_native_nary_forward(&edb, &rules).expect("native rerun");
    assert_eq!(
        native, native_again,
        "the native n-ary closure must be deterministic across runs"
    );
}

/// Drive Nemo over the n-ary EDB + original `.rls` via the facts-only typed chase, decoding
/// each returned row into a [`NaryTuple`].
fn nemo_nary_forward(edb: &[NaryTuple], rls: &str) -> Vec<NaryTuple> {
    use crate::facts::TypedFactSet;

    let mut typed = TypedFactSet::new();
    for tuple in edb {
        let ids: Vec<_> = tuple.args.iter().map(|a| typed.intern(a)).collect();
        typed.push_fact(&tuple.relation, ids);
    }
    let rows = crate::nemo_engine::run_chase_typed_facts_only(&typed, rls)
        .expect("nemo facts-only n-ary chase");
    rows.into_iter()
        .map(|row| NaryTuple {
            relation: row.predicate,
            args: row.args,
        })
        .collect()
}

/// Assert the native closure carries ≥1 `m1` and ≥1 `m2` tuple and that every firing's
/// existential null is SHARED — the `m1(a, e, c)` witness equals the `m2(e, d)` witness.
fn assert_shared_null_structure(tuples: &[NaryTuple]) {
    let m1: Vec<&NaryTuple> = tuples.iter().filter(|t| t.relation == M1).collect();
    let m2: Vec<&NaryTuple> = tuples.iter().filter(|t| t.relation == M2).collect();
    assert_eq!(m1.len(), 2, "one m1 tuple per EDB binding");
    assert_eq!(m2.len(), 2, "one m2 tuple per EDB binding");

    // Every m1 null (arg1) is an invented witness that also heads exactly one m2 tuple.
    for t in &m1 {
        let null = &t.args[1];
        assert!(
            is_null(null),
            "m1's shared position must be an invented null: {t:?}"
        );
        assert!(
            m2.iter().any(|u| &u.args[0] == null),
            "the m1 null {null:?} must be SHARED as the subject of an m2 tuple: {m2:?}"
        );
    }
}

// ── Null-blind canonicalization (colour refinement) ───────────────────────────

/// Whether a term is an invented null: a native chase Skolem IRI (`…/skolem/…`) or a Nemo
/// labeled null (`urn:gmeow:nemo-null:…`).
fn is_null(t: &TermValue) -> bool {
    let d = term_display(t);
    d.contains("/skolem/") || d.contains("nemo-null:")
}

/// Canonicalize an n-ary tuple set to a null-blind MULTISET by colour refinement of the
/// null-labeled tuple hypergraph: a null's colour is the fixpoint of the multiset of
/// `(relation, its position, the colours of every argument)` contexts it occurs in, grounded
/// in the named terms. Isomorphic null structures converge to equal colours across engines;
/// non-isomorphic ones never do, and witness MULTIPLICITY is preserved by the count.
fn canonical_multiset(tuples: &[NaryTuple]) -> BTreeMap<(String, Vec<String>), usize> {
    let nulls: BTreeSet<String> = tuples
        .iter()
        .flat_map(|t| t.args.iter())
        .filter(|a| is_null(a))
        .map(term_display)
        .collect();

    // Seed colours: a named term anchors on its own surface; a null starts uniform.
    let mut colour: BTreeMap<String, String> = BTreeMap::new();
    for t in tuples {
        for a in &t.args {
            let s = term_display(a);
            colour.entry(s.clone()).or_insert_with(|| {
                if is_null(a) {
                    "\u{0}".to_owned()
                } else {
                    s.clone()
                }
            });
        }
    }

    if !nulls.is_empty() {
        for _ in 0..=nulls.len() {
            let mut next = colour.clone();
            let mut changed = false;
            for n in &nulls {
                let mut sig: Vec<String> = Vec::new();
                for t in tuples {
                    let ctx: Vec<String> = t
                        .args
                        .iter()
                        .map(|a| colour[&term_display(a)].clone())
                        .collect();
                    for (p, a) in t.args.iter().enumerate() {
                        if term_display(a) == *n {
                            sig.push(format!(
                                "{}\u{1f}{p}\u{1f}{}",
                                t.relation,
                                ctx.join("\u{1f}")
                            ));
                        }
                    }
                }
                sig.sort();
                let refined = crate::provenance::sha1_hex(&sig.join("\u{1e}"));
                if next[n] != refined {
                    changed = true;
                    next.insert(n.clone(), refined);
                }
            }
            colour = next;
            if !changed {
                break;
            }
        }
    }

    let distinct: BTreeSet<String> = nulls.iter().map(|n| colour[n].clone()).collect();
    let token: BTreeMap<String, String> = distinct
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, format!("gmeow:null#{i}")))
        .collect();

    let mut ms: BTreeMap<(String, Vec<String>), usize> = BTreeMap::new();
    for t in tuples {
        let args: Vec<String> = t
            .args
            .iter()
            .map(|a| {
                let s = term_display(a);
                if is_null(a) {
                    token[&colour[&s]].clone()
                } else {
                    s
                }
            })
            .collect();
        *ms.entry((t.relation.clone(), args)).or_insert(0) += 1;
    }
    ms
}
