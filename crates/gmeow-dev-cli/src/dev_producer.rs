// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime admission of the producer selected by the explicit build stage.

use gmeow_action_cache::executable::{ExecutableReceipt, source_digest};
use gmeow_errors::Diag;

use crate::{Commands, LogicCommands, TestFixtureMode};

#[path = "../../../build-support/producer_inputs.rs"]
mod producer_inputs;

/// Classify commands that must authenticate the optimized producer before dispatch.
///
/// Metadata inspection and fixture verification remain read-only. Console assembly
/// performs its own admission after refusing output paths owned by synchronization.
pub(crate) fn requires_admission(command: &Commands) -> bool {
    match command {
        Commands::Sync {
            metadata,
            list_paths,
            ..
        } => !metadata && !list_paths,
        Commands::TestFixtures { mode, .. } => matches!(mode, TestFixtureMode::Produce),
        Commands::Fanout { .. }
        | Commands::MediumSweep { .. }
        | Commands::MediumSeed { .. }
        | Commands::TermReleaseAuthority { .. }
        | Commands::DocsMeasure
        | Commands::DocsPackage { .. }
        | Commands::DocLint
        | Commands::Normalize
        | Commands::Transform { .. }
        | Commands::UpProject { .. }
        | Commands::Crossref
        | Commands::ImportFoundation { .. }
        | Commands::SliceSpecWorker { .. }
        | Commands::ReleaseBundle { .. }
        | Commands::Validate { .. }
        | Commands::Reason { .. }
        | Commands::Verify { .. }
        | Commands::ReasonVerify { .. }
        | Commands::Explain
        | Commands::Mappings
        | Commands::Build
        | Commands::Project { .. }
        | Commands::Acceptance { .. }
        | Commands::CompileGts { .. }
        | Commands::Feedback { .. }
        | Commands::Logic {
            command: LogicCommands::Compile { .. },
        }
        | Commands::Mcp
        | Commands::SliceQuality { .. }
        | Commands::SliceQualityGate
        | Commands::SliceQualitySeedFloors { .. }
        | Commands::SliceQualitySeedCeilings { .. }
        | Commands::SliceQualityRelocationPreview { .. } => true,
        // Console output-path refusal precedes admission; the handler admits
        // the producer before opening the bundle or rendering any output.
        Commands::ConsoleAssemble { .. } => false,
        // Source inspection, user-input queries, and authoring maintenance do
        // not produce the corpus. Keep this exhaustive so new commands must
        // explicitly declare their producer boundary.
        Commands::BuildIdentity
        | Commands::Version
        | Commands::Info
        | Commands::GtsFrameProfile { .. }
        | Commands::MediumGate { .. }
        | Commands::ExternalTool { .. }
        | Commands::ConstitutionCheck
        | Commands::Audit { .. }
        | Commands::ComplianceReport { .. }
        | Commands::Temporal { .. }
        | Commands::Extract { .. }
        | Commands::LintAlignment { .. }
        | Commands::CrateCheck
        | Commands::RefreshTargetAxioms { .. }
        | Commands::Wikidata { .. }
        | Commands::WikidataCoverage { .. }
        | Commands::DcCoverage { .. }
        | Commands::UpProjectionAudit { .. }
        | Commands::Coverage { .. }
        | Commands::Quality { .. }
        | Commands::Describe { .. }
        | Commands::ShapeEquivalence { .. }
        | Commands::ShapeLift { .. }
        | Commands::ShapeMigrate { .. }
        | Commands::Certify { .. }
        | Commands::SliceQualityProjectionDebt { .. }
        | Commands::SliceFixDeps { .. }
        | Commands::BoxRoles { .. }
        | Commands::Logic {
            command: LogicCommands::Query { .. },
        }
        | Commands::I18n { .. } => false,
    }
}

/// Authenticate this running producer and the checkout selected for its operation.
///
/// Require the embedded recipe and compilation policy, exact executable bytes,
/// pipeline profile, and current source inventory to agree with the adjacent
/// receipt. Missing evidence, test/debug identities, and stale sources fail;
/// this boundary never builds or repairs the producer.
pub(crate) fn admit() -> gmeow_errors::Result<()> {
    let fail = |error: String| -> Diag { crate::error::sync(error) };
    let contract = gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT;
    if contract.len() != 64 || !contract.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(fail("this executable is a test/debug build, not an admitted O3/full-LTO producer; run make producer-build".into()));
    }
    let executable = std::env::current_exe().map_err(|e| fail(e.to_string()))?;
    let receipt = ExecutableReceipt::read(&executable.with_extension("receipt.json"))
        .map_err(|e| fail(e.to_string()))?;
    receipt
        .verify(&executable, contract)
        .map_err(|e| fail(e.to_string()))?;
    if receipt
        .recipe
        .compilation_digest()
        .map_err(|e| fail(e.to_string()))?
        != gmeow_pipeline::cache::PRODUCER_COMPILATION_CONTRACT
    {
        return Err(fail(
            "action compilation policy differs from the admitted executable recipe".into(),
        ));
    }
    if receipt.recipe.profile != "pipeline" {
        return Err(fail(
            "producer receipt does not select the pipeline profile".into(),
        ));
    }
    let root = crate::dev_common::project_root();
    let current = source_digest(
        &root,
        producer_inputs::paths(&root, &root.join("crates/gmeow-dev-cli")),
    )
    .map_err(|e| fail(e.to_string()))?;
    if current != receipt.recipe.source_digest {
        return Err(fail(
            "producer source inventory changed; run make producer-build".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// Classify parsed producer commands without executing them or constructing a corpus.
    #[test]
    fn production_commands_require_admission_without_dispatching_them() {
        for arguments in [
            vec!["gmeow-dev", "sync"],
            vec!["gmeow-dev", "test-fixtures", "produce"],
            vec!["gmeow-dev", "feedback"],
            vec!["gmeow-dev", "logic", "compile"],
            vec!["gmeow-dev", "mcp"],
            vec!["gmeow-dev", "medium-sweep"],
            vec!["gmeow-dev", "medium-seed"],
            vec!["gmeow-dev", "normalize"],
            vec!["gmeow-dev", "docs-package"],
            vec!["gmeow-dev", "doc-lint"],
            vec!["gmeow-dev", "explain"],
            vec!["gmeow-dev", "acceptance"],
            vec!["gmeow-dev", "slice-quality-gate"],
            vec!["gmeow-dev", "slice-quality-seed-floors", "--all-axes"],
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
}
