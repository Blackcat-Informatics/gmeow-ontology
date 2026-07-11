// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit + native↔Nemo parity coverage for the reified n-ary lowering.

use std::collections::BTreeSet;

use purrdf::TermValue;

use super::{
    NaryArg, NaryAtom, NaryRule, NaryTuple, canonical_null_blind_multiset,
    certify_nary_termination, is_null, lower_nary_fact, lower_nary_rules, run_native_nary_forward,
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
    let native_ms = canonical_null_blind_multiset(&native);
    let nemo_ms = canonical_null_blind_multiset(&nemo);
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

#[test]
fn el_shaped_multi_arity_curie_recursion_native_agrees_with_nemo() {
    // A COMPACT mirror of the Nemo-KR2024 EL calculus's structure: `@prefix`-CURIE
    // relations of MIXED arity (arity-1 `init`/`isMainClass`, arity-2 `subClassOf`, arity-3
    // `ex`/`exists`) that recurse ACROSS arities and relations (subClassOf ⇒ ex ⇒ init ⇒
    // subClassOf …) over the SHARED `naryArg{i}` reification predicates. This is exactly the
    // shape whose native chase collapsed to zero derivations before the relation-identity
    // fix; it now derives the full closure in EXACT agreement with Nemo (a pure-Datalog
    // program — no value nulls — so the de-reified tuple multisets are identical).
    use super::{run_native_nary_forward, run_nemo_nary_forward};
    use crate::nary_rls::{parse_nary_rls_program, parse_rls_prefixes, resolve_relation_name};

    let rls = "@prefix ex: <http://ex/mini/> .\n\
        ex:init(?C) :- ex:isMainClass(?C) .\n\
        ex:init(?C) :- ex:exq(?E, ?R, ?C) .\n\
        ex:subClassOf(?C, ?C) :- ex:init(?C) .\n\
        ex:subClassOf(?C, ?E) :- ex:subClassOf(?C, ?D), ex:nfSubClassOf(?D, ?E) .\n\
        ex:exq(?E, ?R, ?C) :- ex:subClassOf(?E, ?Y), ex:exists(?Y, ?R, ?C) .\n\
        ex:subClassOf(?E, ?Y) :- ex:exq(?E, ?R, ?C), ex:subClassOf(?C, ?D), \
            ex:subProp(?R, ?S), ex:exists(?Y, ?S, ?D), ex:isSubClass(?Y) .\n";
    let rules = parse_nary_rls_program(rls).expect("parse mini-EL program");
    let prefixes = parse_rls_prefixes(rls);

    // Author the EDB with CURIE stems resolved to the program namespace (the same identity
    // the loader produces from a `ex:isMainClass.csv` file).
    let rel = |curie: &str| resolve_relation_name(curie, &prefixes);
    let a = |s: &str| iri(&format!("http://ex/mini/{s}"));
    let tuple = |curie: &str, args: Vec<TermValue>| NaryTuple {
        relation: rel(curie),
        args,
    };
    let edb = vec![
        tuple("ex:isMainClass", vec![a("A")]),
        tuple("ex:isMainClass", vec![a("B")]),
        tuple("ex:isSubClass", vec![a("Y1")]),
        tuple("ex:exists", vec![a("Y1"), a("r"), a("A")]),
        tuple("ex:subProp", vec![a("r"), a("r")]),
        tuple("ex:nfSubClassOf", vec![a("B"), a("Y1")]),
    ];

    let native = run_native_nary_forward(&edb, &rules).expect("native mini-EL");
    let nemo = run_nemo_nary_forward(&edb, rls).expect("nemo mini-EL");

    // The closure must be NON-VACUOUS across all three derived arities (the recursion fired).
    let derived_arity = |arity: usize| {
        native
            .iter()
            .filter(|t| t.relation.starts_with("http://ex/mini/") && t.args.len() == arity)
            .count()
    };
    assert!(derived_arity(1) > 0, "arity-1 init derived");
    assert!(derived_arity(2) > 0, "arity-2 subClassOf derived");
    let ex_count = native
        .iter()
        .filter(|t| t.relation == "http://ex/mini/exq")
        .count();
    assert!(
        ex_count > 0,
        "arity-3 exq derived (cross-arity recursion fired)"
    );

    // EXACT parity: a pure-Datalog program invents no nulls, so the de-reified multisets
    // are identical between the native reified chase and the Nemo n-ary chase.
    assert_eq!(
        canonical_null_blind_multiset(&native),
        canonical_null_blind_multiset(&nemo),
        "native multi-arity recursive closure must EQUAL Nemo's"
    );
}

// ── Recursive CURIE-program relation-identity parity (regression) ─────────────
//
// A `@prefix`-CURIE recursive n-ary program (ternary transitive `conn` over `link`) whose
// EDB is authored — as the ChaseBench / Nemo-KR2024 corpora are — with CURIE FILE STEMS
// (`ex:link.csv`). The Nemo front-end EXPANDS the rule-atom CURIE `ex:link` to
// `<@prefix ex>link`, so the delimited EDB relation MUST be resolved against the same
// `@prefix` map ([`crate::nary_rls::resolve_relation_name`]) or the reified body atoms
// never join the EDB and the native chase derives NOTHING — a silent completeness collapse
// that the Nemo oracle hides because it re-parses the raw stem through its own front-end.
// This locks in that relation-identity seam end-to-end through the loader.

/// Write `ex:link.csv` (a chain `n0→n1→…→nk` in one world) into a fresh temp dir and return
/// its path; the stem is a CURIE exactly like the real corpora.
fn write_curie_link_csv(chain: usize) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmeow-nary-curie-{}-{chain}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let mut csv = String::new();
    for i in 0..chain {
        csv.push_str(&format!(
            "<http://ex/nary/n{i}>,<http://ex/nary/n{}>,<http://ex/nary/w>\n",
            i + 1
        ));
    }
    let path = dir.join("ex:link.csv");
    std::fs::write(&path, csv).expect("write link csv");
    path
}

