// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External, independently authored artifacts exercise the installed math command.
//! These tests never produce the repository corpus or invoke a producer.

use std::path::Path;
use std::process::{Command, Output};

fn inputs(dir: &Path) {
    // gmeow-test-input: synthetic-only
    std::fs::write(dir.join("source.ttl"), "@prefix math: <https://blackcatinformatics.ca/math/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . <urn:math:leaf> a math:NumberLiteral ; math:literalValue <urn:math:seven> . <urn:math:seven> a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator \"+007\"^^xsd:integer ; math:denominator 1 .").unwrap();
    std::fs::write(dir.join("reference.ttl"), "").unwrap();
    std::fs::write(
        dir.join("source.omobj"),
        "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMI>7</OMI></OMOBJ>",
    )
    .unwrap();
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn project(dir: &Path, out: &str, ledger: &str) -> Output {
    run(
        dir,
        &[
            "math",
            "project",
            "source.ttl",
            "--subject",
            "urn:math:leaf",
            "--to",
            "openmath",
            "--reference",
            "reference.ttl",
            "--out",
            out,
            "--ledger",
            ledger,
        ],
    )
}

#[test]
fn math_projection_cli_keeps_artifact_and_ledger_distinct() {
    let dir = tempfile::tempdir().unwrap();
    inputs(dir.path());
    let output = project(dir.path(), "./same.out", "same.out");
    assert!(!output.status.success());
    assert!(!dir.path().join("same.out").exists());

    // Publishing each regular file atomically must also separate old hard links.
    std::fs::write(dir.path().join("out.omobj"), "old").unwrap();
    std::fs::hard_link(dir.path().join("out.omobj"), dir.path().join("loss.ttl")).unwrap();
    let output = project(dir.path(), "out.omobj", "loss.ttl");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        std::fs::read_to_string(dir.path().join("out.omobj"))
            .unwrap()
            .contains("<OMI>7</OMI>")
    );
    assert!(
        std::fs::read_to_string(dir.path().join("loss.ttl"))
            .unwrap()
            .contains("ProjectionRecord")
    );
}

#[cfg(unix)]
#[test]
fn math_projection_cli_rejects_a_dangling_alias_between_outputs() {
    let dir = tempfile::tempdir().unwrap();
    inputs(dir.path());
    std::os::unix::fs::symlink("same.out", dir.path().join("alias.out")).unwrap();
    let output = project(dir.path(), "alias.out", "same.out");
    assert!(!output.status.success());
    assert!(!dir.path().join("same.out").exists());
}

#[cfg(unix)]
#[test]
fn math_projection_cli_preserves_output_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    inputs(dir.path());
    let expected = dir.path().join("ordinary.out");
    std::fs::write(&expected, "ordinary output").unwrap();
    let output = project(dir.path(), "out.omobj", "loss.ttl");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created_mode = std::fs::metadata(expected).unwrap().permissions().mode() & 0o777;
    for name in ["out.omobj", "loss.ttl"] {
        let path = dir.path().join(name);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            created_mode
        );
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let output = project(dir.path(), "out.omobj", "loss.ttl");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for name in ["out.omobj", "loss.ttl"] {
        assert_eq!(
            std::fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}

#[test]
fn math_projection_cli_names_the_failing_input_and_operation() {
    let dir = tempfile::tempdir().unwrap();
    inputs(dir.path());
    std::fs::write(dir.path().join("broken.ttl"), "not valid Turtle").unwrap();
    for (leaf, source, reference, expected) in [
        (
            "project",
            "source.ttl",
            "missing-reference.ttl",
            "missing-reference.ttl",
        ),
        ("project", "source.ttl", "broken.ttl", "broken.ttl"),
        ("project", "broken.ttl", "reference.ttl", "broken.ttl"),
        (
            "lift",
            "source.omobj",
            "missing-reference.ttl",
            "missing-reference.ttl",
        ),
        ("lift", "source.omobj", "broken.ttl", "broken.ttl"),
    ] {
        let mut args = vec![
            "--console",
            "jsonl",
            "math",
            leaf,
            source,
            "--subject",
            "urn:math:leaf",
            "--reference",
            reference,
            "--out",
            "out.ttl",
        ];
        if leaf == "project" {
            args.extend(["--to", "openmath", "--ledger", "loss.ttl"]);
        } else {
            args.extend(["--from", "openmath"]);
        }
        let output = run(dir.path(), &args);
        assert!(!output.status.success());
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(rendered.contains(expected), "{rendered}");
        assert!(
            rendered.contains(&format!("gmeow math {leaf}")),
            "{rendered}"
        );
    }
}
