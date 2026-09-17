// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed `gmeow prove` protocol controls over tiny owned inputs. The external
//! process is a stub: these tests verify GMEOW selection/projection/report/exit
//! behavior and do not duplicate an external prover's conformance suite.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;

use assert_cmd::Command;

const TINY_TTL: &str = r#"@prefix ex: <https://example.org/> .
ex:subject ex:relation ex:object .
"#;

const TINY_MODEL_TRUE: &str = r#"tff('declare_$i1',type,d0:$i).
tff('finite_domain_$i',axiom,! [X:$i] : (X = d0)).
tff(declare_subject,type,'I|https://example.org/subject':$i).
tff(subject_definition,axiom,'I|https://example.org/subject' = d0).
tff(declare_object,type,'I|https://example.org/object':$i).
tff(object_definition,axiom,'I|https://example.org/object' = d0).
tff(declare_relation,type,'P|2|https://example.org/relation':($i * $i) > $o).
tff(predicate_relation,axiom,'P|2|https://example.org/relation'(d0,d0)).
"#;

const TINY_MODEL_FALSE: &str = r#"tff('declare_$i1',type,d0:$i).
tff('finite_domain_$i',axiom,! [X:$i] : (X = d0)).
tff(declare_subject,type,'I|https://example.org/subject':$i).
tff(subject_definition,axiom,'I|https://example.org/subject' = d0).
tff(declare_object,type,'I|https://example.org/object':$i).
tff(object_definition,axiom,'I|https://example.org/object' = d0).
tff(declare_relation,type,'P|2|https://example.org/relation':($i * $i) > $o).
tff(predicate_relation,axiom,~'P|2|https://example.org/relation'(d0,d0)).
"#;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary")
}

fn source(directory: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
    let path = directory.path().join("source.ttl");
    std::fs::write(&path, content).expect("write source");
    path
}

fn prover(directory: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
    let path = directory.path().join("eprover");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo 'stub-eprover 1.0'\n  exit 0\nfi\n{body}\n"
        ),
    )
    .expect("write prover stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("make prover executable");
    path
}

fn vampire_model_prover(directory: &tempfile::TempDir, model: &str) -> std::path::PathBuf {
    prover(
        directory,
        &format!(
            "case \" $* \" in\n\
               *\" --schedule casc_sat \"*)\n\
            cat <<'GMEOW_FINITE_MODEL'\n\
            % SZS status Satisfiable\n\
            % SZS output start FiniteModel\n\
            {model}\
            % SZS output end FiniteModel\n\
            GMEOW_FINITE_MODEL\n\
            ;;\n\
               *)\n\
            echo '% SZS status Satisfiable'\n\
            ;;\n\
            esac"
        ),
    )
}

fn stdout_json(assert: &assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("typed JSON report")
}

#[test]
fn prove_reports_bound_attestation_without_promoting_it_to_certificate() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(&directory, "echo '% SZS status Satisfiable'");
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "consistent");
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "attestation");
    assert_eq!(report["inputs"].as_array().map(Vec::len), Some(1));
    assert!(
        report["admission"]["units"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(
        report["execution"]["passes"][0]["szs"]["status"],
        "Satisfiable"
    );
    assert!(report["projection"]["problem_digest"].is_string());
    assert!(report["engine"]["executable"]["content_digest"].is_string());
    assert!(report["execution"]["executable"]["content_digest"].is_string());
}

#[test]
fn gate_refuses_unchecked_external_attestation_with_exit_three() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(&directory, "echo '% SZS status Unsatisfiable'");
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
            "--gate",
        ])
        .assert()
        .code(3);
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "inconsistent");
    assert_eq!(report["gate_admission"], "attestation");
}

#[test]
fn checked_vampire_finite_model_is_certificate_grade_and_gate_admissible() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = vampire_model_prover(&directory, TINY_MODEL_TRUE);
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--prover",
            "vampire",
            "--format",
            "json",
            "--gate",
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "consistent");
    assert_eq!(report["evidence_grade"], "certificate");
    assert_eq!(report["gate_admission"], "certificate");
    assert_eq!(report["completeness"], "complete");
    assert_eq!(
        report["execution"]["passes"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        report["execution"]["passes"][1]["artifacts"][0]["check"]["code"],
        "FINITE_MODEL_CHECKED"
    );
    assert!(
        report["metrics"]["decisions"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
}

#[test]
fn false_vampire_model_is_an_execution_failure() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = vampire_model_prover(&directory, TINY_MODEL_FALSE);
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--prover",
            "vampire",
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "execution-failure");
    assert!(report["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["code"] == "FINITE_MODEL_FALSIFIES_PROBLEM")
    }));
}

