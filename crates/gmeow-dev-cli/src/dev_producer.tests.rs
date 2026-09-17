// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use clap::Parser;

use super::*;

/// Classify parsed producer commands without executing them or constructing a corpus.
#[test]
fn production_commands_require_admission_without_dispatching_them() {
    for arguments in [
        vec!["gmeow-dev", "sync"],
        vec!["gmeow-dev", "project", "--profile", "gmeow"],
        vec!["gmeow-dev", "test-fixtures", "produce"],
        vec!["gmeow-dev", "feedback"],
        vec!["gmeow-dev", "logic", "compile"],
        vec!["gmeow-dev", "mcp"],
        vec!["gmeow-dev", "medium-sweep"],
        vec!["gmeow-dev", "medium-seed"],
        vec!["gmeow-dev", "docs-measure-verify"],
        vec!["gmeow-dev", "slice-quality-gate"],
        vec!["gmeow-dev", "slice-quality-seed-floors", "--all-axes"],
        vec![
            "gmeow-dev",
            "slice-quality-relocation-preview",
            "--term",
            "urn:synthetic:term",
            "--from",
            "synthetic-source",
            "--to",
            "synthetic-target",
        ],
        vec!["gmeow-dev", "normalize"],
        vec!["gmeow-dev", "docs-package"],
        vec!["gmeow-dev", "doc-lint"],
        vec!["gmeow-dev", "explain"],
        vec!["gmeow-dev", "acceptance"],
        vec!["gmeow-dev", "slice-quality-seed-ceilings"],
        vec![
            "gmeow-dev",
            "slice-quality-relocation-preview",
            "--term",
            "urn:term",
            "--from",
            "source",
            "--to",
            "target",
        ],
        vec!["gmeow-dev", "term-release-authority"],
        vec!["gmeow-dev", "term-release-authority", "--bootstrap"],
        vec!["gmeow-dev", "crossref"],
        vec!["gmeow-dev", "transform", "input.nq"],
        vec!["gmeow-dev", "up-project", "input.nq"],
        vec![
            "gmeow-dev",
            "import-foundation",
            "input.jsonl",
            "--out",
            "output",
        ],
        vec![
            "gmeow-dev",
            "slice-spec-worker",
            "--kind",
            "structural",
            "--spec",
            "spec.yaml",
        ],
    ] {
        let cli = crate::Cli::try_parse_from(arguments).expect("parse production operation");
        assert!(requires_admission(&cli.command));
    }
    let cli = crate::Cli::try_parse_from(["gmeow-dev", "sync", "--metadata"])
        .expect("parse metadata inspection");
    assert!(!requires_admission(&cli.command));
    let cli = crate::Cli::try_parse_from([
        "gmeow-dev",
        "logic",
        "query",
        "input.nq",
        "query.logic",
        "--json",
    ])
    .expect("parse query over caller-supplied input");
    assert!(!requires_admission(&cli.command));
}

/// Preserve each medium command's explicit output and reject a missing path during parsing.
#[test]
fn medium_maintenance_selects_its_output_before_producer_admission() {
    for (command, expected_seed) in [("medium-sweep", false), ("medium-seed", true)] {
        let cli =
            crate::Cli::try_parse_from(["gmeow-dev", command, "--out", "selected/medium.json"])
                .expect("parse maintenance selection without running the producer");
        assert!(requires_admission(&cli.command));
        let (out, seed) = match cli.command {
            Commands::MediumSweep { out } => (out, false),
            Commands::MediumSeed { out } => (out, true),
            _ => panic!("selected a different maintenance operation"),
        };
        assert_eq!(out, std::path::Path::new("selected/medium.json"));
        assert_eq!(seed, expected_seed);
        assert!(
            crate::Cli::try_parse_from(["gmeow-dev", command, "--out"]).is_err(),
            "an explicit output flag requires its path"
        );
    }
}

/// Reject a test binary whose build carries no admitted producer identity.
#[test]
fn test_executable_cannot_admit_itself_as_a_corpus_producer() {
    assert!(gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT.is_empty());
    assert!(admit().is_err());
}
