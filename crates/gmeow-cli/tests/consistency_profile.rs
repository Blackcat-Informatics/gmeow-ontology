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

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("typed reasoning JSON")
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
fn consistency_retains_a_constructor_no_model_as_a_negative_verdict() {
    let (_d, _p, out) = run(
        "consistency",
        &["--format", "json"],
        REASONER_OPEN_FAILURE_TTL,
    );
    assert!(
        out.status.success(),
        "reporter mode retains the decided negative result: {out:?}"
    );
    let report = json(&out);
    assert_eq!(report["verdict"], "false");
    assert_eq!(report["decision"], "negative");
    assert_eq!(report["evidence_grade"], "certificate");
    assert_eq!(report["result"]["has_model"], false);
    assert_eq!(report["diagnostics"][0]["code"], "ONTOLOGY_HAS_NO_MODEL");
}

#[test]
fn consistency_json_carries_the_shared_identity_and_evidence_contract() {
    let (_d, _p, out) = run("consistency", &["--format", "json"], CONSISTENT_TTL);
    assert!(out.status.success(), "exact consistency report: {out:?}");
    let report = json(&out);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["operation"], "consistency");
    assert_eq!(report["verdict"], "true");
    assert_eq!(report["decision"], "positive");
    assert_eq!(report["input_status"], "valid");
    assert_eq!(report["evaluation_status"], "completed");
    assert_eq!(report["completeness"], "complete-for-fragment");
    assert_eq!(report["information_state"], "supported");
    assert_eq!(report["evidence_grade"], "certificate");
    assert_eq!(report["gate_admission"], "certificate");
    assert_eq!(report["result"]["kind"], "consistency");
    assert_eq!(report["result"]["has_model"], true);
    assert_eq!(report["inputs"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        report["inputs"][0]["content_digest"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(
        report["program"]["content_digest"].as_str().map(str::len),
        Some(64)
    );
    assert!(report["engine"]["version"].is_string());
    assert_eq!(report["engine"]["implementation"], "purrdf-owl-dl");
    assert!(report["metrics"]["decisions"].is_number());
}

#[test]
fn consistency_gate_distinguishes_positive_negative_and_undecided() {
    let (_d, _p, positive) = run("consistency", &["--gate"], CONSISTENT_TTL);
    assert!(
        positive.status.success(),
        "certificate admits: {positive:?}"
    );

    let (_d, _p, negative) = run("consistency", &["--gate"], INCONSISTENT_TTL);
    assert_eq!(
        negative.status.code(),
        Some(1),
        "refutation exit: {negative:?}"
    );

    let (_d, _p, undecided) = run(
        "consistency",
        &["--step-cap", "1", "--gate", "--format", "json"],
        INCONSISTENT_TTL,
    );
    assert_eq!(
        undecided.status.code(),
        Some(3),
        "bounded answer cannot admit a gate: {undecided:?}"
    );
    let report = json(&undecided);
    assert_eq!(report["decision"], "undecided");
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "attestation");
}

#[test]
fn malformed_reasoning_json_uses_the_dedicated_exit_and_report_shape() {
    let (_d, _p, out) = run("consistency", &["--format", "json"], MALFORMED_TTL);
    assert_eq!(out.status.code(), Some(4), "malformed input exit: {out:?}");
    let report = json(&out);
    assert_eq!(report["decision"], "malformed");
    assert_eq!(report["input_status"], "invalid");
    assert_eq!(report["gate_admission"], "refused");
    assert_eq!(report["diagnostics"][0]["code"], "INPUT_PARSE_FAILED");
}

#[test]
fn entailment_gate_distinguishes_entailment_countermodel_and_capability_gap() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let premise = directory.path().join("premise.ttl");
    let entailed = directory.path().join("entailed.ttl");
    let not_entailed = directory.path().join("not-entailed.ttl");
    let gap = directory.path().join("gap.ttl");
    std::fs::write(
        &premise,
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix ex: <http://gmeow.example/> .\nex:x rdf:type ex:A .\n",
    )
    .expect("premise");
    std::fs::write(
        &entailed,
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix ex: <http://gmeow.example/> .\nex:x rdf:type ex:A .\n",
    )
    .expect("entailed conclusion");
    std::fs::write(
        &not_entailed,
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix ex: <http://gmeow.example/> .\nex:x rdf:type ex:B .\n",
    )
    .expect("countermodel conclusion");
    std::fs::write(
        &gap,
        "@prefix ex: <http://gmeow.example/> .\nex:x ex:knows ex:y .\n",
    )
    .expect("unsupported conclusion");

    let invoke = |conclusion: &std::path::Path| {
        gmeow()
            .args([
                "entails",
                premise.to_str().expect("utf8 premise"),
                conclusion.to_str().expect("utf8 conclusion"),
                "--format",
                "json",
                "--gate",
            ])
            .output()
            .expect("run entailment")
    };

    let positive = invoke(&entailed);
    assert!(positive.status.success(), "entailed gate: {positive:?}");
    let positive_report = json(&positive);
    assert_eq!(positive_report["operation"], "entailment");
    assert_eq!(positive_report["verdict"], "entailed");
    assert_eq!(positive_report["gate_admission"], "certificate");
    assert!(positive_report["metrics"]["decisions"].is_number());
    assert!(positive_report["metrics"]["steps"].is_number());
    assert_eq!(positive_report["engine"]["implementation"], "gmeow-logic");

    let negative = invoke(&not_entailed);
    assert_eq!(
        negative.status.code(),
        Some(1),
        "countermodel gate: {negative:?}"
    );
    let negative_report = json(&negative);
    assert_eq!(negative_report["verdict"], "not-entailed");
    assert_eq!(negative_report["decision"], "negative");

    let unsupported = invoke(&gap);
    assert_eq!(
        unsupported.status.code(),
        Some(3),
        "capability gap: {unsupported:?}"
    );
    let unsupported_report = json(&unsupported);
    assert_eq!(unsupported_report["verdict"], "gap");
    assert_eq!(unsupported_report["decision"], "unsupported");
    assert_eq!(unsupported_report["result"]["gap_shape"], "role-assertion");
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
    let (_d, _p, out) = run(
        "classify",
        &["--format", "json", "--gate"],
        INCONSISTENT_TTL,
    );
    assert_eq!(out.status.code(), Some(1), "typed negative exit: {out:?}");
    let report = json(&out);
    assert_eq!(report["verdict"], "no-model");
    assert_eq!(report["decision"], "negative");
    assert_eq!(report["diagnostics"][0]["code"], "ONTOLOGY_HAS_NO_MODEL");
}

