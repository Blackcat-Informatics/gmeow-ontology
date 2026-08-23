// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end acceptance for the five `gmeow` DL-service subcommands — `consistency`,
//! `profile`, `classify`, `realize`, and `module` — the public façade over
//! `gmeow_logic::reasoner_services`, driven through the built binary via `assert_cmd`. Each
//! test pins the split the CLI promises: the greppable product on stdout, diagnostics on
//! stderr, and the honest exit code.
//!
//! Covered paths: a decided `true`/`false` consistency verdict (exit 0), the honest
//! `unknown` verdict under a narrowed `--step-cap` (exit 0 — never a fabricated verdict),
//! command-specific parse diagnostics on malformed input (non-zero), a reasoner-open
//! failure (exit 1), the `certified`/`violation` profile surfaces, the classification
//! hierarchy (transitive subsumptions + direct reduction, hard-fail on no model), the
//! realization types (told + entailed, hard-fail on no model), and locality-module
//! extraction (method/axioms/signature, explicit-notion selection, hard-fail on an unknown
//! `--method`).

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

/// A tiny consistent ontology: `x : A`, `A ⊑ B`. It has a model, and its only construct
/// (an atomic `rdfs:subClassOf`) is inside every OWL 2 profile.
const CONSISTENT_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:x rdf:type ex:A .
";

/// An entailed inconsistency: `x : A`, `A ⊑ B`, `A ⊑ C`, `B ⊐⊏ C` forces `x` into
/// `owl:Nothing` (mirrors `verify_deep.rs`'s `INCONSISTENT_TTL`).
const INCONSISTENT_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";

/// `owl:complementOf` — a full negation constructor that is NOT in the OWL 2 EL / QL / RL
/// profiles, so certification reports a violation for those while DL / Full accept it.
const COMPLEMENT_TTL: &str = "\
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A a owl:Class ; owl:complementOf ex:B .
ex:B a owl:Class .
";

/// Syntactically broken Turtle: a subject/predicate with no object and no terminator.
const MALFORMED_TTL: &str = "\
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf
";

/// An `owl:hasKey` axiom over an ontology that is ALREADY unsatisfiable (`A ⊑ B`, `A ⊑ C`,
/// `B ⊐⊏ C`, `x : A`). The reverse mapping reasons over the key while opening the reasoner
/// and finds the ontology has no model, so it refuses — every service EXCEPT `consistency`
/// (which is defined to detect that) then reports a reasoner-open failure rather than a
/// verdict. This is the documented `DlReasoner::new` error path.
const REASONER_OPEN_FAILURE_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
ex:A owl:hasKey ( ex:p ) .
";

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Write `ttl` to a temp `.ttl` file and run `gmeow <subcommand> [extra…] <file>`, returning
/// the process output (the caller keeps the `TempDir` alive).
fn run(subcommand: &str, extra: &[&str], ttl: &str) -> (tempfile::TempDir, PathBuf, Output) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let path = dir.path().join("ontology.ttl");
    std::fs::write(&path, ttl).expect("write fixture ontology");
    let mut cmd = gmeow();
    cmd.arg(subcommand);
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.arg(&path).output().expect("run gmeow");
    (dir, path, output)
}

// ── consistency ───────────────────────────────────────────────────────────────────────

#[test]
fn consistency_reports_true_and_exits_zero_on_a_consistent_ontology() {
    let (_d, _p, out) = run("consistency", &[], CONSISTENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "a consistent ontology exits 0: {out:?}"
    );
    assert!(
        stdout.contains("verdict true"),
        "a model-carrying ontology is `true`: {stdout}"
    );
    assert!(
        stdout.contains("completeness"),
        "the run's completeness is disclosed, never flattened: {stdout}"
    );
}

#[test]
fn consistency_reports_false_and_exits_zero_on_an_inconsistent_ontology() {
    let (_d, _p, out) = run("consistency", &[], INCONSISTENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Consistency is the one service that DETECTS unsatisfiability as a verdict rather than
    // erroring: `false` with a clean exit, not a reasoner-open failure.
    assert!(
        out.status.success(),
        "an inconsistent ontology is a successful `false` answer (exit 0): {out:?}"
    );
    assert!(
        stdout.contains("verdict false"),
        "the entailed clash makes the ontology unsatisfiable: {stdout}"
    );
}

#[test]
fn consistency_reports_unknown_and_exits_zero_under_a_narrowed_step_cap() {
    // A `--step-cap 1` exhausts the tableau budget before it can decide, so the honest
    // three-valued answer is `unknown` — never a fabricated `true`/`false`. It is still a
    // successful, well-formed answer, so the exit code is 0.
    let (_d, _p, out) = run("consistency", &["--step-cap", "1"], INCONSISTENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "an honest `unknown` verdict is a successful answer (exit 0): {out:?}"
    );
    assert!(
        stdout.contains("verdict unknown"),
        "a budget-exhausted decision reports `unknown`, not a guess: {stdout}"
    );
}

#[test]
fn consistency_hard_fails_with_a_command_specific_diagnostic_on_malformed_input() {
    let (_d, _p, out) = run("consistency", &[], MALFORMED_TTL);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "unparsable input is a hard fail, never a silent empty answer: {out:?}"
    );
    assert!(
        stderr.contains("gmeow-cli.consistency"),
        "the parse diagnostic is scoped to the consistency command: {stderr}"
    );
}

#[test]
fn consistency_reports_a_reasoner_open_failure_when_reverse_mapping_refuses() {
    let (_d, _p, out) = run("consistency", &[], REASONER_OPEN_FAILURE_TTL);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "an ontology the DL reverse mapping refuses to open is a hard fail: {out:?}"
    );
    assert!(
        stderr.contains("cannot open the DL reasoner"),
        "the reasoner-open failure is surfaced, not swallowed: {stderr}"
    );
}