/// The recursive CURIE program: `ex:conn` is the transitive closure of `ex:link`, both
/// declared through the `ex:` prefix (never as bracketed absolute IRIs).
fn curie_conn_program() -> String {
    "@prefix ex: <http://ex/nary/> .\n\
     #[name(\"http://ex/rules/base\")]\n\
     ex:conn(?s, ?o, ?w) :- ex:link(?s, ?o, ?w) .\n\
     #[name(\"http://ex/rules/step\")]\n\
     ex:conn(?s, ?o, ?w) :- ex:link(?s, ?m, ?w), ex:conn(?m, ?o, ?w) .\n"
        .to_owned()
}

#[test]
fn recursive_curie_program_native_agrees_with_nemo_via_prefix_resolution() {
    use super::{run_native_nary_forward, run_nemo_nary_forward};
    use crate::nary_rls::{load_nary_data_file, parse_nary_rls_program, parse_rls_prefixes};

    const CONN: &str = "http://ex/nary/conn";
    let rls = curie_conn_program();
    let rules = parse_nary_rls_program(&rls).expect("parse recursive CURIE program");
    let prefixes = parse_rls_prefixes(&rls);
    assert_eq!(
        prefixes.get("ex").map(String::as_str),
        Some("http://ex/nary/"),
        "the @prefix map must carry the ex: declaration"
    );

    let link_csv = write_curie_link_csv(4);

    // THE BUG (no resolution): loading the CURIE stem WITHOUT the program's prefixes names
    // the EDB relation `ex:link`, which never equals the rule's expanded `http://ex/nary/link`
    // — so the native chase derives ZERO conn tuples (a silent completeness collapse).
    let edb_unresolved =
        load_nary_data_file(&link_csv, &std::collections::BTreeMap::new()).expect("load raw stem");
    assert_eq!(edb_unresolved[0].relation, "ex:link");
    let native_unresolved =
        run_native_nary_forward(&edb_unresolved, &rules).expect("native (unresolved)");
    let conn_unresolved = native_unresolved
        .iter()
        .filter(|t| t.relation == CONN)
        .count();
    assert_eq!(
        conn_unresolved, 0,
        "the unresolved-CURIE EDB reproduces the bug: no reified body atom joins, zero derivations"
    );

    // THE FIX (resolution): loading the SAME stem WITH the program's prefixes names the EDB
    // relation `http://ex/nary/link`, matching the rule — the native chase now derives the
    // full transitive closure and AGREES with Nemo.
    let edb = load_nary_data_file(&link_csv, &prefixes).expect("load resolved stem");
    assert_eq!(edb[0].relation, "http://ex/nary/link");

    let native = run_native_nary_forward(&edb, &rules).expect("native (resolved)");
    let nemo = run_nemo_nary_forward(&edb, &rls).expect("nemo");
    let native_conn = native.iter().filter(|t| t.relation == CONN).count();
    let nemo_conn = nemo.iter().filter(|t| t.relation == CONN).count();
    // A 4-edge chain has 4+3+2+1 = 10 conn tuples (transitive closure).
    assert_eq!(
        native_conn, 10,
        "native derives the full transitive closure"
    );
    assert_eq!(
        native_conn, nemo_conn,
        "native reified chase must AGREE with Nemo on the recursive CURIE program"
    );

    if let Some(parent) = link_csv.parent() {
        std::fs::remove_dir_all(parent).ok();
    }
}

// ── Null-blind canonicalization (colour refinement) ───────────────────────────
//
// The `is_null` predicate and the `canonical_null_blind_multiset` colour-refinement
// canonicalization this parity test relies on now live in the parent module (promoted so
// the engine-bench harness computes the SAME cross-engine agreement verdict); they are
// imported above and exercised here as the load-bearing parity oracle.
