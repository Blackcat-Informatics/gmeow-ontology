// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust integration tests for the constitution-as-code gate.
//!
//! Migrates the 16 Python cases from `tests/test_constitution.py` to native Rust.
//! Every failure mode the gate exists to catch is recreated in a temporary
//! manifest/constitution pair so the regressions survive any future fix to the
//! real data.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use gmeow_errors::Severity;
use gmeow_validate::constitution::{
    collect_principles, constitution_full_report, constitution_headings,
};
use purrdf::RdfDataset;

const PREFIXES: &str = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
     @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";

const MINIMAL_CONSTITUTION: &str = "## 1. Be good\n\nprose\n";

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn load_dataset(ttl: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

fn write_pair(
    tmp: &tempfile::TempDir,
    manifest_ttl: &str,
    constitution_md: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let manifest = tmp.path().join("constitution.ttl");
    fs::write(&manifest, format!("{PREFIXES}{manifest_ttl}")).unwrap();
    let constitution = tmp.path().join("CONSTITUTION.md");
    fs::write(&constitution, constitution_md).unwrap();
    (manifest, constitution)
}

fn run_report(manifest: &Path, constitution: &Path, root: &Path) -> Vec<gmeow_errors::Finding> {
    constitution_full_report(manifest, constitution, root)
}

#[test]
fn constitution_report_uses_granular_codes() {
    let root = repo_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let findings = run_report(&manifest, &constitution, root);

    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let codes: BTreeSet<_> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(!codes.contains("constitution.error"));
    assert!(!codes.contains("constitution.warning"));
    assert!(codes.contains("constitution.honor-system"));
}

#[test]
fn real_manifest_passes() {
    let root = repo_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let findings = run_report(&manifest, &constitution, root);
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn every_principle_has_a_manifest_entry() {
    let root = repo_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");

    let ttl = fs::read_to_string(&manifest).unwrap();
    let dataset = load_dataset(&ttl);
    let principles = collect_principles(&dataset);
    let md_text = fs::read_to_string(&constitution).unwrap();
    let headings = constitution_headings(&md_text);

    let manifest_map: BTreeMap<i64, String> = principles
        .into_iter()
        .map(|p| (p.number, p.title))
        .collect();
    assert_eq!(manifest_map, headings);
}

#[test]
fn principle_18_native_rdf12_stack_enforced() {
    let root = repo_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let ttl = fs::read_to_string(&manifest).unwrap();
    let dataset = load_dataset(&ttl);
    let principles = collect_principles(&dataset);

    let by_number: BTreeMap<i64, _> = principles.into_iter().map(|p| (p.number, p)).collect();
    let p18 = by_number
        .get(&18)
        .expect("Principle 18 missing from the manifest");
    assert_eq!(
        p18.title,
        "The reference RDF-1.2 stack — complete, coherent, and Docker-free"
    );

    let enforcers: BTreeSet<_> = p18.enforced_by.iter().map(String::as_str).collect();
    let meta = "https://blackcatinformatics.ca/gmeow/meta#";
    assert!(enforcers.contains(&format!("{meta}gate-reason-native")[..]));
    assert!(enforcers.contains(&format!("{meta}gate-dl-el-crosscheck")[..]));
}

#[test]
fn honor_system_principles_are_visible_not_silent() {
    let root = repo_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let findings = run_report(&manifest, &constitution, root);

    let flagged: BTreeSet<i64> = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning && f.message.contains("review practice"))
        .filter_map(|f| {
            // "principle N (...) is enforced only by review practice ..."
            f.message
                .split_whitespace()
                .nth(1)
                .and_then(|n| n.parse().ok())
        })
        .collect();
    assert_eq!(flagged, BTreeSet::from([1, 6, 15]));
}

#[test]
fn zero_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement"))
    );
}

#[test]
fn stale_artifact_reference_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ; meta:artifact \"no/such/file.py\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:gate-x .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("'no/such/file.py' does not exist"))
    );
}

#[test]
fn stale_symbol_make_target_and_cli_command_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let py_dir = tmp.path().join("src/gmeow_tools");
    fs::create_dir_all(&py_dir).unwrap();
    fs::write(py_dir.join("validate.py"), "def real_function(): pass\n").unwrap();

    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ;\n\
         meta:artifact \"src/gmeow_tools/validate.py\" ;\n\
         meta:symbol \"no_such_function\" ;\n\
         meta:makeTarget \"no-such-target\" ;\n\
         meta:cliCommand \"no-such-command\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:gate-x .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    assert!(text.contains("'no_such_function' not found"), "{text}");
    assert!(text.contains("Makefile target 'no-such-target'"), "{text}");
    assert!(text.contains("CLI command 'no-such-command'"), "{text}");
}

#[test]
fn orphaned_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-used a meta:Gate ; meta:artifact \"Makefile\" .\n\
         meta:gate-orphan a meta:Lint ; meta:artifact \"Makefile\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:gate-used .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message.contains("orphaned enforcement") && f.message.contains("gate-orphan")
    }));
}

#[test]
fn title_drift_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be excellent\" ;\n\
         meta:enforcedBy meta:gate-x .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| f.message.contains("title drift")));
}

#[test]
fn undeclared_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:nonexistent-gate .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("undeclared enforcement"))
    );
}

#[test]
fn practice_only_principle_warns_not_errors() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:practice-x a meta:Practice ; meta:artifact \"Makefile\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:practice-x .\n",
        MINIMAL_CONSTITUTION,
    );
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Warning
                && f.message.contains("only by review practice"))
    );
    assert!(
        !findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement"))
    );
}

const SUPERSESSION_MANIFEST: &str = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
     meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
     meta:enforcedBy meta:gate-x .\n\
     meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
     meta:enforcedBy meta:gate-x ; meta:supersededInPartBy meta:Principle1 .\n";

const SUPERSESSION_MD: &str = "## 1. Be good\n\nprose\n\n\
     ## 2. Be great\n\n\
     **Superseded in part by Principle 1:** because reasons.\n";

#[test]
fn supersession_matching_pair_passes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(&tmp, SUPERSESSION_MANIFEST, SUPERSESSION_MD);
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(
        !findings
            .iter()
            .any(|f| f.message.contains("supersededInPartBy drift"))
    );
}

#[test]
fn supersession_markdown_only_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl =
        SUPERSESSION_MANIFEST.replace(" ; meta:supersededInPartBy meta:Principle1", "");
    let (manifest, constitution) = write_pair(&tmp, &manifest_ttl, SUPERSESSION_MD);
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message
            .contains("principle 2 meta:supersededInPartBy drift")
    }));
}

#[test]
fn supersession_ttl_only_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\nno marker here.\n";
    let (manifest, constitution) = write_pair(&tmp, SUPERSESSION_MANIFEST, md);
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message
            .contains("principle 2 meta:supersededInPartBy drift")
    }));
}

#[test]
fn extends_matching_pair_passes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
         meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
         meta:enforcedBy meta:gate-x .\n\
         meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
         meta:enforcedBy meta:gate-x ; meta:extends meta:Principle1 .\n";
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Extends Principle 1.**\n";
    let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
    let findings = run_report(&manifest, &constitution, tmp.path());
    assert!(!findings.iter().any(|f| f.message.contains("extends drift")));
}
