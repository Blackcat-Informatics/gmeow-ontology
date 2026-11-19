// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed `gmeow prove` over tiny owned `logic:` inputs. Both engines run
//! in-process; these tests pin the three-valued verdict, the drop ledger, the exit
//! contract, deterministic output and proof replay.

use assert_cmd::Command;

const PREFIXES: &str = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
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

/// PROTECTS: explicit roots and actual cyclic imports contribute to one theory;
/// separate module consistency never licenses their contradictory composition.
/// Qualified duplicate carriers cannot erase admitted premises or borrow admission.
/// A blocked positive rule stays unread while the admitted fragment seeks a witness.
/// An extensional set can list a selected claim and a quotation without changing their authority.
/// Excluded collection owners supply no cells or synthetic classes, while
/// unrelated assertions about a shared resource remain executable.
#[test]
fn selected_modules_form_one_theory_with_actual_import_closure() {
    let body = r#"
ex:LeftModule a logic:Module .
ex:RightModule a logic:Module .
ex:Hub a logic:Module ; logic:imports ex:LeftModule, ex:Diamond .
ex:Diamond a logic:Module ; logic:imports ex:RightModule, ex:LeftModule, ex:Hub .
ex:A logic:disjointWith ex:B .
ex:left rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:inModule ex:LeftModule .
ex:right rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:inModule ex:RightModule .
ex:quoted rdf:reifies <<( ex:a logic:instanceOf logic:Nothing )>> .
ex:claims a math:FiniteSet ; math:hasElement ex:right, ex:quoted .
"#;
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "modules.ttl", body);
    let run = |extra: &[&str]| {
        let output = gmeow()
            .args(["prove", path(&source), "--format", "json"])
            .args(extra)
            .output()
            .expect("selected module operation");
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("module report")
    };
    for module in ["LeftModule", "RightModule"] {
        let root = format!("https://example.org/prove/{module}");
        let report = run(&["--module", &root]);
        assert_eq!(report["verdict"], "consistent", "{report:#}");
        assert_eq!(report["selection"]["claims_read"], 1);
    }
    let joint = run(&[
        "--module",
        "https://example.org/prove/LeftModule",
        "--module",
        "https://example.org/prove/RightModule",
    ]);
    assert_eq!(joint["verdict"], "inconsistent", "{joint:#}");
    assert_eq!(joint["selection"]["claims_read"], 2);
    assert!(
        joint["engines"]["native"]["refutations"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    let native = joint["refutations"]
        .as_array()
        .expect("refutations")
        .iter()
        .find(|row| row["engine"] == "native")
        .expect("native joint witness");
    let premises = native["premises"].to_string();
    for term in [
        "https://example.org/prove/a",
        "https://example.org/prove/A",
        "https://example.org/prove/B",
    ] {
        assert!(premises.contains(term), "missing {term} from {premises}");
    }
    assert_eq!(
        joint,
        run(&[
            "--module",
            "https://example.org/prove/RightModule",
            "--module",
            "https://example.org/prove/LeftModule"
        ])
    );
    let text = gmeow()
        .args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/LeftModule",
        ])
        .output()
        .expect("text module report");
    let text = String::from_utf8(text.stdout).expect("UTF-8 text");
    assert!(text.contains("theory-composition composed-modules"));
    assert!(text.contains("theory-root https://example.org/prove/LeftModule"));
    assert!(text.contains("verdict consistent"));
    assert!(
        text.lines()
            .any(|line| line.starts_with("claim-carrier program:axiom:")
                && line.ends_with("https://example.org/prove/left")),
        "{text}"
    );
    assert!(
        text.lines().any(
            |line| line.starts_with("source-unit OutsideRequestedTheory ")
                && line.contains("https://example.org/prove/right")
        ),
        "{text}"
    );
    assert!(text.contains(&format!("semantic-units {}", joint["selection"]["units"])));
    let imported = run(&["--module", "https://example.org/prove/Hub"]);
    assert_eq!(imported["verdict"], "inconsistent", "{imported:#}");
    let selection = &imported["selection"]["theory_selection"];
    assert_eq!(selection["modules"].as_array().map(Vec::len), Some(4));
    assert_eq!(selection["imports"].as_array().map(Vec::len), Some(5));
    assert_eq!(selection["includes_top_level"], true);
    assert_eq!(imported["selection"]["units"], joint["selection"]["units"]);
    for (left, expected, count) in [
        (
            "ex:left rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:inModule ex:LeftModule ; <https://blackcatinformatics.ca/gmeow/graphBoxRole> <https://blackcatinformatics.ca/gmeow/boxCBox> .
ex:alsoLeft rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:inModule ex:LeftModule .",
            "inconsistent",
            2,
        ),
        ("", "not-decided", 1),
    ] {
        let (report, code) = prove_json(
            "qualified-module-carrier.ttl",
            &format!(
                r#"
ex:LeftModule a logic:Module . ex:RightModule a logic:Module .
ex:A logic:disjointWith ex:B .
{left}
ex:right rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:inModule ex:RightModule .
ex:qualified rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ;
    logic:inModule ex:LeftModule ; <https://blackcatinformatics.ca/gmeow/graphBoxRole> <https://blackcatinformatics.ca/gmeow/boxCBox> ; ex:qualifier ex:unknown .
_:r a logic:TermCarrier ; logic:termVariable "r" .
ex:blocked a logic:Rule ; logic:inModule ex:LeftModule ; logic:standpoint ex:observer ;
    logic:head [ rdf:reifies <<( _:r logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:r logic:instanceOf ex:B )>> ] .
"#
            ),
            &[
                "--module", "https://example.org/prove/LeftModule",
                "--module", "https://example.org/prove/RightModule", "--gate",
            ],
        );
        assert_eq!(report["verdict"], expected, "{report:#}");
        assert_eq!(code, 3, "native verdicts retain attestation grade");
        assert_eq!(report["selection"]["complete"], false);
        assert_eq!(report["selection"]["claims_read"], count);
        assert!(has_drop(&report, "CONTEXT_ERASURE_FORBIDDEN", "blocked"), "{report:#}");
        let carriers: Vec<_> = report["selection"]["claims"]
            .as_array().expect("read claims")
            .iter()
            .flat_map(|claim| claim["carriers"].as_array().expect("actual carriers"))
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(carriers.contains(&"https://example.org/prove/right"));
        assert!(!carriers.contains(&"https://example.org/prove/qualified"));
        if expected == "inconsistent" {
            assert!(carriers.contains(&"https://example.org/prove/left"));
            assert!(carriers.contains(&"https://example.org/prove/alsoLeft"));
            assert_eq!(report["engines"]["native"]["verdict"], "clash");
            let premises = report["refutations"].to_string();
            assert!(premises.contains("https://example.org/prove/A"));
            assert!(premises.contains("https://example.org/prove/B"));
        } else {
            assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
        }
        let unread: Vec<_> = report["native_admission"]["execution_drops"]
            .as_array().expect("execution boundaries")
            .iter().filter(|drop| drop["code"] == "SCOPED_CLAIM_NOT_READ").collect();
        assert!(matches!(unread[..], [drop] if drop["detail"].as_str()
            .is_some_and(|detail| detail.contains("https://example.org/prove/qualified"))));
        assert!(report["native_admission"]["units"].as_array().expect("source obligations")
            .iter().any(|unit| unit["kind"] == "Quotation"
                && unit["anchor"].as_str().is_some_and(|anchor|
                    anchor.contains("https://example.org/prove/qualified"))
                && unit["disposition"]["Blocked"]["code"] == "SCOPED_CLAIM_NOT_READ"));
    }

    let collection = r#"
