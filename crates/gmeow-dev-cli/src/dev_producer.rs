// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime admission of the producer selected by the explicit build stage.

use gmeow_action_cache::executable::{ExecutableReceipt, source_digest};
use gmeow_errors::Diag;

use crate::{Commands, LogicCommands, TestFixtureMode};

#[path = "../../../build-support/producer_inputs.rs"]
mod producer_inputs;

pub(crate) fn requires_admission(command: &Commands) -> bool {
    match command {
        Commands::Sync {
            metadata,
            list_paths,
            ..
        } => !metadata && !list_paths,
        Commands::TestFixtures { mode, .. } => matches!(mode, TestFixtureMode::Produce),
        Commands::Fanout { .. }
        | Commands::DocsMeasure
        | Commands::ReleaseBundle { .. }
        | Commands::Validate { .. }
        | Commands::Reason { .. }
        | Commands::Verify { .. }
        | Commands::ReasonVerify { .. }
        | Commands::Mappings
        | Commands::Build
        | Commands::Project { .. }
        | Commands::CompileGts { .. }
        | Commands::Feedback { .. }
        | Commands::Logic {
            command: LogicCommands::Compile { .. },
        }
        | Commands::Mcp
        | Commands::SliceQuality { .. } => true,
        _ => false,
    }
}

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

    #[test]
    fn production_commands_require_admission_without_dispatching_them() {
        for arguments in [
            vec!["gmeow-dev", "sync"],
            vec!["gmeow-dev", "test-fixtures", "produce"],
            vec!["gmeow-dev", "feedback"],
            vec!["gmeow-dev", "logic", "compile"],
            vec!["gmeow-dev", "mcp"],
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

    #[test]
    fn test_executable_cannot_admit_itself_as_a_corpus_producer() {
        assert!(gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT.is_empty());
        assert!(admit().is_err());
    }
}
