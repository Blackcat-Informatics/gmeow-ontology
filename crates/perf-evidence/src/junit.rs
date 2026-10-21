// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticate one or more nextest JUnit reports as a canonical test inventory.
//!
//! JUnit is observational evidence: durations and outcomes never decide ontology
//! correctness here.  The canonical identity list does, however, let performance
//! comparisons prove that both variants executed the same selected tests.

use crate::{PerfResult as InventoryResult, write_json_atomic};
use purrdf_lex::json::{Object, Value, record::ToJson as _};

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct Testcase {
    classname: String,
    name: String,
    status: &'static str,
    duration_micros: u64,
}

/// Execute the JUnit inventory CLI, refusing malformed or ambiguous evidence.
pub fn run_cli() {
    match parse_args().and_then(|(output, inputs)| run(&output, &inputs)) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("junit-inventory: {error}");
            std::process::exit(1);
        }
    }
}

fn parse_args() -> InventoryResult<(PathBuf, Vec<PathBuf>)> {
    let mut args = std::env::args().skip(1);
    let mut output = None;
    let mut inputs = Vec::new();
    while let Some(argument) = args.next() {
        if argument == "--output" {
            output = Some(PathBuf::from(
                args.next().ok_or("--output requires a path")?,
            ));
        } else if argument.starts_with('-') {
            return Err(format!("unknown argument {argument:?}").into());
        } else {
            inputs.push(PathBuf::from(argument));
        }
    }
    let output = output.ok_or("usage: junit-inventory --output <receipt.json> <junit.xml>...")?;
    if inputs.is_empty() {
        return Err("at least one JUnit XML input is required".into());
    }
    Ok((output, inputs))
}

fn run(output: &Path, inputs: &[PathBuf]) -> InventoryResult<()> {
    let mut testcases = Vec::new();
    let mut input_receipts = Vec::new();
    for input in inputs {
        let bytes =
            fs::read(input).map_err(|error| format!("read {}: {error}", input.display()))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| format!("{} is not UTF-8: {error}", input.display()))?;
        testcases.extend(parse_junit(text, input)?);
        input_receipts.push(Value::from(
            Object::new()
                .with("path", (input.display().to_string()).to_json())
                .with("sha256", (sha256(&bytes)).to_json())
                .with("bytes", (bytes.len()).to_json()),
        ));
    }
    let payload = inventory_payload(testcases, input_receipts)?;
    write_json_atomic(output, &payload)?;
    println!("{}", output.display());
    Ok(())
}

fn inventory_payload(
    mut testcases: Vec<Testcase>,
    input_receipts: Vec<Value>,
) -> InventoryResult<Value> {
    testcases.sort();

    let mut identities = BTreeSet::new();
    for testcase in &testcases {
        let identity = format!("{}\0{}", testcase.classname, testcase.name);
        if !identities.insert(identity) {
            return Err(format!(
                "duplicate JUnit testcase identity: {}::{}",
                testcase.classname, testcase.name
            )
            .into());
        }
    }

    let inventory_sha256 = digest_inventory(&testcases);
    let duration_micros = testcases
        .iter()
        .map(|testcase| testcase.duration_micros)
        .sum::<u64>();
    let passed = testcases
        .iter()
        .filter(|testcase| testcase.status == "passed")
        .count();
    let failed = testcases
        .iter()
        .filter(|testcase| testcase.status == "failed")
        .count();
    let skipped = testcases
        .iter()
        .filter(|testcase| testcase.status == "skipped")
        .count();
    let payload = Value::from(
        Object::new()
            .with("schema_version", SCHEMA_VERSION.to_json())
            .with("command", "junit-inventory".to_json())
            .with(
                "deterministic_work",
                Value::from(
                    Object::new()
                        .with("inventory_sha256", inventory_sha256.to_json())
                        .with("test_count", (testcases.len()).to_json())
                        .with(
                            "identities",
                            (testcases
                                .iter()
                                .map(|testcase| {
                                    Value::from(
                                        Object::new()
                                            .with("classname", testcase.classname.to_json())
                                            .with("name", testcase.name.to_json()),
                                    )
                                })
                                .collect::<Vec<_>>())
                            .to_json(),
                        ),
                ),
            )
            .with(
                "observations",
                Value::from(
                    Object::new()
                        .with("duration_micros", duration_micros.to_json())
                        .with("passed", passed.to_json())
                        .with("failed", failed.to_json())
                        .with("skipped", skipped.to_json())
                        .with(
                            "testcases",
                            (testcases
                                .iter()
                                .map(|testcase| {
                                    Value::from(
                                        Object::new()
                                            .with("classname", testcase.classname.to_json())
                                            .with("name", testcase.name.to_json())
                                            .with("status", testcase.status.to_json())
                                            .with(
                                                "duration_micros",
                                                testcase.duration_micros.to_json(),
                                            ),
                                    )
                                })
                                .collect::<Vec<_>>())
                            .to_json(),
                        )
                        .with("inputs", input_receipts.to_json()),
                ),
            ),
    );
    Ok(payload)
}

fn parse_junit(text: &str, path: &Path) -> InventoryResult<Vec<Testcase>> {
    let document = purrdf_lex::xml::Document::parse(text)
        .map_err(|error| format!("parse {} as JUnit XML: {error}", path.display()))?;
    let mut testcases = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("testcase"))
    {
        let classname = node
            .attribute("classname")
            .ok_or_else(|| format!("{} testcase lacks classname", path.display()))?;
        let name = node
            .attribute("name")
            .ok_or_else(|| format!("{} testcase lacks name", path.display()))?;
        let duration_micros = parse_duration_micros(node.attribute("time").unwrap_or("0"))
            .map_err(|error| format!("{} testcase {classname}::{name}: {error}", path.display()))?;
        let status = if node
            .children()
            .any(|child| child.has_tag_name("failure") || child.has_tag_name("error"))
        {
            "failed"
        } else if node.children().any(|child| child.has_tag_name("skipped")) {
            "skipped"
        } else {
            "passed"
        };
        testcases.push(Testcase {
            classname: classname.to_string(),
            name: name.to_string(),
            status,
            duration_micros,
        });
    }
    if testcases.is_empty() {
        return Err(format!("{} contains no testcase elements", path.display()).into());
    }
    Ok(testcases)
}

fn parse_duration_micros(value: &str) -> InventoryResult<u64> {
    let seconds = value
        .parse::<f64>()
        .map_err(|error| format!("invalid time {value:?}: {error}"))?;
    if !seconds.is_finite() || seconds.is_sign_negative() {
        return Err(format!("invalid non-finite or negative time {value:?}").into());
    }
    let micros = seconds * 1_000_000.0;
    if micros > u64::MAX as f64 {
        return Err(format!("time {value:?} is too large").into());
    }
    Ok(micros.round() as u64)
}

fn digest_inventory(testcases: &[Testcase]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"gmeow:junit-inventory:v1\0");
    for testcase in testcases {
        digest.update(testcase.classname.as_bytes());
        digest.update([0]);
        digest.update(testcase.name.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
#[path = "junit.tests.rs"]
mod tests;