ex:M a logic:Module . ex:N a logic:Module .
ex:chain logic:propertyChainAxiom ex:head ; logic:inModule ex:N .
ex:head rdf:first ex:property ; rdf:rest rdf:nil .
rdf:first logic:domain logic:Nothing .
"#;
    for (extra, expected, native) in [
        ("", "not-decided", "no-clash"),
        (
            "ex:mark logic:domain logic:Nothing . ex:head ex:mark ex:value .",
            "inconsistent",
            "clash",
        ),
    ] {
        let (report, _) = prove_json(
            "module-collection.ttl",
            &format!("{collection}{extra}"),
            &["--module", "https://example.org/prove/M", "--gate"],
        );
        assert_eq!(report["verdict"], expected, "{report:#}");
        assert_eq!(report["engines"]["native"]["verdict"], native, "{report:#}");
        let units = report["native_admission"]["units"]
            .as_array()
            .expect("the source units remain visible");
        assert!(
            !units.iter().any(|unit| unit["kind"] == "ClassDefinition"
                && unit["anchor"]
                    .as_str()
                    .is_some_and(|anchor| anchor.contains("https://example.org/prove/head"))),
            "a property-list head is not a class definition: {report:#}"
        );
        assert!(
            units.iter().any(
                |unit| unit["disposition"].get("OutsideRequestedTheory").is_some()
                    && unit["anchor"]
                        .as_str()
                        .is_some_and(|anchor| anchor.contains("https://example.org/prove/head")
                            && anchor.contains("first"))
            ),
            "excluded physical cells remain inventoried: {report:#}"
        );
    }
}

/// PROTECTS: unresolved roots/imports fail closed before native execution.
#[test]
fn module_requests_refuse_missing_declarations_and_imports() {
    for body in [
        "ex:Root logic:imports ex:Missing .",
        "ex:Root a logic:Module ; logic:imports ex:Missing .",
    ] {
        let (report, code) = prove_json(
            "missing-module.ttl",
            body,
            &["--module", "https://example.org/prove/Root"],
        );
        assert_eq!(code, 4, "{report:#}");
        assert!(report["engines"].is_null());
    }
}

/// PROTECTS: an atomic formula link remains unread whether its equal ordinary
/// statement is independently asserted or excluded.
/// Its provenance follows the original formula account after exact source selection.
#[test]
fn excluded_statement_cannot_account_for_an_unread_atomic_formula() {
    let body = r#"
ex:Selected a logic:Module ; logic:hasFormula ex:law .
ex:Excluded a logic:Module .
ex:p logic:domain ex:C {scope} .
ex:a ex:p ex:b ; logic:instanceOf ex:D .
ex:C logic:disjointWith ex:D .
ex:law a logic:Formula ; logic:relation logic:domain ; logic:argument ex:propertyTerm, ex:classTerm ;
    logic:provenance ex:lawSource .
ex:propertyTerm logic:termIndex 0 ; logic:termIri ex:p .
ex:classTerm logic:termIndex 1 ; logic:termIri ex:C .
"#;
    for (scope, verdict) in [
        ("; logic:inModule ex:Excluded", "not-decided"),
        ("", "inconsistent"),
    ] {
        let (report, _) = prove_json(
            "atomic-formula.ttl",
            &body.replace("{scope}", scope),
            &["--module", "https://example.org/prove/Selected"],
        );
        assert_eq!(report["verdict"], verdict, "{report:#}");
        let units = report["native_admission"]["units"]
            .as_array()
            .expect("source units");
        let link = units
            .iter()
            .find(|unit| {
                unit["anchor"]
                    .as_str()
                    .is_some_and(|anchor| anchor.contains("/hasFormula"))
            })
            .expect("original module formula link");
        assert_eq!(
            link["disposition"]["Blocked"]["code"],
            "FORMULA_AXIOM_NOT_READ"
        );
        let evidence = units
            .iter()
            .find(|unit| {
                unit["anchor"].as_str().is_some_and(|anchor| {
                    anchor.contains("/provenance") && anchor.contains("/lawSource")
                })
            })
            .expect("original provenance row");
        if scope.is_empty() {
            let formula = units
                .iter()
                .find(|unit| {
                    unit["kind"] == "Formula"
                        && unit["anchor"]
                            .as_str()
                            .is_some_and(|anchor| anchor.ends_with("/law|0:"))
                })
                .expect("original formula occurrence");
            assert!(
                !formula["disposition"]["Accounted"].is_null(),
                "{formula:#}"
            );
            assert_eq!(
                evidence["disposition"]["FormulaEvidence"]["formula_origins"],
                serde_json::json!([formula["id"].clone()])
            );
        } else {
            assert_eq!(
                evidence["disposition"]["Blocked"]["code"],
                "FORMULA_EVIDENCE_NOT_ACCOUNTED"
            );
        }
    }
}

/// PROTECTS: composed required outputs remove stale bytes, and raw proof replay
/// cannot acquire composition authority from a supplied file or matching digest.
#[test]
fn composed_outputs_and_raw_replay_refuse_after_reporting_theory() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "module.ttl", "ex:Root a logic:Module .");
    let fof = directory.path().join("problem.p");
    let proof = directory.path().join("proof.cbor");
    std::fs::write(&fof, "stale FOF").expect("old output");
    std::fs::write(&proof, "stale proof").expect("old proof");
    let refusal = gmeow()
        .args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/Root",
            "--out",
            path(&fof),
            "--proof-out",
            path(&proof),
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let report = report(&refusal);
    assert_eq!(
        report["selection"]["theory_selection"]["modules"],
        serde_json::json!(["https://example.org/prove/Root"])
    );
    assert_eq!(report["export"]["written"], false);
    assert!(!fof.exists());
    assert!(!proof.exists());
    let missing_parent = directory.path().join("missing-parent/proof.cbor");
    for candidate in [&proof, &missing_parent] {
        let replay = gmeow()
            .args([
                "prove",
                path(&source),
                "--module",
                "https://example.org/prove/Root",
                "--check-proof",
                path(candidate),
                "--format",
                "json",
            ])
            .assert()
            .code(1);
        let replay: serde_json::Value =
            serde_json::from_slice(&replay.get_output().stdout).expect("proof-check report");
        assert_eq!(replay["verdict"], "rejected");
        assert_eq!(
            replay["theory_selection"]["modules"],
            serde_json::json!(["https://example.org/prove/Root"])
        );
        assert!(replay["diagnostics"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["code"] == "COMPOSED_PROOF_REPLAY_UNSUPPORTED")
        }));
    }
    let evidence = directory.path().join("unpublished-evidence.json");
    gmeow()
        .args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/Root",
            "--check-proof",
            path(&missing_parent),
            "--evidence-out",
            path(&evidence),
            "--format",
            "json",
        ])
        .assert()
        .code(4);
    assert!(
        !evidence.exists(),
        "unprotectable replay input disables sinks"
    );
    let original = std::fs::read(&source).expect("source bytes");
    let alias = directory.path().join("source-alias.ttl");
    std::fs::hard_link(&source, &alias).expect("input hard link");
    for sink in ["--out", "--proof-out", "--evidence-out"] {
        let mut command = gmeow();
        command.args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/Root",
            sink,
            path(&alias),
            "--format",
            "json",
        ]);
        if sink != "--evidence-out" {
            command.args(["--evidence-out", path(&source)]);
        }
        command.assert().code(4);
        assert_eq!(std::fs::read(&source).expect("preserved input"), original);
        assert_eq!(std::fs::read(&alias).expect("preserved alias"), original);
    }
    let shared = directory.path().join("shared-output.bin");
    std::fs::write(&shared, "retained output").expect("existing selected sink");
    gmeow()
        .args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/Root",
            "--out",
            path(&shared),
            "--proof-out",
            path(&shared),
            "--evidence-out",
            path(&shared),
            "--format",
            "json",
        ])
        .assert()
        .code(4);
    assert_eq!(
        std::fs::read(&shared).expect("preserved colliding output"),
        b"retained output"
    );
    let absent = directory.path().join("future-output.bin");
    std::fs::create_dir(directory.path().join("sub")).expect("existing path component");
    let equivalent = directory.path().join("sub/../future-output.bin");
    gmeow()
        .args([
            "prove",
            path(&source),
            "--module",
            "https://example.org/prove/Root",
            "--out",
            path(&absent),
            "--proof-out",
            path(&equivalent),
            "--format",
            "json",
        ])
        .assert()
        .code(4);
    assert!(
        !absent.exists(),
        "an alias refusal publishes neither selected sink"
    );
    #[cfg(unix)]
    {
        let parent_alias = directory.path().join("parent-alias");
        std::os::unix::fs::symlink(directory.path(), &parent_alias)
            .expect("static symlink parent alias");
        let source_alias = parent_alias.join("module.ttl");
        for sink in ["--out", "--proof-out", "--evidence-out"] {
            gmeow()
                .args([
                    "prove",
                    path(&source),
                    sink,
                    path(&source_alias),
                    "--format",
                    "json",
                ])
                .assert()
                .code(4);
            assert_eq!(std::fs::read(&source).expect("source retained"), original);
        }
        let dangling = directory.path().join("dangling-output.bin");
        std::os::unix::fs::symlink(&absent, &dangling).expect("terminal dangling output symlink");
        gmeow()
            .args([
                "prove",
                path(&source),
                "--module",
                "https://example.org/prove/Root",
                "--out",
                path(&dangling),
                "--format",
                "json",
            ])
            .assert()
            .code(5);
        assert_eq!(
            std::fs::read_link(&dangling).expect("guard preserves symlink"),
            absent
        );
        assert!(!absent.exists());
    }
}

