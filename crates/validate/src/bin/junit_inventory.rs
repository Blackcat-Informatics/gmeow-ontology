// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticate one or more nextest JUnit reports as a canonical test inventory.
//!
//! JUnit is observational evidence: durations and outcomes never decide ontology
//! correctness here.  The canonical identity list does, however, let performance
//! comparisons prove that both variants executed the same selected tests.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
struct InventoryError(String);

impl std::fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for InventoryError {}

impl From<String> for InventoryError {
    fn from(detail: String) -> Self {
        Self(detail)
    }
}

impl From<&str> for InventoryError {
    fn from(detail: &str) -> Self {
        detail.to_string().into()
    }
}

type InventoryResult<T> = std::result::Result<T, InventoryError>;

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct Testcase {
    classname: String,
    name: String,
    status: &'static str,
    duration_micros: u64,
}

fn main() {
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
    let output = output.ok_or("usage: junit_inventory --output <receipt.json> <junit.xml>...")?;
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
        input_receipts.push(serde_json::json!({
            "path": input.display().to_string(),
            "sha256": sha256(&bytes),
            "bytes": bytes.len(),
        }));
    }
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
    let payload = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "command": "junit-inventory",
        "deterministic_work": {
            "inventory_sha256": inventory_sha256,
            "test_count": testcases.len(),
            "identities": testcases.iter().map(|testcase| serde_json::json!({
                "classname": testcase.classname,
                "name": testcase.name,
            })).collect::<Vec<_>>(),
        },
        "observations": {
            "duration_micros": duration_micros,
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "testcases": testcases.iter().map(|testcase| serde_json::json!({
                "classname": testcase.classname,
                "name": testcase.name,
                "status": testcase.status,
                "duration_micros": testcase.duration_micros,
            })).collect::<Vec<_>>(),
            "inputs": input_receipts,
        },
    });
    write_json_atomic(output, &payload)?;
    println!("{}", output.display());
    Ok(())
}

fn parse_junit(text: &str, path: &Path) -> InventoryResult<Vec<Testcase>> {
    let document = roxmltree::Document::parse(text)
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

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> InventoryResult<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("junit"),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    let write_result = serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| format!("serialize JUnit receipt: {error}"))
        .and_then(|()| {
            file.write_all(b"\n")
                .and_then(|()| file.sync_all())
                .map_err(|error| format!("flush {}: {error}", temporary.display()))
        })
        .and_then(|()| {
            fs::rename(&temporary, path)
                .map_err(|error| format!("publish {}: {error}", path.display()))
        });
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(write_result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_is_order_independent_and_duration_is_observational() {
        let a = parse_junit(
            r#"<testsuite><testcase classname="c" name="b" time="2.5"/><testcase classname="c" name="a" time="1"><skipped/></testcase></testsuite>"#,
            Path::new("a.xml"),
        )
        .unwrap();
        let b = parse_junit(
            r#"<testsuite><testcase classname="c" name="a" time="99"><skipped/></testcase><testcase classname="c" name="b" time="0"/></testsuite>"#,
            Path::new("b.xml"),
        )
        .unwrap();
        let mut a = a;
        let mut b = b;
        a.sort();
        b.sort();
        assert_eq!(digest_inventory(&a), digest_inventory(&b));
        assert_ne!(
            a.iter().map(|case| case.duration_micros).sum::<u64>(),
            b.iter().map(|case| case.duration_micros).sum::<u64>()
        );
    }

    #[test]
    fn malformed_and_negative_durations_fail_closed() {
        assert!(parse_duration_micros("-1").is_err());
        assert!(parse_duration_micros("NaN").is_err());
        assert!(parse_duration_micros("forever").is_err());
    }
}
