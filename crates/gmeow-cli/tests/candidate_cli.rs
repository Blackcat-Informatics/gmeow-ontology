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

#[test]
fn candidate_submit_list_withdraw_full_lifecycle() {
    let (_tmp, formula, kb, store) = staged(corroborating_formula(), corroborating_kb());
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
        .args(["candidate", "submit", "--formula"])
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
}

#[test]
fn candidate_submit_refuted_hard_fails_and_writes_nothing() {
    let (_tmp, formula, kb, store) = staged(refuting_formula(), refuting_kb());
    gmeow()
        .args(["candidate", "submit", "--formula"])
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
}

#[test]
fn candidate_submit_dry_run_writes_nothing() {
    let (_tmp, formula, kb, store) = staged(corroborating_formula(), corroborating_kb());
    gmeow()
        .args(["candidate", "submit", "--formula"])
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
        "a dry-run submit must write nothing"
    );
}
