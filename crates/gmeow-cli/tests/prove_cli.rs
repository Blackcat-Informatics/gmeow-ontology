// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed `gmeow prove` over tiny owned `logic:` inputs. Both engines run
//! in-process; these tests pin the three-valued verdict, the drop ledger, the exit
//! contract, deterministic output and proof replay.

use assert_cmd::Command;

const PREFIXES: &str = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex:    <https://example.org/prove/> .
"#;

/// A disjointness clash spelled in canonical `logic:` vocabulary.
const CLASH: &str = r#"
ex:A logic:disjointWith ex:B .
ex:a logic:instanceOf ex:A , ex:B .
"#;

/// A small Horn theory: a taxonomy, facts and one positive rule.
const HORN: &str = r#"
ex:A logic:subClassOf ex:B .
ex:a logic:instanceOf ex:A .
ex:a ex:knows ex:b .
_:x a logic:TermCarrier ; logic:termVariable "x" .
ex:selfReach a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x ex:reaches _:x )>> ] ;
    logic:body [ rdf:reifies <<( _:x logic:instanceOf ex:B )>> ] .
"#;

/// A disjunctive consequent the native Horn lowering cannot carry.
const DISJUNCTIVE: &str = r#"
ex:a logic:instanceOf ex:A .
ex:law a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
ex:implication a logic:Formula ; logic:antecedent ex:guard ; logic:consequent ex:choice .
ex:guard a logic:Formula ; logic:relation logic:instanceOf ; logic:argument ex:x, ex:classA .
ex:choice a logic:Formula ; logic:or ex:inB, ex:inC .
ex:inB a logic:Formula ; logic:relation logic:instanceOf ; logic:argument ex:x, ex:classB .
ex:inC a logic:Formula ; logic:relation logic:instanceOf ; logic:argument ex:x, ex:classC .
ex:x logic:termIndex 0 ; logic:termVariable "x" .
ex:classA logic:termIndex 1 ; logic:termIri ex:A .
ex:classB logic:termIndex 1 ; logic:termIri ex:B .
ex:classC logic:termIndex 1 ; logic:termIri ex:C .
"#;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary")
}

fn write(directory: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("source directory");
    }
    std::fs::write(&path, format!("{PREFIXES}{body}")).expect("write source");
    path
}

fn path(path: &std::path::Path) -> &str {
    path.to_str().expect("utf8 path")
}

fn report(assert: &assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("JSON report")
}

fn drop_codes(report: &serde_json::Value) -> Vec<String> {
    report["selection"]["drops"]
        .as_array()
        .expect("selection drops")
        .iter()
        .map(|drop| drop["code"].as_str().expect("drop code").to_owned())
        .collect()
}

/// Run `prove --format json` on one owned source and return its report.
fn prove_json(name: &str, body: &str, extra: &[&str]) -> (serde_json::Value, i32) {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), name, body);
    let output = gmeow()
        .args(["prove", path(&source), "--format", "json"])
        .args(extra)
        .output()
        .expect("run prove");
    let report = serde_json::from_slice(&output.stdout).expect("JSON report");
    (report, output.status.code().expect("exit status"))
}

/// A clash derived by the native engine refutes the selection, names the law and
/// its premises, and is the gate's certified negative.
#[test]
fn disjointness_clash_is_inconsistent_and_gate_exits_one() {
    let (report, code) = prove_json("clash.logic.ttl", CLASH, &["--gate"]);
    assert_eq!(code, 1, "{report:#}");
    assert_eq!(report["verdict"], "inconsistent");
    let refutation = &report["refutations"][0];
    assert_eq!(refutation["engine"], "native");
    assert!(
        refutation["law"]
            .as_str()
            .is_some_and(|law| !law.is_empty())
    );
    assert!(
        refutation["premises"]
            .as_array()
            .is_some_and(|premises| !premises.is_empty())
    );
    let (_, code) = prove_json("clash.logic.ttl", CLASH, &[]);
    assert_eq!(code, 0, "reporter mode exits 0 for a decided negative");
}

/// A Horn theory whose every unit is accounted and whose closure completed is
/// `consistent` with a complete, drop-free selection.
#[test]
fn horn_theory_is_consistent_with_zero_drops_and_gate_exits_zero() {
    let (report, code) = prove_json("horn.logic.ttl", HORN, &["--gate"]);
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 0);
    assert_eq!(report["selection"]["complete"], true);
    assert_eq!(report["selection"]["drops"], serde_json::json!([]));
    assert_eq!(
        report["engines"]["native"]["information_state"],
        "supported"
    );
    assert!(
        report["semantics"]
            .as_str()
            .is_some_and(|semantics| semantics.contains("native logic: semantics"))
    );
}