/// A clash derived by the native engine refutes the selection, names the law and
/// its premises, and reports an unchecked attestation.
#[test]
fn disjointness_clash_is_inconsistent_and_gate_exits_three() {
    let (report, code) = prove_json("clash.logic.ttl", CLASH, &["--gate"]);
    assert_eq!(code, 3, "{report:#}");
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "refused");
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
fn horn_theory_is_consistent_with_zero_drops_and_gate_exits_three() {
    let (report, code) = prove_json("horn.logic.ttl", HORN, &["--gate"]);
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["selection"]["complete"], true);
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "attestation");
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

/// A formula the native lowering cannot carry blocks `consistent` and retains
/// the source boundary on its module link and provenance.
#[test]
fn uncarried_formula_is_not_decided_with_its_drop_named_and_gate_exits_three() {
    let (report, code) = prove_json("disjunctive.logic.ttl", DISJUNCTIVE, &["--gate"]);
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "attestation");
    assert_eq!(report["selection"]["complete"], false);
    assert!(
        report["selection"]["drops"]
            .as_array()
            .expect("drops")
            .iter()
            .any(|drop| drop["engine"] == "native" && drop["code"] == "UNSUPPORTED_CONSTRUCT"),
        "{report:#}"
    );
    let linked = format!(
        "{DISJUNCTIVE}\nex:Selected a logic:Module ; logic:hasFormula ex:law .\nex:law logic:provenance ex:sourceRecord .\n"
    );
    let (report, code) = prove_json(
        "linked-disjunction.ttl",
        &linked,
        &["--module", "https://example.org/prove/Selected", "--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    let units = report["native_admission"]["units"]
        .as_array()
        .expect("source units");
    let link = units
        .iter()
        .find(|unit| {
            unit["anchor"]
                .as_str()
                .is_some_and(|anchor| anchor.contains("/hasFormula"))
        })
        .expect("original disjunctive formula link");
    assert_eq!(
        link["disposition"]["Blocked"]["code"],
        "FORMULA_NATIVE_LOWERING_RESIDUE"
    );
    assert!(has_drop(
        &report,
        "FORMULA_EVIDENCE_NOT_ACCOUNTED",
        "/sourceRecord"
    ));
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
/// Replay input identity is retained, output aliases cannot truncate the proof,
/// and a required proof refusal removes an earlier proof at the selected sink.
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
    let proof_bytes = std::fs::read(&proof).expect("recorded proof bytes");
    let proof_input = check["inputs"]
        .as_array()
        .expect("selected input receipts")
        .iter()
        .find(|input| input["role"] == "selected-proof-replay")
        .expect("actual replay input receipt");
    assert_eq!(proof_input["path"], path(&proof));
    assert_eq!(proof_input["media_type"], "application/cbor");
    let proof_alias = directory.path().join("proof-alias.bin");
    std::fs::hard_link(&proof, &proof_alias).expect("proof hardlink");
    for output in [&proof, &proof_alias] {
        gmeow()
            .args([
                "prove",
                path(&source),
                "--check-proof",
                path(&proof),
                "--evidence-out",
                path(output),
                "--format",
                "json",
            ])
            .assert()
            .code(4);
        assert_eq!(std::fs::read(&proof).expect("proof retained"), proof_bytes);
    }

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

    let no_proof_source = write(
        directory.path(),
        "unrecorded.logic.ttl",
        "ex:shortChain logic:propertyChainAxiom ( ex:operand ) .",
    );
    let refusal = gmeow()
        .args([
            "prove",
            path(&no_proof_source),
            "--proof-out",
            path(&proof),
            "--format",
            "json",
        ])
        .assert()
        .code(5);
    let refusal = report(&refusal);
    assert!(
        refusal["diagnostics"]
            .as_array()
            .expect("refusal diagnostics")
            .iter()
            .any(|row| row["code"] == "PROOF_NOT_RECORDED"),
        "{refusal:#}"
    );
    assert!(
        !proof.exists(),
        "old proof cannot appear current after refusal"
    );
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

/// A record-form irreflexive law yields a native refutation with its actual fields.
#[test]
fn irreflexive_characteristic_record_refutes_a_self_edge() {
    let (report, exit) = prove_json(
        "record-irreflexive.logic.ttl",
        r#"
ex:pIrreflexive a logic:PropertyCharacteristicAssertion ;
    logic:characterizes ex:p ;
    logic:characteristicSort logic:irreflexiveProperty .
ex:a ex:p ex:a .
"#,
        &["--gate"],
    );
    assert_ne!(exit, 0);
    assert_eq!(report["verdict"], "inconsistent", "{report:#}");
    assert_eq!(
        report["engines"]["native"]["verdict"], "clash",
        "{report:#}"
    );
    assert!(
        !drop_codes(&report)
            .iter()
            .any(|code| code == "CHARACTERISTIC_RECORD_NOT_READ"),
        "{report:#}"
    );
    let premises = report["refutations"].to_string();
    assert!(premises.contains("characterizes"), "{report:#}");
    assert!(premises.contains("characteristicSort"), "{report:#}");
}

/// Transitive record consequences feed asymmetric record refutation in one theory.
#[test]
fn transitive_characteristic_record_feeds_asymmetric_refutation() {
    let (report, exit) = prove_json(
        "record-asymmetric.logic.ttl",
        r#"
ex:pAsymmetric a logic:PropertyCharacteristicAssertion ;
    logic:characterizes ex:p ;
    logic:characteristicSort logic:asymmetricProperty .
ex:pTransitive a logic:PropertyCharacteristicAssertion ;
    logic:characterizes ex:p ;
    logic:characteristicSort logic:transitiveProperty .
ex:a ex:p ex:b .
ex:b ex:p ex:c .
ex:c ex:p ex:a .
"#,
        &["--gate"],
    );
    assert_ne!(exit, 0);
    assert_eq!(report["verdict"], "inconsistent", "{report:#}");
    assert_eq!(
        report["engines"]["native"]["verdict"], "clash",
        "{report:#}"
    );
    assert!(
        !drop_codes(&report)
            .iter()
            .any(|code| code == "CHARACTERISTIC_RECORD_NOT_READ"),
        "{report:#}"
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

/// Named content cannot refute the selected default graph or hide a default-graph
/// refutation. All excluded source remains visible as named drops.
#[test]
fn named_graph_content_is_a_named_drop() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = directory.path().join("named-graph.trig");
    for (body, verdict, native) in [
        (
            "ex:A logic:disjointWith ex:B .\nex:a logic:instanceOf ex:A .\nex:g { ex:a logic:instanceOf ex:B . }",
            "not-decided",
            "no-clash",
        ),
        (
            "ex:sentinel ex:p ex:value .\nex:g { ex:A logic:disjointWith ex:B . ex:a logic:instanceOf ex:A, ex:B . }",
            "not-decided",
            "no-clash",
        ),
        (
            "ex:A logic:disjointWith ex:B .\nex:a logic:instanceOf ex:A, ex:B .\nex:g { ex:other ex:p ex:value . }",
            "inconsistent",
            "clash",
        ),
    ] {
        std::fs::write(&source, format!("{PREFIXES}\n{body}\n")).expect("write source");
        let output = gmeow()
            .args(["prove", path(&source), "--format", "json", "--gate"])
            .output()
            .expect("run prove");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert_eq!(report["verdict"], verdict, "{report:#}");
        assert_eq!(report["engines"]["native"]["verdict"], native, "{report:#}");
        assert_eq!(output.status.code(), Some(3), "{report:#}");
        assert!(
            drop_codes(&report)
                .iter()
                .any(|code| code == "OUTSIDE_SELECTION"),
            "{report:#}"
        );
        if verdict == "inconsistent" {
            assert!(
                report["refutations"]
                    .as_array()
                    .is_some_and(|refutations| refutations
                        .iter()
                        .any(|refutation| refutation["world"]
                            == gmeow_logic::reason::rl::DEFAULT_WORLD)),
                "{report:#}"
            );
        } else {
            assert_eq!(report["refutations"], serde_json::json!([]), "{report:#}");
        }
    }
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
    assert_eq!(code, 3);
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
/// Procedural negation is also refused before its absence-based clash can execute.
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

    let (report, code) = prove_json(
        "naf-clash.logic.ttl",
        r#"
ex:a ex:p ex:b .
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:absenceClash a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf logic:Nothing )>> ] ;
    logic:body [ rdf:reifies <<( _:x ex:p _:y )>> ] ;
    logic:negatedBody [ rdf:reifies <<( _:x ex:q _:y )>> ] .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "refused");
    assert_eq!(report["selection"]["complete"], false);
    assert_eq!(report["refutations"], serde_json::json!([]), "{report:#}");
    assert!(
        has_drop(
            &report,
            "NEGATION_AS_FAILURE_IS_NOT_CLASSICAL_NEGATION",
            "absenceClash"
        ) && has_drop(&report, "NATIVE_REFUSAL_REASON", "program:rule:0"),
        "{report:#}"
    );
}

/// PROTECTS: relationship-type membership is accounted through either type spelling
/// and carries its exact authored premise into the native refutation.
#[test]
fn relationship_type_membership_supplies_an_accounted_native_premise() {
    const CLASS: &str = "https://blackcatinformatics.ca/logic/RelationshipType";
    const SUBJECT: &str = "https://example.org/prove/relationship";
    const RULE: &str = r#"
ex:conflict a logic:Rule ;
    logic:body [ rdf:reifies <<( ex:relationship logic:instanceOf logic:RelationshipType )>> ] ;
    logic:head [ rdf:reifies <<( ex:relationship logic:instanceOf logic:Nothing )>> ] .
"#;
    for (syntax, predicate) in [
        (
            "logic:instanceOf",
            "https://blackcatinformatics.ca/logic/instanceOf",
        ),
        ("a", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
    ] {
        let source = format!("{RULE}\nex:relationship {syntax} logic:RelationshipType .");
        let (report, code) = prove_json("relationship-type.logic.ttl", &source, &["--gate"]);
        assert_eq!(code, 3);
        assert_eq!(report["verdict"], "inconsistent", "{report:#}");
        assert_eq!(report["engines"]["native"]["verdict"], "clash");
        let units = report["native_admission"]["units"]
            .as_array()
            .expect("source units");
        let assertion = units
            .iter()
            .find(|unit| {
                unit["kind"] == "Assertion"
                    && unit["anchor"].as_str().is_some_and(|anchor| {
                        anchor.contains(SUBJECT)
                            && anchor.contains(predicate)
                            && anchor.contains(CLASS)
                    })
            })
            .expect("original membership assertion");
        assert!(
            !assertion["disposition"]["Accounted"].is_null(),
            "{assertion:#}"
        );
        assert!(
            !has_drop(&report, "UNIT_NOT_ACCOUNTED", "RelationshipType"),
            "{report:#}"
        );
        let witness = report["refutations"]
            .as_array()
            .expect("refutations")
            .iter()
            .find(|witness| witness["engine"] == "native")
            .expect("native refutation");
        assert_eq!(
            witness["premises"],
            serde_json::json!([[SUBJECT, predicate, format!("<{CLASS}>")]])
        );
    }

    let (report, code) = prove_json(
        "relationship-type-class-position.logic.ttl",
        &format!("{RULE}\nlogic:RelationshipType logic:subClassOf ex:Classifier ."),
        &["--gate"],
    );
    assert_eq!(code, 3);
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
    assert!(
        has_drop(&report, "UNIT_NOT_ACCOUNTED", "subClassOf"),
        "{report:#}"
    );
}

/// PROTECTS: selected governance retains exact blank-scoped rows and derived
/// witnesses, and withholds absence when selected source coverage is incomplete.
#[test]
fn selected_non_entailment_binds_source_rows_and_keeps_supported_violations() {
    let directory = tempfile::tempdir().expect("source directory");
    write(
        directory.path(),
        "first.ttl",
        r#"
_:obligation a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:absentFirst ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
ex:s ex:seed ex:o .
"#,
    );
    write(
        directory.path(),
        "second.ttl",
        r#"
_:obligation logic:instanceOf logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:absentSecond ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
"#,
    );
    let completed = gmeow()
        .args(["prove", "--dir", path(directory.path()), "--format", "json"])
        .assert()
        .success();
    let completed = report(&completed);
    assert_eq!(completed["verdict"], "consistent", "{completed:#}");
    let text = gmeow()
        .args(["prove", "--dir", path(directory.path()), "--format", "text"])
        .assert()
        .success();
    let text = std::str::from_utf8(&text.get_output().stdout).expect("text report");
    let admission = &completed["native_admission"];
    let records = admission["non_entailment"]["obligations"]
        .as_array()
        .expect("selected governance records");
    assert_eq!(records.len(), 2);
    assert_ne!(records[0]["obligation"], records[1]["obligation"]);
    let units = admission["units"].as_array().expect("source inventory");
    for record in records {
        assert_eq!(record["disposition"], "SyntacticallyDischarged");
        let (expected_document, typing_field) =
            if record["forbidden_predicate"] == "https://example.org/prove/absentFirst" {
                (
                    "first.ttl",
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                )
            } else {
                assert_eq!(
                    record["forbidden_predicate"],
                    "https://example.org/prove/absentSecond"
                );
                (
                    "second.ttl",
                    "https://blackcatinformatics.ca/logic/instanceOf",
                )
            };
        let expected_fields = [
            typing_field,
            "https://blackcatinformatics.ca/logic/obligationForbiddenPredicate",
            "https://blackcatinformatics.ca/logic/obligationDischargeCondition",
        ];
        let mut expected_sources = Vec::with_capacity(expected_fields.len());
        for field in expected_fields {
            let mut matching = units.iter().filter(|unit| {
                unit["anchor"]
                    .as_str()
                    .is_some_and(|anchor| anchor.contains(field))
                    && unit["documents"].as_array().is_some_and(|documents| {
                        documents.len() == 1
                            && documents[0]
                                .as_str()
                                .is_some_and(|name| name.ends_with(expected_document))
                    })
            });
            let unit = matching.next().expect("original selected field");
            assert!(matching.next().is_none(), "{expected_document} {field}");
            assert_eq!(unit["disposition"], "NonEntailmentDeclaration");
            let id = unit["id"].as_str().expect("source ID");
            let anchor = unit["anchor"].as_str().expect("source anchor");
            assert!(text.lines().any(|line| {
                line.starts_with(&format!(
                    "source-unit NonEntailmentDeclaration {anchor} - {id} "
                )) && line.contains(expected_document)
            }));
            expected_sources.push(id);
        }
        let mut sources: Vec<_> = record["sources"]
            .as_array()
            .expect("original source IDs")
            .iter()
            .map(|id| id.as_str().expect("source ID"))
            .collect();
        sources.sort_unstable();
        expected_sources.sort_unstable();
        assert_eq!(sources, expected_sources);
    }

    let (incomplete, code) = prove_json(
        "governance.ttl",
        r#"
ex:malformed a logic:NonEntailmentObligation ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
ex:inconclusive a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:unproduced ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
ex:violated a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:forbidden ;
    logic:obligationDischargeCondition "invalid condition" .
ex:s ex:seed ex:o .
ex:derive a logic:Rule ;
    logic:body [ rdf:reifies <<( ex:s ex:seed ex:o )>> ] ;
    logic:head [ rdf:reifies <<( ex:s ex:forbidden ex:o )>> ] .
"#,
        &["--gate"],
    );
    assert_eq!(code, 3, "{incomplete:#}");
    assert_eq!(incomplete["verdict"], "not-decided", "{incomplete:#}");
    assert_eq!(incomplete["engines"]["native"]["verdict"], "no-clash");
    let records = incomplete["native_admission"]["non_entailment"]["obligations"]
        .as_array()
        .expect("selected governance records");
    let record = |iri| {
        records
            .iter()
            .find(|record| record["obligation"] == iri)
            .expect("declared obligation")
    };
    assert!(
        record("https://example.org/prove/malformed")["disposition"]["Invalid"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("obligationForbiddenPredicate"))
    );
    assert_eq!(
        record("https://example.org/prove/inconclusive")["disposition"]["Inconclusive"]["reason"],
        "the selected theory has malformed or unsupported non-entailment declarations"
    );
    let violated = record("https://example.org/prove/violated");
    let witness = &violated["disposition"]["Violated"];
    assert_eq!(witness["predicate"], "https://example.org/prove/forbidden");
    assert_eq!(witness["is_edb"], false);
    assert!(
        violated["declaration_error"]
            .as_str()
            .is_some_and(|detail| { detail.contains("non-IRI discharge condition") })
    );

    let blocked_source = write(
        directory.path(),
        "governance-policy.ttl",
        r#"
ex:policy a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:possible ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
ex:absent a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:unreachable ;
    logic:obligationDischargeCondition logic:DischargeSyntacticReachability .
ex:finite a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:possible ;
    logic:obligationDischargeCondition logic:DischargeFiniteClosure .
ex:s ex:seed ex:o ; logic:unaccountedRelation ex:o .
ex:possibleWriter a logic:Rule ;
    logic:body [ rdf:reifies <<( ex:s ex:missingSeed ex:o )>> ] ;
    logic:head [ rdf:reifies <<( ex:s ex:possible ex:o )>> ] .
ex:derivedCondition a logic:Rule ;
    logic:body [ rdf:reifies <<( ex:s ex:seed ex:o )>> ] ;
    logic:head [ rdf:reifies <<( ex:policy logic:obligationDischargeCondition logic:DischargeFiniteClosure )>> ] .
"#,
    );
    let blocked = gmeow()
        .args(["prove", path(&blocked_source), "--format", "json", "--gate"])
        .assert()
        .code(3);
    let blocked = report(&blocked);
    let records = blocked["native_admission"]["non_entailment"]["obligations"]
        .as_array()
        .expect("selected governance records");
    let reason = |iri| {
        records
            .iter()
            .find(|record| record["obligation"] == iri)
            .expect("declared obligation")["disposition"]["Inconclusive"]["reason"]
            .as_str()
            .expect("inconclusive reason")
    };
    assert!(reason("https://example.org/prove/policy").contains("matches a rule head"));
    let selected_gap = reason("https://example.org/prove/absent");
    assert!(selected_gap.contains("incomplete coverage"));
    let drops = blocked["selection"]["drops"]
        .as_array()
        .expect("selected drops");
    assert!(drops.iter().any(|drop| {
        let code = drop["code"].as_str().expect("code");
        let detail = drop["detail"].as_str().expect("detail");
        selected_gap.ends_with(&format!("({code}): {detail}"))
    }));
    assert_eq!(reason("https://example.org/prove/finite"), selected_gap);
    let malformed_source = write(
        directory.path(),
        "malformed-governance.ttl",
        r#"
ex:unread a logic:NonEntailmentObligation ;
    logic:obligationForbiddenPredicate ex:other ;
    logic:obligationDischargeCondition <<( ex:s ex:seed ex:o )>> .
"#,
    );
    let malformed = gmeow()
        .args([
            "prove",
            path(&malformed_source),
            "--format",
            "json",
            "--gate",
        ])
        .assert()
        .code(4);
    let malformed = report(&malformed);
    assert_eq!(malformed["verdict"], "malformed");
    assert!(malformed["engines"].is_null());
    let units = malformed["native_admission"]["units"]
        .as_array()
        .expect("source inventory");
    let field = units
        .iter()
        .find(|unit| {
            unit["anchor"].as_str().is_some_and(|anchor| {
                anchor.contains("https://example.org/prove/unread")
                    && anchor.contains(
                        "https://blackcatinformatics.ca/logic/obligationDischargeCondition",
                    )
            })
        })
        .expect("original unread condition");
    let boundary = &field["disposition"]["Blocked"];
    assert_eq!(boundary["code"], "UNSUPPORTED_SELECTED_ASSERTION");
    assert_eq!(
        boundary["non_entailment_field"],
        "obligationDischargeCondition"
    );
    assert!(
        boundary["reason"]
            .as_str()
            .is_some_and(|detail| detail.contains("quoted triple"))
    );
    assert!(units.iter().any(|unit| {
        unit["disposition"]["Malformed"]["code"] == "COMPILER_ERROR_IN_SELECTED_THEORY"
    }));
    let malformed_text = gmeow()
        .args([
            "prove",
            path(&malformed_source),
            "--format",
            "text",
            "--gate",
        ])
        .assert()
        .code(4);
    let malformed_text =
        std::str::from_utf8(&malformed_text.get_output().stdout).expect("text report");
    let id = field["id"].as_str().expect("source ID");
    let anchor = field["anchor"].as_str().expect("source anchor");
    assert!(malformed_text.lines().any(|line| {
        line.starts_with(&format!(
            "source-unit Blocked {anchor} UNSUPPORTED_SELECTED_ASSERTION {id} "
        )) && line.contains("malformed-governance.ttl")
            && line.contains("obligationDischargeCondition quoted triple")
    }));
}

/// PROTECTS: universal-class tests that native closure does not populate prevent
/// a complete no-clash claim, including premises exposed by negation normalization.
/// Asserted/head membership remains accountable, and an independent clash remains
/// a faithful refutation even beside the incomplete universal-class account.
#[test]
fn universal_membership_premises_do_not_authorize_complete_no_clash() {
    const FACTS: &str = r#"
ex:A logic:disjointWith ex:B .
ex:a logic:instanceOf ex:B ; ex:p ex:b .
"#;
    const ATOMS: &str = r#"
ex:x a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "x" .
ex:thing a logic:TermCarrier ; logic:termIndex 1 ;
    logic:termIri <http://www.w3.org/2002/07/owl#Thing> .
ex:classA a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:A .
ex:thingBody a logic:Formula ; logic:relation logic:instanceOf ;
    logic:argument ex:x, ex:thing .
ex:classHead a logic:Formula ; logic:relation logic:instanceOf ;
    logic:argument ex:x, ex:classA .
ex:law a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:imp .
"#;
    for (name, rule) in [
        (
            "compact",
            r#"
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:thingGuard a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf ex:A )>> ] ;
    logic:body [ rdf:reifies <<( _:x logic:instanceOf logic:Thing )>> ],
        [ rdf:reifies <<( _:x ex:p _:y )>> ] .
"#
            .to_owned(),
        ),
        (
            "implication",
            format!(
                "{ATOMS} ex:imp a logic:Formula ; logic:antecedent ex:thingBody ; logic:consequent ex:classHead ."
            ),
        ),
        (
            "normalized-disjunction",
            format!(
                "{ATOMS} ex:negative a logic:Formula ; logic:not ex:thingBody . ex:imp a logic:Formula ; logic:or ex:negative, ex:classHead ."
            ),
        ),
    ] {
        let source = format!("{FACTS}{rule}");
        let (report, code) = prove_json(&format!("{name}.logic.ttl"), &source, &["--gate"]);
        assert_eq!(report["verdict"], "not-decided", "{name}: {report:#}");
        assert_eq!(code, 3);
        assert_eq!(report["selection"]["complete"], false);
        assert_eq!(report["engines"]["native"]["verdict"], "no-clash");
        assert!(
            has_drop(&report, "UNIT_NOT_ACCOUNTED", "universal membership"),
            "{name}: {report:#}"
        );
        if name == "compact" {
            let (report, code) = prove_json(
                "independent-clash.logic.ttl",
                &format!("{source} ex:a logic:instanceOf ex:A ."),
                &["--gate"],
            );
            assert_eq!(report["verdict"], "inconsistent", "{report:#}");
            assert_eq!(code, 3);
            assert_eq!(report["selection"]["complete"], false);
            assert_eq!(report["engines"]["native"]["verdict"], "clash");
            assert!(
                report["refutations"]
                    .as_array()
                    .is_some_and(|refutations| !refutations.is_empty()),
                "{report:#}"
            );
        }
    }
    let (report, code) = prove_json(
        "positive-thing.logic.ttl",
        r#"
ex:a logic:instanceOf logic:Thing ; ex:p ex:b .
_:x a logic:TermCarrier ; logic:termVariable "x" .
_:y a logic:TermCarrier ; logic:termVariable "y" .
ex:thingHead a logic:Rule ;
    logic:head [ rdf:reifies <<( _:x logic:instanceOf logic:Thing )>> ] ;
    logic:body [ rdf:reifies <<( _:x ex:p _:y )>> ] .
"#,
        &["--gate"],
    );
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["selection"]["complete"], true);
    assert_eq!(report["selection"]["drops"], serde_json::json!([]));
}