#[test]
fn structurally_checked_refutation_remains_attestation_grade() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(
        &directory,
        r#"cat <<'GMEOW_PROOF'
% SZS status Unsatisfiable
% SZS output start Proof
fof(input,axiom,'P|2|https://example.org/relation'('I|https://example.org/subject','I|https://example.org/object'),file('problem.p',input)).
cnf(done,plain,$false,inference(unchecked_rule,[],[input])).
% SZS output end Proof
GMEOW_PROOF"#,
    );
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
            "--gate",
        ])
        .assert()
        .code(3);
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "inconsistent");
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "attestation");
    assert_eq!(
        report["execution"]["passes"][0]["artifacts"][0]["check"]["code"],
        "TSTP_STRUCTURE_CHECKED_INFERENCES_UNREPLAYED"
    );
}

#[test]
fn malformed_artifact_framing_is_an_execution_failure() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(
        &directory,
        "echo '% SZS status Unsatisfiable'\n\
         echo '% SZS output start Proof'\n\
         echo 'fof(input,axiom,p(a)).'",
    );
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = stdout_json(&assert);
    assert!(report["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["code"] == "UNTERMINATED_SZS_ARTIFACT")
    }));
}

#[test]
fn conclusive_artifact_must_agree_with_the_szs_outcome() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(
        &directory,
        r#"cat <<'GMEOW_PROOF'
% SZS status Satisfiable
% SZS output start Proof
fof(input,axiom,'P|2|https://example.org/relation'('I|https://example.org/subject','I|https://example.org/object'),file('problem.p',input)).
cnf(done,plain,$false,inference(unchecked_rule,[],[input])).
% SZS output end Proof
GMEOW_PROOF"#,
    );
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = stdout_json(&assert);
    assert!(report["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["code"] == "SZS_ARTIFACT_OUTCOME_MISMATCH")
    }));
}

#[test]
fn contradictory_vampire_pass_outcomes_are_rejected() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(
        &directory,
        "case \" $* \" in\n\
           *\" --schedule casc_sat \"*) echo '% SZS status Unsatisfiable' ;;\n\
           *) echo '% SZS status Satisfiable' ;;\n\
         esac",
    );
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--prover",
            "vampire",
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = stdout_json(&assert);
    assert!(report["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["code"] == "CONFLICTING_PROVER_PASS_OUTCOME")
    }));
}

#[test]
fn unsupported_required_unit_blocks_before_prover_resolution() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(
        &directory,
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:left logic:termEqual ex:right ."#,
    );
    let assert = gmeow()
        .env_remove("GMEOW_PROVER_PATH")
        .env("PATH", directory.path())
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "unsupported");
    assert!(report["execution"].is_null());
    assert!(
        report["projection"]["blockers"]
            .as_array()
            .is_some_and(|rows| rows
                .iter()
                .any(|row| { row["code"] == "PROCEDURAL_RELATION_REQUIRES_BINDING_SEMANTICS" }))
    );
}

#[test]
fn conflicting_szs_status_is_an_execution_failure() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(
        &directory,
        "echo '% SZS status Satisfiable'\necho '# SZS status Unsatisfiable' >&2",
    );
    let assert = gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "execution-failure");
    assert!(report["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["code"] == "CONFLICTING_SZS_STATUS")
    }));
}

#[test]
fn malformed_input_has_its_own_exit_and_machine_report() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let missing = directory.path().join("missing.ttl");
    let assert = gmeow()
        .args([
            "prove",
            missing.to_str().expect("utf8 path"),
            "--format",
            "json",
        ])
        .assert()
        .code(4);
    let report = stdout_json(&assert);
    assert_eq!(report["verdict"], "malformed");
    assert_eq!(report["diagnostics"][0]["code"], "SOURCE_PATH_UNAVAILABLE");
}

#[test]
fn out_and_evidence_files_bind_the_same_problem_receipt() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = source(&directory, TINY_TTL);
    let prover = prover(&directory, "echo '% SZS status GaveUp'");
    let problem = directory.path().join("problem.p");
    let evidence = directory.path().join("evidence.json");
    gmeow()
        .env("GMEOW_PROVER_PATH", prover)
        .args([
            "prove",
            source.to_str().expect("utf8 path"),
            "--out",
            problem.to_str().expect("utf8 path"),
            "--evidence-out",
            evidence.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();
    let problem = std::fs::read_to_string(problem).expect("TPTP problem");
    assert!(problem.contains("% source-digest:"));
    assert!(problem.contains("% program-digest:"));
    assert!(problem.contains("fof(gmeow_"));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(evidence).expect("evidence receipt"))
            .expect("evidence JSON");
    assert_eq!(report["verdict"], "undecided");
    assert_eq!(report["execution"]["passes"][0]["szs"]["status"], "GaveUp");
}
