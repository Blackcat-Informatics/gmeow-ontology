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

/// Two reports from different sources that type one individual into two disjoint
/// classes.
const PROVENANCE_CLASH: &str = r#"
ex:A logic:disjointWith ex:B .
[] rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:provenance ex:first .
[] rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:provenance ex:second .
"#;

/// A claim that carries only provenance is a statement of the selected theory, so
/// two such claims that contradict each other refute the selection even though
/// source admission has not admitted either one.
#[test]
fn provenance_only_contradiction_is_inconsistent() {
    let (report, code) = prove_json("provenance-clash.logic.ttl", PROVENANCE_CLASH, &["--gate"]);
    assert_eq!(report["verdict"], "inconsistent", "{report:#}");
    assert_eq!(code, 1);
    assert_eq!(report["engines"]["native"]["verdict"], "clash");
    assert_eq!(report["selection"]["claims_read"], 2);
    // The report lists each claim it read, with the source that reported it.
    let sources: Vec<&str> = report["selection"]["claims"]
        .as_array()
        .expect("read claims")
        .iter()
        .map(|claim| claim["provenance"].as_str().expect("claim provenance"))
        .collect();
    assert_eq!(
        sources,
        [
            "https://example.org/prove/first",
            "https://example.org/prove/second"
        ]
    );
}

/// Reading a claim never decides `consistent` on its own: while source admission
/// still blocks a reified claim, a selection of claims with no clash is not decided,
/// the engine ran rather than refused, and the report counts the claims it read.
#[test]
fn provenance_only_claims_without_a_clash_are_not_decided_until_admitted() {
    let (report, code) = prove_json(
        "provenance-agree.logic.ttl",
        r#"
[] rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:provenance ex:first .
[] rdf:reifies <<( ex:b logic:instanceOf ex:A )>> ; logic:provenance ex:second .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(report["selection"]["claims_read"], 2);
    assert!(
        drop_codes(&report)
            .iter()
            .any(|code| code == "CONTEXTUAL_REIFICATION_REQUIRES_TYPED_TRANSLATION"),
        "{report:#}"
    );
}

/// A claim placed in a module is outside the selected theory: it is a named drop
/// that states the coordinate, it is not read, and it no longer makes the native
/// engine refuse the whole selection.
#[test]
fn a_module_scoped_claim_is_named_unread_and_the_engine_still_runs() {
    let (report, code) = prove_json(
        "module-claim.logic.ttl",
        r#"
ex:A logic:disjointWith ex:B .
ex:a logic:instanceOf ex:A .
[] rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:inModule ex:module .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(report["selection"]["claims_read"], 0);
    let unread: Vec<&str> = report["selection"]["drops"]
        .as_array()
        .expect("selection drops")
        .iter()
        .filter(|drop| drop["code"] == "SCOPED_CLAIM_NOT_READ")
        .map(|drop| drop["detail"].as_str().expect("drop detail"))
        .collect();
    assert!(
        matches!(unread[..], [detail] if detail.contains("logic:inModule")),
        "{report:#}"
    );
    assert!(
        !drop_codes(&report)
            .iter()
            .any(|code| code == "NATIVE_REFUSED"),
        "{report:#}"
    );
}

/// A contradiction across standpoints is not a contradiction of the theory. Two
/// claims that contradict each other, each placed in its own standpoint and each
/// carrying provenance, are not read: the selection is not decided, and each claim
/// is a drop that names the predicate that kept it out.
#[test]
fn claims_in_different_standpoints_are_not_a_contradiction() {
    let (report, code) = prove_json(
        "standpoints.logic.ttl",
        r#"
ex:A logic:disjointWith ex:B .
[] rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ;
    <https://blackcatinformatics.ca/gmeow/accordingTo> ex:S1 ; logic:provenance ex:first .
[] rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ;
    <https://blackcatinformatics.ca/gmeow/accordingTo> ex:S2 ; logic:provenance ex:second .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(report["selection"]["claims_read"], 0);
    let unread: Vec<&str> = report["selection"]["drops"]
        .as_array()
        .expect("selection drops")
        .iter()
        .filter(|drop| drop["code"] == "SCOPED_CLAIM_NOT_READ")
        .map(|drop| drop["detail"].as_str().expect("drop detail"))
        .collect();
    assert_eq!(unread.len(), 2, "{report:#}");
    assert!(
        unread
            .iter()
            .all(|detail| detail.contains("<https://blackcatinformatics.ca/gmeow/accordingTo>")),
        "{report:#}"
    );
}

/// A statement about a claim keeps the claim out even when another reifier carries
/// it. Here a second reifier refutes the first claim's reifier; the first claim is
/// not read, so its statement does not contradict the assertion, and the drop names
/// the reifier that mentions it. The refuting reifier counts wherever it is written:
/// as an evidence-only claim of the default graph, or as a quotation in a named
/// graph, where the drop also names the graph.
#[test]
fn a_claim_refuted_by_another_claim_is_not_read() {
    const CONTESTED: &str = r#"
ex:A logic:disjointWith ex:B .
ex:a logic:instanceOf ex:A .
ex:r1 rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:provenance ex:first .
"#;
    const REFUTES: &str = r#"ex:r2 rdf:reifies <<( ex:r1 <https://blackcatinformatics.ca/gmeow/standpointModality>
        <https://blackcatinformatics.ca/gmeow/refuted> )>>"#;
    for (name, body, claims_read, place) in [
        (
            "claim-about-claim.logic.ttl",
            format!("{CONTESTED}{REFUTES} ; logic:provenance ex:second .\n"),
            1,
            "that of https://example.org/prove/r2",
        ),
        (
            "claim-about-claim.trig",
            format!("{CONTESTED}ex:g {{ {REFUTES} . }}\n"),
            0,
            "that of https://example.org/prove/r2 in graph https://example.org/prove/g",
        ),
    ] {
        let (report, code) = prove_json(name, &body, &["--gate"]);
        refuted_claim_is_unread(&report, code, claims_read, place);
    }
}

fn refuted_claim_is_unread(report: &serde_json::Value, code: i32, claims_read: u64, place: &str) {
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(report["selection"]["claims_read"], claims_read);
    let unread: Vec<&str> = report["selection"]["drops"]
        .as_array()
        .expect("selection drops")
        .iter()
        .filter(|drop| drop["code"] == "SCOPED_CLAIM_NOT_READ")
        .map(|drop| drop["detail"].as_str().expect("drop detail"))
        .collect();
    assert!(
        matches!(
            unread[..],
            [detail] if detail.contains("its carrier https://example.org/prove/r1 is mentioned")
                && detail.ends_with(place)
        ),
        "{report:#}"
    );
}

/// `forall A. Src(A) -> (first and second)`, with `second` spelled by the caller.
fn conjunctive_consequent(second: &str) -> String {
    format!(
        r#"
ex:a logic:instanceOf ex:Src .
ex:law a logic:Formula ; logic:quantifiedVariable [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ] ; logic:forall ex:imp .
ex:imp a logic:Formula ; logic:antecedent ex:g ; logic:consequent ex:c .
ex:g a logic:Formula ; logic:relation logic:instanceOf ; logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Src ] .
ex:c a logic:Formula ; logic:and ex:c1, ex:c2 .
ex:c1 a logic:Formula ; logic:relation logic:instanceOf ; logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Dst ] .
ex:c2 a logic:Formula ; {second} .
"#
    )
}

/// A consequent that conjoins a unary atom with an atom of arity three is one
/// dependency the native engine executes: the run is decided, not an execution
/// failure, and no drop names the formula.
#[test]
fn a_consequent_mixing_a_unary_and_a_ternary_atom_is_executed() {
    let (report, code) = prove_json(
        "mixed-head.logic.ttl",
        &conjunctive_consequent(
            r#"logic:relation ex:tern ; logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 2 ; logic:termVariable "A" ]"#,
        ),
        &["--gate"],
    );
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 0);
    assert_eq!(report["selection"]["drops"], serde_json::json!([]));
}