/// A formula the native lowering cannot carry blocks `consistent` and is named.
#[test]
fn uncarried_formula_is_not_decided_with_its_drop_named_and_gate_exits_three() {
    let (report, code) = prove_json("disjunctive.logic.ttl", DISJUNCTIVE, &["--gate"]);
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["selection"]["complete"], false);
    assert!(
        report["selection"]["drops"]
            .as_array()
            .expect("drops")
            .iter()
            .any(|drop| drop["engine"] == "native" && drop["code"] == "UNSUPPORTED_CONSTRUCT"),
        "{report:#}"
    );
}

/// An exhausted derivation allowance is a partial closure and never `consistent`.
#[test]
fn tiny_step_allowance_is_never_consistent() {
    let (report, code) = prove_json("horn.logic.ttl", HORN, &["--max-steps", "1", "--gate"]);
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert!(!drop_codes(&report).is_empty());
}

/// Malformed input has its own exit status and a machine-readable report.
#[test]
fn malformed_input_has_its_own_exit_and_machine_report() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let missing = directory.path().join("missing.ttl");
    let assert = gmeow()
        .args(["prove", path(&missing), "--format", "json"])
        .assert()
        .code(4);
    let missing_report = report(&assert);
    assert_eq!(missing_report["verdict"], "malformed");
    assert_eq!(
        missing_report["diagnostics"][0]["code"],
        "SOURCE_PATH_UNAVAILABLE"
    );

    let source = write(directory.path(), "broken.ttl", "this is not turtle <<<");
    let assert = gmeow()
        .args(["prove", path(&source), "--format", "json", "--gate"])
        .assert()
        .code(4);
    assert_eq!(report(&assert)["verdict"], "malformed");
}

/// `--dir` selects exactly `module.ttl` and `*.logic.ttl`, and a rerun over the same
/// inputs prints identical bytes.
#[test]
fn dir_selects_semantic_sources_only_and_reruns_are_byte_identical() {
    let directory = tempfile::tempdir().expect("scratch directory");
    write(
        directory.path(),
        "slice/module.ttl",
        "ex:A logic:disjointWith ex:B .\nex:a logic:instanceOf ex:A .\n",
    );
    write(
        directory.path(),
        "slice/extra/facts.logic.ttl",
        "ex:b logic:instanceOf ex:B .\n",
    );
    // Selecting this file would complete the clash; the walk must ignore it.
    write(
        directory.path(),
        "slice/other.ttl",
        "ex:a logic:instanceOf ex:B .\n",
    );
    let run = || {
        gmeow()
            .args(["prove", "--dir", path(directory.path()), "--format", "json"])
            .output()
            .expect("run prove")
    };
    let first = run();
    let second = run();
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "two runs must print the same bytes"
    );
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).expect("JSON");
    let inputs: Vec<&str> = report["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .map(|input| input["path"].as_str().expect("path"))
        .collect();
    assert_eq!(inputs.len(), 2, "{inputs:?}");
    assert!(
        inputs
            .iter()
            .any(|input| input.ends_with("slice/module.ttl"))
    );
    assert!(
        inputs
            .iter()
            .any(|input| input.ends_with("facts.logic.ttl"))
    );
    assert_eq!(report["verdict"], "consistent", "{report:#}");
}

/// A recorded tableau proof replays against the same inputs and is rejected
/// against altered input, without a new solver decision.
#[test]
fn proof_replays_on_same_inputs_and_is_rejected_on_altered_input() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "horn.logic.ttl", HORN);
    let proof = directory.path().join("proof.bin");
    let assert = gmeow()
        .args([
            "prove",
            path(&source),
            "--format",
            "json",
            "--proof-out",
            path(&proof),
        ])
        .assert()
        .success();
    let digest = report(&assert)["engines"]["owl_dl"]["proof_digest"].clone();
    assert!(digest.is_string(), "the report carries the proof digest");

    let assert = gmeow()
        .args([
            "prove",
            path(&source),
            "--format",
            "json",
            "--check-proof",
            path(&proof),
        ])
        .assert()
        .code(0);
    let check = report(&assert);
    assert_eq!(check["verdict"], "replayed", "{check:#}");
    assert_eq!(check["proof_digest"], digest);
    // A second reader learns which answer the replayed proof certifies.
    assert_eq!(check["claims"], serde_json::json!(["owl-view-consistent"]));

    write(
        directory.path(),
        "horn.logic.ttl",
        &format!("{HORN}\nex:c logic:instanceOf ex:A .\n"),
    );
    let assert = gmeow()
        .args([
            "prove",
            path(&source),
            "--format",
            "json",
            "--check-proof",
            path(&proof),
        ])
        .assert()
        .code(1);
    assert_eq!(report(&assert)["verdict"], "rejected");
}

