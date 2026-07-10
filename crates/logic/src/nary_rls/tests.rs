// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit coverage for the n-ary `.rls` program parser and the delimited EDB loader.

use std::collections::BTreeSet;

use purrdf::TermValue;

use super::{
    classify_data_file, load_nary_data_file, parse_nary_delimited, parse_nary_rls_program,
};
use crate::nary::{NaryArg, nary_closures_agree, run_native_nary_forward, run_nemo_nary_forward};

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

/// The n-ary multi-head existential demonstrator (bare predicate names): an arity-4 EDB
/// relation `m0` and ONE multi-head TGD inventing two tuples per binding that share a
/// SINGLE existential null `!e` — `m1(?a, !e, ?c) ∧ m2(!e, ?d) ← m0(?a, ?b, ?c, ?d)`.
const DEMO_RLS: &str = "#[name(\"http://ex/nary/rules/split\")]\n\
     m1(?a, !e, ?c), m2(!e, ?d) :- m0(?a, ?b, ?c, ?d) .\n";

fn demo_edb() -> Vec<crate::nary::NaryTuple> {
    (0..2)
        .map(|i| crate::nary::NaryTuple {
            relation: "m0".to_owned(),
            args: vec![
                iri(&format!("http://ex/a{i}")),
                iri(&format!("http://ex/b{i}")),
                iri(&format!("http://ex/c{i}")),
                iri(&format!("http://ex/d{i}")),
            ],
        })
        .collect()
}

// ── Parser: structural shape + existential detection ──────────────────────────

#[test]
fn parser_preserves_full_arity_and_detects_the_shared_existential() {
    let rules = parse_nary_rls_program(DEMO_RLS).expect("demo n-ary .rls must parse");
    assert_eq!(rules.len(), 1, "one rule");
    let rule = &rules[0];
    assert_eq!(rule.name, "http://ex/nary/rules/split");

    // Body atom preserved at FULL arity 4 (no world-slot projection).
    assert_eq!(rule.body.len(), 1);
    assert_eq!(rule.body[0].relation, "m0");
    assert_eq!(rule.body[0].args.len(), 4, "arity-4 body atom preserved");

    // Two head atoms (conjunctive multi-head), arities 3 and 2.
    assert_eq!(rule.head.len(), 2, "two-atom conjunctive head");
    let head_arities: Vec<usize> = rule.head.iter().map(|a| a.args.len()).collect();
    assert_eq!(head_arities, vec![3, 2]);

    // The existential head variable occurs in the head but no body atom, and is SHARED
    // across both head atoms (the mark of a value null, not a Skolem function).
    let body_vars: BTreeSet<&str> = rule
        .body
        .iter()
        .flat_map(|a| a.args.iter())
        .filter_map(|a| match a {
            NaryArg::Var(v) => Some(v.as_str()),
            _ => None,
        })
        .collect();
    let shared: &str = {
        // The var present in BOTH head atoms is the shared existential.
        let h0: BTreeSet<&str> = rule.head[0]
            .args
            .iter()
            .filter_map(|a| match a {
                NaryArg::Var(v) => Some(v.as_str()),
                _ => None,
            })
            .collect();
        let h1: BTreeSet<&str> = rule.head[1]
            .args
            .iter()
            .filter_map(|a| match a {
                NaryArg::Var(v) => Some(v.as_str()),
                _ => None,
            })
            .collect();
        h0.intersection(&h1)
            .copied()
            .next()
            .expect("a shared head var")
    };
    assert!(
        !body_vars.contains(shared),
        "the shared head variable {shared:?} must be existential (bound by no body atom)"
    );
}

/// The SAME parsed program drives BOTH engines to an AGREEING (null-blind) closure — the
/// whole point of a single-source n-ary `.rls`.
#[test]
fn parsed_program_drives_native_and_nemo_to_agreement() {
    let edb = demo_edb();
    let rules = parse_nary_rls_program(DEMO_RLS).expect("parse");

    let native = run_native_nary_forward(&edb, &rules).expect("native n-ary forward");
    let nemo = run_nemo_nary_forward(&edb, DEMO_RLS).expect("nemo n-ary forward");

    assert!(
        nary_closures_agree(&native, &nemo),
        "native and Nemo must agree null-blind on the parsed program.\nnative: {native:#?}\nnemo: {nemo:#?}"
    );

    // Non-vacuity: the agreed closure spans all three relations (EDB m0 + invented m1, m2).
    let rels: BTreeSet<&str> = native.iter().map(|t| t.relation.as_str()).collect();
    assert!(
        rels.contains("m0") && rels.contains("m1") && rels.contains("m2"),
        "closure must span m0, m1, m2: {rels:?}"
    );
}

// ── Parser refusals (named, never mis-parsed) ─────────────────────────────────