/// PROTECTS: configuration and correspondence catalogues account only their exact
/// validated records, retain owned recovery syntax outside object premises, and
/// keep resolved module metadata outside object premises without hiding a clash
/// or unsupported statement on the same node. Rejected and orphan Lens links
/// cannot become premises, while independent reification retains its assertion.
#[test]
fn configuration_declarations_are_accounted_without_a_drop() {
    const CATALOGUE: &str = r#"
ex:classical a logic:ReasoningContract ; logic:modelSemantics logic:ClassicalSemantics .
ex:catalogue a logic:Correspondence ; logic:correspondenceRelation logic:RelatedMatch ;
    logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ;
    logic:sourceEndpoint ex:source ; logic:targetEndpoint ex:target ;
    logic:recoveryCase ex:case .
ex:case a logic:RecoveryCase ; logic:recoveryTransform ex:recovery .
ex:recovery a logic:Formula ; logic:relation ex:p ; logic:argument ex:left, ex:right .
ex:left logic:termIndex 0 ; logic:termIri ex:subject .
ex:right logic:termIndex 1 ; logic:termIri ex:object .
ex:subject ex:property ex:object .
"#;
    let (report, code) = prove_json("contracts.logic.ttl", CATALOGUE, &["--gate"]);
    assert_eq!(report["verdict"], "consistent", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["selection"]["drops"], serde_json::json!([]));
    assert!(
        report["native_admission"]["units"]
            .as_array()
            .expect("admission units")
            .iter()
            .any(|unit| unit["disposition"]
                .as_object()
                .is_some_and(|disposition| disposition.contains_key("ConfigurationDeclaration"))),
        "{report:#}"
    );
    let records: Vec<_> = report["native_admission"]["units"]
        .as_array()
        .expect("source units")
        .iter()
        .filter(|unit| unit["disposition"].get("MetatheoryDeclaration").is_some())
        .collect();
    assert!(
        records.iter().any(|row| row["kind"] == "Owner"),
        "{report:#}"
    );
    for field in [
        "correspondenceRelation",
        "recoveryTransform",
        "relation",
        "termIri",
        "termIndex",
    ] {
        assert!(
            records.iter().any(|row| row["anchor"]
                .as_str()
                .is_some_and(|anchor| anchor.contains(field))),
            "{field}: {report:#}"
        );
    }
    for (extra, expected) in [
        (
            "ex:A logic:disjointWith ex:B . ex:catalogue logic:instanceOf ex:A, ex:B .",
            "inconsistent",
        ),
        ("ex:catalogue logic:instanceOf logic:Lens .", "not-decided"),
        (
            "ex:catalogue logic:unreadRecordField ex:value .",
            "not-decided",
        ),
        (
            "ex:unreadAtomic a logic:Formula ; logic:relation logic:sourceEndpoint ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:catalogue ], [ logic:termIndex 1 ; logic:termIri ex:source ] .",
            "not-decided",
        ),
        ("logic:relation logic:domain logic:Nothing .", "not-decided"),
    ] {
        let (changed, _) = prove_json(
            "catalogue-boundary.logic.ttl",
            &format!("{CATALOGUE}{extra}"),
            &["--gate"],
        );
        assert_eq!(changed["verdict"], expected, "{changed:#}");
        if expected == "not-decided" {
            assert!(
                !drop_codes(&changed).is_empty(),
                "unaccounted object obligations stay visible"
            );
        }
        if extra.starts_with("logic:relation") {
            assert_eq!(
                changed["engines"]["native"]["verdict"], "no-clash",
                "catalogue syntax is not an object premise: {changed:#}"
            );
        }
    }
    let rejected = format!(
        "{CATALOGUE}\nex:catalogue logic:hasLens ex:undeclaredLens .\n\
         logic:hasLens logic:domain logic:Nothing ."
    );
    for (extra, expected, native) in [
        ("", "not-decided", "no-clash"),
        (
            "ex:assertion rdf:reifies <<( ex:catalogue logic:hasLens ex:undeclaredLens )>> ; logic:provenance ex:source .",
            "inconsistent",
            "clash",
        ),
    ] {
        let (changed, code) = prove_json(
            "rejected-lens-boundary.logic.ttl",
            &format!("{rejected}\n{extra}"),
            &["--gate"],
        );
        assert_eq!(changed["verdict"], expected, "{changed:#}");
        assert_eq!(code, 3);
        assert_eq!(
            changed["engines"]["native"]["verdict"], native,
            "{changed:#}"
        );
        assert_eq!(
            changed["engines"]["owl_dl"]["state"], "consistent",
            "{changed:#}"
        );
        assert_eq!(changed["selection"]["complete"], false);
        assert!(
            has_drop(&changed, "SKIPPED_SELECTED_OWNER", "catalogue"),
            "{changed:#}"
        );
        assert!(
            changed["native_admission"]["units"]
                .as_array()
                .expect("source units")
                .iter()
                .any(|unit| unit["anchor"]
                    .as_str()
                    .is_some_and(|anchor| anchor.starts_with("quad|")
                        && anchor.contains("hasLens")
                        && anchor.contains("undeclaredLens"))
                    && unit["disposition"].get("Blocked").is_some()),
            "{changed:#}"
        );
    }
    let (orphan, code) = prove_json(
        "orphan-lens-boundary.logic.ttl",
        &format!(
            "{CATALOGUE}\nex:orphan a logic:ReasoningContract ; \
             logic:modelSemantics logic:ClassicalSemantics ; logic:hasLens ex:undeclaredLens .\n\
             logic:hasLens logic:domain logic:Nothing ."
        ),
        &["--gate"],
    );
    assert_eq!(orphan["verdict"], "not-decided", "{orphan:#}");
    assert_eq!(code, 3);
    assert_eq!(
        orphan["engines"]["native"]["verdict"], "no-clash",
        "{orphan:#}"
    );
    assert_eq!(
        orphan["engines"]["owl_dl"]["state"], "consistent",
        "{orphan:#}"
    );
    assert!(
        has_drop(&orphan, "NO_EXECUTABLE_OWNER_PATH", "hasLens"),
        "{orphan:#}"
    );

    let scoped = format!(
        "{CATALOGUE}\nex:M a logic:Module . ex:catalogue logic:inModule ex:M .\n\
         logic:inModule logic:domain logic:Nothing ."
    );
    for (extra, expected, native) in [
        ("", "not-decided", "no-clash"),
        (
            "ex:A logic:disjointWith ex:B . ex:catalogue logic:instanceOf ex:A, ex:B .",
            "inconsistent",
            "clash",
        ),
    ] {
        let (changed, _) = prove_json(
            "scoped-catalogue-boundary.logic.ttl",
            &format!("{scoped}\n{extra}"),
            &["--module", "https://example.org/prove/M", "--gate"],
        );
        assert_eq!(changed["verdict"], expected, "{changed:#}");
        assert_eq!(
            changed["engines"]["native"]["verdict"], native,
            "{changed:#}"
        );
    }
}