/// Assert a counterexample is never `consistent` and that the expected drop is named.
fn assert_not_consistent_with(name: &str, body: &str, code: &str) {
    let (report, exit) = prove_json(name, body, &["--gate"]);
    assert_ne!(report["verdict"], "consistent", "{report:#}");
    assert_ne!(exit, 0, "{report:#}");
    assert!(
        drop_codes(&report).iter().any(|drop| drop == code),
        "expected drop {code}: {report:#}"
    );
}

/// A record-form irreflexive characteristic the engine does not read is a drop,
/// never a silent `consistent`.
#[test]
fn irreflexive_characteristic_record_is_a_named_drop() {
    assert_not_consistent_with(
        "record-irreflexive.logic.ttl",
        r#"
ex:pIrreflexive a logic:PropertyCharacteristicAssertion ;
    logic:characterizes ex:p ;
    logic:characteristicSort logic:irreflexiveProperty .
ex:a ex:p ex:a .
"#,
        "CHARACTERISTIC_RECORD_NOT_READ",
    );
}

/// A record-form asymmetric characteristic the engine does not read is a drop.
#[test]
fn asymmetric_characteristic_record_is_a_named_drop() {
    assert_not_consistent_with(
        "record-asymmetric.logic.ttl",
        r#"
ex:pAsymmetric a logic:PropertyCharacteristicAssertion ;
    logic:characterizes ex:p ;
    logic:characteristicSort logic:asymmetricProperty .
ex:a ex:p ex:b .
ex:b ex:p ex:a .
"#,
        "CHARACTERISTIC_RECORD_NOT_READ",
    );
}

/// Equality between different terms needs substitution the engine does not have.
#[test]
fn equality_between_different_terms_is_a_named_drop() {
    assert_not_consistent_with(
        "sameas-congruence.logic.ttl",
        r#"
_:x a logic:TermCarrier ; logic:termVariable "x" .
ex:selfLoopIsAbsurd a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:x ex:q _:x )>> ] .
ex:a ex:q ex:b .
ex:a logic:sameAs ex:b .
"#,
        "EQUALITY_WITHOUT_SUBSTITUTION",
    );
}

/// A universal restriction is a class-expression law outside the accounting list.
#[test]
fn universal_restriction_is_a_named_drop() {
    let (report, exit) = prove_json(
        "allvaluesfrom-universal.logic.ttl",
        r#"
ex:R logic:onProperty ex:p ;
     logic:allValuesFrom logic:Thing .
ex:R logic:subClassOf logic:Nothing .
"#,
        &["--gate"],
    );
    assert_ne!(report["verdict"], "consistent", "{report:#}");
    assert_ne!(exit, 0);
    assert!(!drop_codes(&report).is_empty(), "{report:#}");
}

/// Named-graph content is outside the decided default graph and is a named drop.
#[test]
fn named_graph_content_is_a_named_drop() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = directory.path().join("named-graph.trig");
    std::fs::write(
        &source,
        format!(
            "{PREFIXES}\nex:A logic:disjointWith ex:B .\nex:a logic:instanceOf ex:A .\nex:g {{\n    ex:a logic:instanceOf ex:B .\n}}\n"
        ),
    )
    .expect("write source");
    let output = gmeow()
        .args(["prove", path(&source), "--format", "json", "--gate"])
        .output()
        .expect("run prove");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_ne!(report["verdict"], "consistent", "{report:#}");
    assert_ne!(output.status.code(), Some(0));
    assert!(
        drop_codes(&report)
            .iter()
            .any(|code| code == "OUTSIDE_SELECTION"),
        "{report:#}"
    );
}

/// The text report states the selection completeness line and one line per drop.
#[test]
fn text_report_lists_selection_completeness_and_each_drop() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "disjunctive.logic.ttl", DISJUNCTIVE);
    let output = gmeow()
        .args(["prove", path(&source)])
        .output()
        .expect("run prove");
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(
        text.lines().any(|line| line == "verdict not-decided"),
        "{text}"
    );
    let drops = text
        .lines()
        .filter(|line| line.starts_with("drop native "))
        .count();
    assert!(drops > 0, "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with("selection incomplete drops=")
                && line.ends_with("(native accounting)")),
        "{text}"
    );
}

/// A file that is not a proof, and a truncated proof, are rejected with exit 1 and a
/// named reason; neither is mistaken for a replay.
#[test]
fn foreign_and_truncated_proof_files_are_rejected() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "horn.logic.ttl", HORN);
    let proof = directory.path().join("proof.bin");
    gmeow()
        .args(["prove", path(&source), "--proof-out", path(&proof)])
        .assert()
        .success();
    let bytes = std::fs::read(&proof).expect("recorded proof");
    let truncated = directory.path().join("truncated.bin");
    std::fs::write(&truncated, &bytes[..bytes.len() / 2]).expect("write truncated proof");
    let foreign = directory.path().join("foreign.bin");
    std::fs::write(&foreign, b"this is not a tableau proof").expect("write foreign file");
    for candidate in [&truncated, &foreign] {
        let assert = gmeow()
            .args([
                "prove",
                path(&source),
                "--format",
                "json",
                "--check-proof",
                path(candidate),
            ])
            .assert()
            .code(1);
        let check = report(&assert);
        assert_eq!(check["verdict"], "rejected", "{check:#}");
        assert!(
            check["diagnostics"][0]["code"]
                .as_str()
                .is_some_and(|code| !code.is_empty()),
            "{check:#}"
        );
    }
}