// ── profile ───────────────────────────────────────────────────────────────────────────

#[test]
fn profile_prints_certified_membership_for_a_profile_conformant_ontology() {
    let (_d, _p, out) = run("profile", &[], CONSISTENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "profile certification exits 0: {out:?}"
    );
    assert!(
        stdout.contains("certified "),
        "an atomic subclass ontology is certified in the tractable profiles: {stdout}"
    );
}

#[test]
fn profile_prints_a_violation_for_an_out_of_profile_construct() {
    let (_d, _p, out) = run("profile", &[], COMPLEMENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "profile certification exits 0: {out:?}"
    );
    assert!(
        stdout.contains("violation "),
        "owl:complementOf is out of EL/QL/RL, so a violation is reported: {stdout}"
    );
}

#[test]
fn profile_hard_fails_with_a_command_specific_diagnostic_on_malformed_input() {
    let (_d, _p, out) = run("profile", &[], MALFORMED_TTL);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "unparsable input is a hard fail: {out:?}"
    );
    assert!(
        stderr.contains("gmeow-cli.profile"),
        "the parse diagnostic is scoped to the profile command: {stderr}"
    );
}

// ── classify ──────────────────────────────────────────────────────────────────────────

/// A two-link subclass chain `A ⊑ B ⊑ C`. Its transitive closure entails `A ⊑ C`, and its
/// transitive reduction keeps only the two direct links — the difference the classifier prints.
const CHAIN_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:B rdfs:subClassOf ex:C .
";

/// The seed class IRI used by the module tests (a class in `CHAIN_TTL`).
const SEED_A: &str = "http://gmeow.example/A";

#[test]
fn classify_prints_the_transitive_subsumptions_and_the_direct_reduction() {
    let (_d, _p, out) = run("classify", &[], CHAIN_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "classification exits 0: {out:?}");
    // The transitive closure establishes A ⊑ C even though it was never told.
    assert!(
        stdout.contains("subsumption http://gmeow.example/A http://gmeow.example/C"),
        "the entailed transitive subsumption A ⊑ C is printed: {stdout}"
    );
    // The reduction keeps only the direct link A ⊑ B (not A ⊑ C).
    assert!(
        stdout.contains("direct http://gmeow.example/A http://gmeow.example/B"),
        "the transitive reduction lists the direct subsumer: {stdout}"
    );
    assert!(
        stdout.contains("completeness"),
        "the run's completeness is disclosed: {stdout}"
    );
}

#[test]
fn classify_hard_fails_on_an_unsatisfiable_ontology() {
    // An ontology with no model has no meaningful hierarchy: classification refuses rather
    // than emitting an empty (and misleading) answer.
    let (_d, _p, out) = run("classify", &[], INCONSISTENT_TTL);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "classifying an unsatisfiable ontology is a hard fail: {out:?}"
    );
    assert!(
        stderr.contains("gmeow-cli.classify"),
        "the diagnostic is scoped to the classify command: {stderr}"
    );
}

// ── realize ───────────────────────────────────────────────────────────────────────────

#[test]
fn realize_prints_the_entailed_types_of_named_individuals() {
    // `x : A` with `A ⊑ B` entails `x : B` — the realization surfaces the told AND the
    // entailed type.
    let (_d, _p, out) = run("realize", &[], CONSISTENT_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "realization exits 0: {out:?}");
    assert!(
        stdout.contains("type http://gmeow.example/x http://gmeow.example/A"),
        "the told type x : A is printed: {stdout}"
    );
    assert!(
        stdout.contains("type http://gmeow.example/x http://gmeow.example/B"),
        "the entailed type x : B is printed: {stdout}"
    );
}

#[test]
fn realize_hard_fails_on_an_unsatisfiable_ontology() {
    let (_d, _p, out) = run("realize", &[], INCONSISTENT_TTL);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "realizing an unsatisfiable ontology is a hard fail: {out:?}"
    );
    assert!(
        stderr.contains("gmeow-cli.realize"),
        "the diagnostic is scoped to the realize command: {stderr}"
    );
}

// ── module ────────────────────────────────────────────────────────────────────────────

#[test]
fn module_extracts_a_locality_module_for_a_seed_signature() {
    let (_d, _p, out) = run("module", &["--seed", SEED_A], CHAIN_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "module extraction exits 0: {out:?}");
    // Default notion is the nested ⊥⊤* module.
    assert!(
        stdout.contains("method STAR"),
        "the default locality notion is STAR: {stdout}"
    );
    assert!(
        stdout.contains("axioms "),
        "the kept-axiom count is printed: {stdout}"
    );
    // The signature the fixpoint closed to includes the seed itself.
    assert!(
        stdout.contains(&format!("signature {SEED_A}")),
        "the closed signature lists the seed class: {stdout}"
    );
}

#[test]
fn module_honors_an_explicit_locality_method() {
    let (_d, _p, out) = run("module", &["--seed", SEED_A, "--method", "bot"], CHAIN_TTL);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "module extraction exits 0: {out:?}");
    assert!(
        stdout.contains("method BOT"),
        "the explicitly selected ⊥ notion is used, not the STAR default: {stdout}"
    );
}

#[test]
fn module_hard_fails_on_an_unknown_method() {
    // No-optionality: an unknown notion is a hard fail, never a silent fallback to a default.
    let (_d, _p, out) = run(
        "module",
        &["--seed", SEED_A, "--method", "bogus"],
        CHAIN_TTL,
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "an unknown module method is a hard fail: {out:?}"
    );
    assert!(
        stderr.contains("gmeow-cli.module.method"),
        "the diagnostic names the unknown-method fault: {stderr}"
    );
}