/// Two reports from different sources that type one individual into two disjoint
/// classes.
const PROVENANCE_CLASH: &str = r#"
ex:A logic:disjointWith ex:B .
[] rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:provenance ex:first .
[] rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:provenance ex:second .
"#;

/// A claim that carries only provenance is a statement of the selected theory, so
/// two such claims that contradict each other refute the selected native theory.
/// Descriptive roles, rationale and lineage retain their values without becoming evidence.
#[test]
fn provenance_only_contradiction_is_inconsistent() {
    for body in [
        PROVENANCE_CLASH.to_owned(),
        PROVENANCE_CLASH.replace(" ; logic:provenance", " ; <https://blackcatinformatics.ca/gmeow/graphBoxRole> <https://blackcatinformatics.ca/gmeow/boxCBox>, ex:futureRole ; logic:provenance"),
        PROVENANCE_CLASH.replace(
            " ; logic:provenance",
            r#" ; <https://blackcatinformatics.ca/gmeow/statedRationale> "source reason"@en ; <https://blackcatinformatics.ca/gmeow/wasDerivedFrom> ex:sourceRecord ; logic:provenance"#,
        ),
    ] {
    let (report, code) = prove_json("provenance-clash.logic.ttl", &body, &["--gate"]);
    assert_eq!(report["verdict"], "inconsistent", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["evidence_grade"], "attestation");
    assert_eq!(report["gate_admission"], "refused");
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
}

/// Evidence-only claims are admitted by the actual native ingress and complete
/// source accounting. Classical export retains its separate reification blocker;
/// selecting that unavailable output fails and removes stale bytes.
#[test]
fn provenance_only_claims_without_a_clash_are_native_consistent() {
    let body = r#"
[] rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:provenance ex:first ;
    <https://blackcatinformatics.ca/gmeow/graphBoxRole> <https://blackcatinformatics.ca/gmeow/boxCBox>, ex:futureRole .
[] rdf:reifies <<( ex:b logic:instanceOf ex:A )>> ; logic:provenance ex:second .
"#;
    let (native, code) = prove_json("provenance-agree.logic.ttl", body, &["--gate"]);
    assert_eq!(native["verdict"], "consistent", "{native:#}");
    assert_eq!(code, 3);
    assert_eq!(native["evidence_grade"], "attestation");
    assert_eq!(native["gate_admission"], "attestation");
    assert_eq!(native["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(native["selection"]["claims_read"], 2);
    assert_eq!(native["selection"]["complete"], true);
    assert_eq!(native["selection"]["drops"], serde_json::json!([]));
    assert_eq!(native["native_admission"]["complete"], true);
    assert!(
        native["admission"].is_null(),
        "native-only proof has no classical admission"
    );

    let directory = tempfile::tempdir().expect("owned export directory");
    let source = write(directory.path(), "provenance-agree.logic.ttl", body);
    let target = directory.path().join("selected.p");
    std::fs::write(&target, "stale problem").expect("existing stale export");
    let output = gmeow()
        .args([
            "prove",
            path(&source),
            "--format",
            "json",
            "--out",
            path(&target),
        ])
        .output()
        .expect("selected classical export runs");
    let exported: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("selected output report");
    assert_eq!(output.status.code(), Some(5), "{exported:#}");
    assert_eq!(exported["verdict"], "execution-failure");
    assert_eq!(
        exported["diagnostics"][0]["code"],
        "SELECTED_EXPORT_BLOCKED"
    );
    assert_eq!(exported["engines"]["native"]["verdict"], "no-clash");
    assert_eq!(exported["native_admission"]["complete"], true);
    assert_eq!(exported["selection"]["complete"], true);
    assert_eq!(exported["export"]["written"], false);
    assert!(
        exported["export"]["blockers"]
            .as_array()
            .is_some_and(|blockers| !blockers.is_empty())
    );
    assert!(
        exported["admission"]["units"]
            .as_array()
            .is_some_and(|units| units
                .iter()
                .any(|unit| unit["disposition"]["Blocked"]["code"]
                    == "CONTEXTUAL_REIFICATION_REQUIRES_TYPED_TRANSLATION"))
    );
    assert!(
        !target.exists(),
        "a selected blocked export never leaves stale bytes"
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
    // Execution boundaries count original unread carriers; source-obligation
    // rows retain their separate quotation and canonical-axiom judgments.
    let unread: Vec<&str> = report["native_admission"]["execution_drops"]
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
    let unread: Vec<&str> = report["native_admission"]["execution_drops"]
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
    let unread: Vec<&str> = report["native_admission"]["execution_drops"]
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

/// A module-linked formula can refute the composed theory, while its excluded
/// and context-blocked occurrences retain independent source dispositions.
/// Its original provenance is attributed to the formula without supplying a premise.
#[test]
fn an_imported_formula_assertion_supplies_a_native_refutation() {
    let body = conjunctive_consequent(
        r#"logic:relation ex:marked ; logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:a ]"#,
    ) + r#"
ex:Chosen a logic:Module ; logic:hasFormula ex:law, ex:blocked .
ex:Other a logic:Module .
ex:Hub a logic:Module ; logic:imports ex:Chosen .
ex:law logic:provenance ex:lawSource .
ex:g logic:provenance ex:guardSource .
ex:blocked a logic:Formula ; logic:quantifiedVariable [
    a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A"
] ; logic:forall ex:imp ; logic:provenance ex:otherSource ; logic:standpoint ex:observer .
ex:a logic:instanceOf ex:Opposition .
ex:Opposition logic:disjointWith ex:Dst .
"#;
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = write(directory.path(), "linked-formulas.ttl", &body);
    let run = |format: &str, module: &str| {
        gmeow()
            .args([
                "prove",
                path(&source),
                "--format",
                format,
                "--module",
                module,
            ])
            .output()
            .expect("proof report")
    };
    for (root, verdict, link_disposition) in [
        ("Hub", "inconsistent", "StructuralSupport"),
        ("Other", "consistent", "OutsideRequestedTheory"),
    ] {
        let iri = format!("https://example.org/prove/{root}");
        let report: serde_json::Value =
            serde_json::from_slice(&run("json", &iri).stdout).expect("JSON proof report");
        assert_eq!(report["verdict"], verdict, "{report:#}");
        let units = report["native_admission"]["units"]
            .as_array()
            .expect("source units");
        let link = |target: &str| {
            units
                .iter()
                .find(|unit| {
                    unit["anchor"].as_str().is_some_and(|anchor| {
                        anchor.contains("/hasFormula") && anchor.contains(target)
                    })
                })
                .expect("original physical module formula link")
        };
        let law = link("/law");
        assert!(!law["disposition"][link_disposition].is_null(), "{law:#}");
        assert_eq!(law["documents"].as_array().map(Vec::len), Some(1));
        let blocked = link("/blocked");
        if root == "Hub" {
            assert_eq!(
                blocked["disposition"]["Blocked"]["code"], "CONTEXT_ERASURE_FORBIDDEN",
                "{blocked:#}"
            );
            let formula = units
                .iter()
                .find(|unit| {
                    unit["kind"] == "Formula"
                        && unit["anchor"]
                            .as_str()
                            .is_some_and(|anchor| anchor.ends_with("/law|0:"))
                })
                .expect("original formula occurrence");
            let output = run("text", &iri);
            let text = std::str::from_utf8(&output.stdout).expect("UTF-8 proof report");
            let formula_id = formula["id"].as_str().expect("formula ID");
            let formula_anchor = formula["anchor"].as_str().expect("formula anchor");
            let origin_line = format!("formula-origin {formula_id} {formula_anchor}");
            assert!(text.lines().any(|line| line == origin_line));
            for source in ["/lawSource", "/guardSource"] {
                let evidence = units
                    .iter()
                    .find(|unit| {
                        unit["anchor"].as_str().is_some_and(|anchor| {
                            anchor.contains("/provenance") && anchor.contains(source)
                        })
                    })
                    .expect("original provenance row");
                assert_eq!(
                    evidence["disposition"]["FormulaEvidence"]["formula_origins"],
                    serde_json::json!([formula["id"].clone()]),
                    "{evidence:#}"
                );
                let evidence_id = evidence["id"].as_str().expect("physical evidence ID");
                let evidence_anchor = evidence["anchor"].as_str().expect("evidence anchor");
                let evidence_line =
                    format!("formula-evidence {evidence_id} {formula_id} {evidence_anchor}");
                assert!(text.lines().any(|line| line == evidence_line));
            }
            let witness = report["refutations"]
                .as_array()
                .expect("refutations")
                .iter()
                .find(|row| row["engine"] == "native")
                .expect("native witness");
            let premises = witness["premises"].to_string();
            for term in ["/Dst", "/Opposition"] {
                assert!(premises.contains(term), "missing {term} from {premises}");
            }
            for source in ["/lawSource", "/guardSource", "/otherSource"] {
                assert!(!premises.contains(source), "{premises}");
            }
        } else {
            assert!(
                !blocked["disposition"]["OutsideRequestedTheory"].is_null(),
                "{blocked:#}"
            );
        }
    }
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
    assert_eq!(code, 3);
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

/// An authored existential rule record whose head the engine cannot carry is declined
/// like any other unsupported construct: a named drop that states the rule and its
/// head relation, beside the drops named before the engine ran. The selection is not
/// decided; it is not an execution failure.
#[test]
fn an_authored_rule_record_the_engine_cannot_carry_is_a_named_drop() {
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
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "refused");
    assert_eq!(report["selection"]["complete"], false);
    assert!(
        has_drop(&report, "SCOPED_CLAIM_NOT_READ", "logic:inModule")
            && has_drop(
                &report,
                "UNSUPPORTED_CONSTRUCT",
                "rule <https://example.org/prove/r> with head relations [<https://blackcatinformatics.ca/logic/naryArg0>]"
            ),
        "{report:#}"
    );
}

/// A formula rule that invents a value of the class it fires on, so the invention can
/// feed itself without end: `Src(a) -> exists y. Src(y) and rel(a, y)`.
const SELF_FEEDING_FORMULA: &str = r#"
ex:cycleBody a logic:Formula ; logic:relation logic:instanceOf ;
    logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ],
                   [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Src ] .
ex:cycleHeadClass a logic:Formula ; logic:relation logic:instanceOf ;
    logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "Y" ],
                   [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Src ] .
ex:cycleHeadRel a logic:Formula ; logic:relation ex:rel ;
    logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ],
                   [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "Y" ] .
ex:cycleHead a logic:Formula ; logic:and ex:cycleHeadClass, ex:cycleHeadRel .
ex:cycleExists a logic:Formula ;
    logic:quantifiedVariable [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "Y" ] ;
    logic:exists ex:cycleHead .
ex:cycleImplication a logic:Formula ; logic:antecedent ex:cycleBody ; logic:consequent ex:cycleExists .
ex:cycleLaw a logic:Formula ;
    logic:quantifiedVariable [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "A" ] ;
    logic:forall ex:cycleImplication .
ex:a logic:instanceOf ex:Src .
"#;

/// A termination refusal names the rule of the cycle by its compiler-produced identity
/// and, because the compiled theory has one, the authored formula it came from. This small program
/// stays under every analysis bound, so none is expected; a cycle in a program that crossed a
/// bound would also carry the bound drop beside the reasons the coarser analysis found.
#[test]
fn a_real_existential_cycle_is_a_named_refusal_with_its_authored_formula() {
    let (report, code) = prove_json("self-feeding.logic.ttl", SELF_FEEDING_FORMULA, &["--gate"]);
    assert_eq!(report["verdict"], "not-decided", "{report:#}");
    assert_eq!(code, 3);
    assert_eq!(report["engines"]["native"]["verdict"], "refused");
    assert!(
        has_drop(
            &report,
            "NATIVE_REFUSAL_REASON",
            "(authored formula <https://example.org/prove/cycleLaw>)"
        ),
        "{report:#}"
    );
    assert!(
        has_drop(
            &report,
            "NATIVE_REFUSAL_REASON",
            "<https://example.org/prove/rel>"
        ),
        "the rule's head relations are named: {report:#}"
    );
    assert!(
        !drop_codes(&report)
            .iter()
            .any(|code| code == "NATIVE_ANALYSIS_BOUND"),
        "no analysis bound is expected for this small program: {report:#}"
    );
}

/// A refused report lists the lowering residue a completed run of the same selection
/// lists: both read it from one preparation.
#[test]
fn a_refused_report_lists_the_lowering_drops_a_completed_run_lists() {
    let (completed, _) = prove_json("disjunctive.logic.ttl", DISJUNCTIVE, &["--gate"]);
    let residue = |report: &serde_json::Value| -> Vec<String> {
        report["selection"]["drops"]
            .as_array()
            .expect("selection drops")
            .iter()
            .filter(|drop| drop["code"] == "UNSUPPORTED_CONSTRUCT")
            .map(|drop| drop["detail"].as_str().expect("detail").to_owned())
            .collect()
    };
    let expected = residue(&completed);
    assert!(!expected.is_empty(), "{completed:#}");
    let (refused, code) = prove_json(
        "refused-with-residue.logic.ttl",
        &format!("{SELF_FEEDING_FORMULA}{DISJUNCTIVE}"),
        &["--gate"],
    );
    assert_eq!(code, 3, "{refused:#}");
    assert_eq!(
        refused["engines"]["native"]["verdict"], "refused",
        "{refused:#}"
    );
    let listed = residue(&refused);
    for construct in &expected {
        assert!(listed.contains(construct), "{construct}: {refused:#}");
    }
}

/// A program whose native termination is certified only for its own input stays
/// decided however many inert rules pad it: the size of the template decides only
/// whether it is cached.
#[test]
fn a_program_padded_past_the_cacheable_bound_is_decided_rather_than_refused() {
    let mut body = String::from(
        r#"
ex:A logic:subClassOf [ a logic:Restriction ; logic:onProperty ex:p ;
    logic:minQualifiedCardinality 2 ; logic:onClass logic:Thing ] .
ex:a logic:instanceOf ex:A .
_:x a logic:TermCarrier ; logic:termVariable "x" .
"#,
    );
    for index in 0..900 {
        body.push_str(&format!(
            "ex:pad{index} a logic:Rule ; logic:head [ rdf:reifies <<( _:x ex:padTo{index} _:x )>> ] ; \
             logic:body [ rdf:reifies <<( _:x ex:padFrom{index} _:x )>> ] .\n"
        ));
    }
    let (report, _) = prove_json("padded.logic.ttl", &body, &["--gate"]);
    assert_eq!(
        report["engines"]["native"]["verdict"], "no-clash",
        "{report:#}"
    );
    assert!(
        !drop_codes(&report)
            .iter()
            .any(|code| code == "NATIVE_REFUSED" || code == "NATIVE_ANALYSIS_BOUND"),
        "{report:#}"
    );
}
