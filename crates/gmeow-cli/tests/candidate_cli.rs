// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary exposes the candidate
//! propose/verify seam — `gmeow candidate submit | withdraw | list` — through the real
//! `Cli`/`Commands::Candidate` clap dispatch in `src/lib.rs`, sharing the same
//! `gmeow_pipeline::mcp` cores the MCP tools run. Drives the built binary through
//! `assert_cmd` against a temp `GMEOW_CANDIDATE_PATH` library.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// A reified ground-atom candidate `rdf:type(ex:a, ex:B)` — CORROBORATED by `corroborating_kb`.
fn corroborating_formula() -> &'static str {
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:phi a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n"
}

/// A KB asserting the fact the candidate names — so the candidate is entailed ⇒ corroborated.
fn corroborating_kb() -> &'static str {
    "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:a rdf:type ex:B .\n"
}

/// A ∀-Horn candidate whose head class is DISJOINT with the individual's type ⇒ refuted.
fn refuting_formula() -> &'static str {
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:cand a logic:Formula ;\n\
         logic:forall ex:body ;\n\
         logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
     ex:body a logic:Formula ;\n\
         logic:antecedent ex:ant ;\n\
         logic:consequent ex:con .\n\
     ex:ant a logic:Formula ;\n\
         logic:relation ex:trigger ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
     ex:con a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n"
}

fn refuting_kb() -> &'static str {
    "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     ex:a ex:trigger ex:mark .\n\
     ex:a rdf:type ex:A .\n\
     ex:A owl:disjointWith ex:B .\n"
}

/// Stage the two TTL fixtures + a candidate-library path into a fresh temp dir.
fn staged(formula: &str, kb: &str) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let fpath = tmp.path().join("formula.ttl");
    let kpath = tmp.path().join("kb.ttl");
    let store = tmp.path().join("candidates.gts");
    fs::write(&fpath, formula).expect("write formula");
    fs::write(&kpath, kb).expect("write kb");
    (tmp, fpath, kpath, store)
}

/// Candidate lifecycle records remain recoverable when standalone artifact publication fails.
#[test]
fn candidate_submit_list_withdraw_full_lifecycle() {
    let (directory, formula, kb, store) = staged(corroborating_formula(), corroborating_kb());
    let standpoint = "http://ex/standpoint/alice";

    // list on an empty (absent) library is an empty list, not an error.
    gmeow()
        .args(["candidate", "list"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"candidate_count\": 0"));

    // submit an ADMISSIBLE (corroborated) candidate — it commits.
    let submit = gmeow()
        .args(["candidate", "submit", "--out"])
        .arg(store.with_extension("verdict.gts"))
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args([
            "--standpoint",
            standpoint,
            "--for-slice",
            "http://ex/slices/demo",
        ])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success()
        .stdout(predicate::str::contains("lifecycle corroborated"))
        .stdout(predicate::str::contains("admissible true"))
        .stdout(predicate::str::contains("persisted committed"));
    assert!(store.exists(), "the admissible candidate was appended");
    let bytes = fs::read(store.with_extension("verdict.gts"))
        .expect("complete candidate evaluation artifact");
    let result = gmeow_logic::result_rdf::parse_reasoning_artifact(&bytes)
        .expect("admitted persisted candidate result");
    assert_eq!(
        result.information,
        gmeow_logic::result::InformationState::Supported
    );

    // Extract the candidate node IRI from stdout ("candidate <iri>").
    let out = submit.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let node = stdout
        .lines()
        .find_map(|l| l.strip_prefix("candidate "))
        .expect("submit prints the candidate IRI")
        .trim()
        .to_string();

    // list shows it in-library with its provenance.
    gmeow()
        .args(["candidate", "list"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"candidate_count\": 1"))
        .stdout(predicate::str::contains("\"disposition\": \"in-library\""))
        .stdout(predicate::str::contains("http://ex/slices/demo"));

    // withdraw it — succeeds and supersedes.
    gmeow()
        .args(["candidate", "withdraw", "--candidate-id", &node])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success();

    gmeow()
        .args(["candidate", "list"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"disposition\": \"withdrawn\""));

    // withdrawing again (already withdrawn) hard-fails.
    gmeow()
        .args(["candidate", "withdraw", "--candidate-id", &node])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .failure();

    // A failed standalone export still identifies the candidate that was committed.
    let failed_export_store = directory.path().join("persisted-without-export.gts");
    let failed = gmeow()
        .args(["candidate", "submit", "--out"])
        .arg(directory.path())
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", standpoint])
        .env("GMEOW_CANDIDATE_PATH", &failed_export_store)
        .assert()
        .failure()
        .stderr(predicate::str::contains("committed=true; dry_run=false"));
    let diagnostic = String::from_utf8_lossy(&failed.get_output().stderr);
    let saved_candidate = diagnostic
        .split_once("; candidate ")
        .and_then(|(_, fields)| fields.split_once("; committed="))
        .map(|(identity, _)| identity)
        .expect("an export failure reports the evaluated candidate identity");
    gmeow()
        .args(["candidate", "list"])
        .env("GMEOW_CANDIDATE_PATH", &failed_export_store)
        .assert()
        .success()
        .stdout(predicate::str::contains(saved_candidate));
}

#[test]
fn candidate_submit_refuted_hard_fails_and_writes_nothing() {
    let (_tmp, formula, kb, store) = staged(refuting_formula(), refuting_kb());
    gmeow()
        .args(["candidate", "submit", "--out"])
        .arg(store.with_extension("verdict.gts"))
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .failure()
        .stdout(predicate::str::contains("lifecycle refuted-in-standpoint"))
        .stdout(predicate::str::contains("admissible false"));
    assert!(
        !Path::new(&store).exists(),
        "a refuted candidate must write nothing to the library"
    );
    let bytes = fs::read(store.with_extension("verdict.gts"))
        .expect("refused admission retains the complete evaluation");
    let result = gmeow_logic::result_rdf::parse_reasoning_artifact(&bytes)
        .expect("admitted refuted candidate evaluation");
    assert_eq!(
        result.information,
        gmeow_logic::result::InformationState::Opposed
    );
}

/// A dry-run candidate publishes its artifact while preserving the selected library,
/// including refusal when the requested artifact would replace that library.
#[test]
fn candidate_submit_dry_run_writes_nothing() {
    let (_tmp, formula, kb, store) = staged(corroborating_formula(), corroborating_kb());
    gmeow()
        .args(["candidate", "submit", "--out"])
        .arg(store.with_extension("verdict.gts"))
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice", "--dry-run"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .success()
        .stdout(predicate::str::contains("persisted dry-run"));
    assert!(
        !Path::new(&store).exists(),
        "a dry-run submit must write nothing to the library"
    );
    let bytes = fs::read(store.with_extension("verdict.gts"))
        .expect("dry run publishes its complete evaluation");
    let result = gmeow_logic::result_rdf::parse_reasoning_artifact(&bytes)
        .expect("admitted dry-run candidate evaluation");
    assert_eq!(
        result.information,
        gmeow_logic::result::InformationState::Supported
    );
    fs::write(&store, b"preserved candidate history").unwrap();
    gmeow()
        .args(["candidate", "submit", "--out"])
        .arg(&store)
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice", "--dry-run"])
        .env("GMEOW_CANDIDATE_PATH", &store)
        .assert()
        .failure()
        .stderr(predicate::str::contains("would replace the active library"));
    assert_eq!(fs::read(&store).unwrap(), b"preserved candidate history");
}