#[test]
fn classification_json_retains_the_operation_specific_answer_under_the_common_contract() {
    let (_d, _p, out) = run("classify", &["--format", "json", "--gate"], CHAIN_TTL);
    assert!(out.status.success(), "exact classification admits: {out:?}");
    let report = json(&out);
    assert_eq!(report["operation"], "classification");
    assert_eq!(report["verdict"], "classified");
    assert_eq!(report["gate_admission"], "certificate");
    assert_eq!(report["result"]["kind"], "classification");
    assert!(
        report["result"]["subsumptions"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["left"] == "http://gmeow.example/A" && row["right"] == "http://gmeow.example/C"
            }))
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
    let (_d, _p, out) = run("realize", &["--format", "json", "--gate"], INCONSISTENT_TTL);
    assert_eq!(out.status.code(), Some(1), "typed negative exit: {out:?}");
    let report = json(&out);
    assert_eq!(report["verdict"], "no-model");
    assert_eq!(report["decision"], "negative");
    assert_eq!(report["diagnostics"][0]["code"], "ONTOLOGY_HAS_NO_MODEL");
}

#[test]
fn realization_json_uses_the_shared_contract_and_retains_type_rows() {
    let (_d, _p, out) = run("realize", &["--format", "json", "--gate"], CONSISTENT_TTL);
    assert!(out.status.success(), "exact realization admits: {out:?}");
    let report = json(&out);
    assert_eq!(report["operation"], "realization");
    assert_eq!(report["verdict"], "realized");
    assert_eq!(report["evidence_grade"], "certificate");
    assert_eq!(report["result"]["kind"], "realization");
    assert!(
        report["result"]["types"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["left"] == "http://gmeow.example/x" && row["right"] == "http://gmeow.example/B"
            }))
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