/// A head the native engine cannot carry, here a positional argument atom on a
/// variable the antecedent binds, is a named drop that states the rule and its head
/// relations. The selection is not decided; it is not an execution failure.
#[test]
fn a_rule_head_the_engine_cannot_carry_is_a_named_drop() {
    let (report, code) = prove_json(
        "refused-head.logic.ttl",
        &conjunctive_consequent(
            r#"logic:relation logic:naryArg0 ; logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "A" ]"#,
        ),
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert!(
        report["selection"]["drops"]
            .as_array()
            .expect("drops")
            .iter()
            .any(|drop| drop["code"] == "UNSUPPORTED_CONSTRUCT"
                && drop["detail"].as_str().is_some_and(|detail| {
                    detail
                        .contains("rule <https://blackcatinformatics.ca/logic/formula-existential/")
                        && detail.contains("head relations [<https://example.org/prove/Dst>]")
                })),
        "{report:#}"
    );
}

/// `forall A. Src(A) -> exists U, V. (R1(V, A) on node U and R2(inner) on node V)`,
/// with both tuples spelled in the reified vocabulary. `inner` is the term the tuple
/// on `V` takes as its argument.
fn tuple_inside_a_tuple(inner: &str) -> String {
    let term = |index: usize, body: &str| {
        format!("[ a logic:TermCarrier ; logic:termIndex {index} ; {body} ]")
    };
    let var = |index: usize, name: &str| term(index, &format!(r#"logic:termVariable "{name}""#));
    let iri = |index: usize, name: &str| term(index, &format!("logic:termIri {name}"));
    let atom = |name: &str, relation: &str, first: String, second: String| {
        format!(
            "ex:{name} a logic:Formula ; logic:relation {relation} ; logic:argument {first}, {second} .\n"
        )
    };
    [
        "\nex:a logic:instanceOf ex:Src .\n".to_owned(),
        format!(
            "ex:law a logic:Formula ; logic:quantifiedVariable {} ; logic:forall ex:imp .\n",
            var(0, "A")
        ),
        "ex:imp a logic:Formula ; logic:antecedent ex:g ; logic:consequent ex:some .\n".to_owned(),
        atom("g", "logic:instanceOf", var(0, "A"), iri(1, "ex:Src")),
        format!(
            "ex:some a logic:Formula ; logic:quantifiedVariable {}, {} ; logic:exists ex:all .\n",
            var(0, "U"),
            var(1, "V")
        ),
        "ex:all a logic:Formula ; logic:and ex:t1, ex:t2, ex:t3, ex:t4, ex:t5 .\n".to_owned(),
        atom("t1", "logic:instanceOf", var(0, "U"), iri(1, "ex:R1")),
        atom("t2", "logic:naryArg0", var(0, "U"), var(1, "V")),
        atom("t3", "logic:naryArg1", var(0, "U"), var(1, "A")),
        atom("t4", "logic:instanceOf", var(0, "V"), iri(1, "ex:R2")),
        atom("t5", "logic:naryArg0", var(0, "V"), var(1, inner)),
    ]
    .concat()
}

fn has_drop(report: &serde_json::Value, code: &str, detail: &str) -> bool {
    report["selection"]["drops"]
        .as_array()
        .expect("drops")
        .iter()
        .any(|drop| {
            drop["code"] == code
                && drop["detail"]
                    .as_str()
                    .is_some_and(|text| text.contains(detail))
        })
}

/// A tuple that takes another tuple's node as an argument is executed whatever order
/// its two nodes were quantified in; it is not an execution failure and its rule is
/// not dropped at lowering.
#[test]
fn a_tuple_that_takes_another_tuples_node_is_executed() {
    let (report, code) = prove_json(
        "nested-tuple.logic.ttl",
        &tuple_inside_a_tuple("A"),
        &["--gate"],
    );
    assert_ne!(code, 5, "{report:#}");
    assert_eq!(
        report["engines"]["native"]["verdict"], "no-clash",
        "{report:#}"
    );
    assert!(
        !has_drop(&report, "UNSUPPORTED_CONSTRUCT", ""),
        "{report:#}"
    );
}

/// Two tuples that take each other's node have no node to mint. The rule is a named
/// drop that states both relations, and the selection is not decided.
#[test]
fn tuples_that_take_each_others_node_are_a_named_drop() {
    let (report, code) = prove_json(
        "tuple-cycle.logic.ttl",
        &tuple_inside_a_tuple("U"),
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert!(
        has_drop(&report, "UNSUPPORTED_CONSTRUCT", "each other's reifier")
            && has_drop(
                &report,
                "UNSUPPORTED_CONSTRUCT",
                "head relations [<https://example.org/prove/R1>, <https://example.org/prove/R2>]"
            ),
        "{report:#}"
    );
}

/// An authored existential rule record whose head the engine refuses is an execution
/// failure, not a drop: the record grammar is executed as written. The report still
/// carries the selection, marked incomplete, with the claim drop named before the
/// engine ran, and the failure names the rule and its head relation as data.
#[test]
fn an_engine_failure_reports_the_selection_and_the_drops_named_before_it() {
    let (report, code) = prove_json(
        "refused-record.logic.ttl",
        r#"
@prefix lx: <https://blackcatinformatics.ca/gmeow/logic/existential#> .
ex:a ex:p ex:a .
[] rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:inModule ex:module .
ex:r a lx:ExistentialRule ; lx:body ex:b1 ; lx:head ex:h1 .
ex:b1 lx:s "?x" ; lx:p ex:p ; lx:o "?x" .
ex:h1 lx:s "?x" ; lx:p logic:naryArg0 ; lx:o "?x" .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "execution-failure", "{report:#}");
    assert_eq!(code, 5);
    assert!(report["engines"].is_null(), "{report:#}");
    assert_eq!(report["selection"]["complete"], false);
    assert!(
        has_drop(&report, "SCOPED_CLAIM_NOT_READ", "logic:inModule"),
        "{report:#}"
    );
    let evidence = &report["diagnostics"][0]["diagnostic"]["fields"]["evidence"];
    assert_eq!(
        evidence["rule"], "https://example.org/prove/r",
        "{report:#}"
    );
    assert_eq!(
        evidence["head_relations"],
        serde_json::json!(["https://blackcatinformatics.ca/logic/naryArg0"])
    );
}