#[test]
fn parser_refuses_a_negated_body_literal() {
    let rls = "#[name(\"http://ex/neg\")]\n\
         q(?a, ?b) :- p(?a, ?b), ~r(?a, ?b) .\n";
    let err = parse_nary_rls_program(rls).expect_err("a negated body literal must be refused");
    assert!(
        err.message().contains("negated body literal"),
        "the refusal must name the negation: {err}"
    );
    assert!(!err.message().contains('#'), "no process refs: {err}");
}

#[test]
fn parser_refuses_a_body_operation_guard() {
    // An inequality guard is a body OPERATION the fixed-arity n-ary atom cannot carry.
    let rls = "#[name(\"http://ex/op\")]\n\
         q(?a, ?b) :- p(?a, ?b), ?a != ?b .\n";
    let err = parse_nary_rls_program(rls).expect_err("a body operation must be refused");
    assert!(
        err.message().contains("body operation"),
        "the refusal must name the operation: {err}"
    );
}

#[test]
fn parser_refuses_a_skolem_function_existential() {
    // `!e` occurs in a SINGLE head atom and no body atom — a Skolem-function obligation the
    // reified lowering refuses; the parser runs that lowering at parse time, so it fires here.
    let rls = "#[name(\"http://ex/skolem\")]\n\
         m1(?a, !e, ?c) :- m0(?a, ?b, ?c, ?d) .\n";
    let err =
        parse_nary_rls_program(rls).expect_err("a Skolem-function existential must be refused");
    assert!(
        err.message().contains("Skolem-function"),
        "the refusal must name the Skolem-function obligation: {err}"
    );
}

// ── Delimited EDB loader ──────────────────────────────────────────────────────

#[test]
fn csv_loader_parses_uniform_arity_tuples() {
    let csv = b"a0,b0,c0,d0\na1,b1,c1,d1\n";
    let tuples = parse_nary_delimited("edge", csv, b',').expect("csv parse");
    assert_eq!(tuples.len(), 2);
    assert!(tuples.iter().all(|t| t.relation == "edge"));
    assert!(tuples.iter().all(|t| t.args.len() == 4), "arity-4 rows");
    // Bare tokens become simple string literals.
    assert_eq!(tuples[0].args[0], TermValue::simple_literal("a0"));
}

#[test]
fn csv_loader_reads_angle_bracketed_iris() {
    let csv = b"<http://ex/a0>,<http://ex/b0>\n";
    let tuples = parse_nary_delimited("rel", csv, b',').expect("csv parse");
    assert_eq!(tuples[0].args[0], TermValue::iri("http://ex/a0"));
}

#[test]
fn tsv_loader_splits_on_tabs() {
    let tsv = b"a0\tb0\tc0\n";
    let tuples = parse_nary_delimited("rel", tsv, b'\t').expect("tsv parse");
    assert_eq!(tuples[0].args.len(), 3);
}

#[test]
fn csv_loader_hard_fails_on_non_uniform_arity() {
    let csv = b"a0,b0,c0,d0\na1,b1\n";
    let err = parse_nary_delimited("edge", csv, b',')
        .expect_err("a non-uniform-arity file must hard-fail");
    assert!(err.message().contains("non-uniform arity"), "{err}");
}

#[test]
fn csv_loader_hard_fails_on_empty_relation() {
    let err = parse_nary_delimited("edge", b"", b',').expect_err("an empty file must hard-fail");
    assert!(err.message().contains("zero data rows"), "{err}");
}

#[test]
fn classify_maps_extension_to_relation_and_delimiter() {
    assert_eq!(
        classify_data_file("edge.csv").unwrap(),
        ("edge".to_owned(), b',', false)
    );
    assert_eq!(
        classify_data_file("edge.tsv").unwrap(),
        ("edge".to_owned(), b'\t', false)
    );
    assert_eq!(
        classify_data_file("edge.csv.gz").unwrap(),
        ("edge".to_owned(), b',', true)
    );
    assert!(
        classify_data_file("edge.parquet").is_err(),
        "unknown extension hard-fails"
    );
}

#[test]
fn gzip_data_file_round_trips_through_the_loader() {
    use std::io::Write;

    let plain = b"a0,b0,c0,d0\na1,b1,c1,d1\n";
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(plain).expect("gzip write");
    let gz = encoder.finish().expect("gzip finish");

    let dir = std::env::temp_dir().join(format!("gmeow-nary-gz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("edge.csv.gz");
    std::fs::write(&path, &gz).expect("write gz");

    let from_gz = load_nary_data_file(&path).expect("load gz");
    let from_plain = parse_nary_delimited("edge", plain, b',').expect("plain parse");
    assert_eq!(
        from_gz, from_plain,
        "gzip and plain must decode identically"
    );

    std::fs::remove_dir_all(&dir).ok();
}