/// A tableau step cap leaves the OWL leg not decided and reported as such, while
/// the verdict follows the native engine's accounting.
#[test]
fn owl_step_cap_is_reported_on_its_leg_and_the_verdict_follows_the_native_engine() {
    let (report, code) = prove_json("horn.logic.ttl", HORN, &["--step-cap", "0", "--gate"]);
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 0);
    assert_eq!(report["selection"]["complete"], true);
    let owl = &report["engines"]["owl_dl"];
    assert_eq!(owl["state"], "not-decided", "{report:#}");
    assert!(
        owl["notes"].as_array().is_some_and(|notes| notes
            .iter()
            .any(|note| note["code"] == "OWL_SEARCH_NOT_DECIDED")),
        "{report:#}"
    );
    assert!(
        drop_codes(&report).is_empty(),
        "the OWL leg's state is never a selection drop"
    );
}

/// A well-formed constraint whose guard the constraint compiler does not lower (here
/// a disjunction) is skipped with a warning. The source is valid, so the selection is
/// not decided and the skipped constraint is named; it is not malformed input.
#[test]
fn a_constraint_the_compiler_skips_is_not_decided_rather_than_malformed() {
    let (report, code) = prove_json(
        "skipped-constraint.logic.ttl",
        r#"
ex:transitionNamesItsTarget a logic:Constraint ;
    logic:formalizes ex:transitionOf ;
    logic:message "A transition names the thing it changes." ;
    logic:integrity [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ] ;
        logic:forall [ a logic:Formula ; logic:antecedent [ a logic:Formula ; logic:or
                [ a logic:Formula ; logic:relation ex:kind ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Paused ] ],
                [ a logic:Formula ; logic:relation ex:kind ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Resumed ] ] ] ;
            logic:consequent [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "target" ] ; logic:exists [ a logic:Formula ; logic:relation ex:transitionOf ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "target" ] ] ] ] ] .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert!(
        drop_codes(&report)
            .iter()
            .any(|code| code == "SKIPPED_SELECTED_OWNER"),
        "{report:#}"
    );
}

/// An engine that declines a program for a reason it names did not fail to execute.
/// Two rules that feed an existential head back into its own body have no termination
/// certificate, so the native engine refuses them: the selection is not decided, the
/// native leg is reported as refused, and the engine's reasons are the named drops.
#[test]
fn a_program_the_native_engine_declines_is_not_decided_rather_than_a_failure() {
    let (report, code) = prove_json(
        "uncertified-chase.logic.ttl",
        r#"
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:everyoneHasAParent a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x ex:parent _:y )>> ] ;
    logic:body [ rdf:reifies <<( _:x logic:instanceOf ex:Person )>> ] .
_:p a logic:TermCarrier ; logic:termVariable "p" .
_:q a logic:TermCarrier ; logic:termVariable "q" .
ex:parentsArePeople a logic:Rule ;
    logic:head [ rdf:reifies <<( _:q logic:instanceOf ex:Person )>> ] ;
    logic:body [ rdf:reifies <<( _:p ex:parent _:q )>> ] .
ex:a logic:instanceOf ex:Person .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "refused");
    let codes = drop_codes(&report);
    for expected in ["NATIVE_REFUSED", "NATIVE_REFUSAL_REASON"] {
        assert!(codes.iter().any(|code| code == expected), "{report:#}");
    }
}

/// A reasoning-contract declaration is catalogued metatheory: it is accounted, is
/// not a drop, and does not change the verdict of the object theory.
#[test]
fn configuration_declarations_are_accounted_without_a_drop() {
    let (report, code) = prove_json(
        "contracts.logic.ttl",
        r#"
ex:classical a logic:ReasoningContract ; logic:modelSemantics logic:ClassicalSemantics .
ex:subject ex:property ex:object .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 0);
    assert_eq!(report["selection"]["drops"], serde_json::json!([]));
    assert!(
        report["admission"]["units"]
            .as_array()
            .expect("admission units")
            .iter()
            .any(|unit| unit["disposition"]
                .as_object()
                .is_some_and(|disposition| disposition.contains_key("ConfigurationDeclaration"))),
        "{report:#}"
    );
}
