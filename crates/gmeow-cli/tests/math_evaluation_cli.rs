// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production command tests with independent user RDF, never corpus production.

use std::path::Path;
use std::process::{Command, Output};

fn source(divisor: &str) -> String {
    format!(
        r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
<urn:root> a math:ApplicationExpression ; math:operator math:Division ; math:argumentSlot <urn:s0>, <urn:s1> .
<urn:s0> a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression <urn:left> .
<urn:s1> a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression <urn:right> .
<urn:left> a math:SymbolReference ; math:hasMathematicalSymbol <urn:x> .
<urn:right> a math:SymbolReference ; math:hasMathematicalSymbol <urn:y> .
<urn:x> a math:MathematicalSymbol . <urn:y> a math:MathematicalSymbol .
<urn:one> a math:Quantity ; math:hasDimension math:dimensionless ; math:quantityValue 1 .
<urn:divisor> a math:Quantity ; math:hasDimension math:dimensionless ; math:quantityValue {divisor} .
"#
    )
}
fn run(dir: &Path, extra: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
        .current_dir(dir)
        .args([
            "math",
            "evaluate",
            "source.ttl",
            "--subject",
            "urn:root",
            "--bind",
            "urn:x=urn:one",
            "--bind",
            "urn:y=urn:divisor",
            "--out",
            "result.ttl",
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn math_evaluation_cli_runs_without_checkout_and_publishes_native_evidence() {
    let dir = tempfile::tempdir().unwrap();
    // gmeow-test-input: synthetic-only
    std::fs::write(dir.path().join("source.ttl"), source("2")).unwrap();
    let output = run(dir.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = std::fs::read_to_string(dir.path().join("result.ttl")).unwrap();
    assert!(result.contains("0.5"));
    assert!(result.contains("quantityValue"));
    assert!(result.contains("resultDerivedAxiom"));
    assert!(result.contains("urn:s0"));
    assert!(result.contains("rdfNumericExactDivide"));
    assert!(!dir.path().join("generated").exists());
}

#[test]
fn math_evaluation_cli_refuses_nonterminating_zero_and_duplicate_bindings_atomically() {
    let dir = tempfile::tempdir().unwrap();
    for divisor in ["0", "3"] {
        // gmeow-test-input: synthetic-only
        std::fs::write(dir.path().join("source.ttl"), source(divisor)).unwrap();
        std::fs::write(dir.path().join("result.ttl"), "previous complete result").unwrap();
        let output = run(dir.path(), &[]);
        assert!(!output.status.success());
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostics.contains("evaluate"));
        assert!(diagnostics.contains("source.ttl"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("result.ttl")).unwrap(),
            "previous complete result"
        );
    }
    std::fs::write(dir.path().join("source.ttl"), source("2")).unwrap();
    let output = run(dir.path(), &["--bind", "urn:x=urn:divisor"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate --bind"));
}
