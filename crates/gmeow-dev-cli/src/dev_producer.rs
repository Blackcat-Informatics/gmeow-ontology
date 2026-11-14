// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime admission of the producer selected by the explicit build stage.

use std::sync::OnceLock;

use gmeow_action_cache::ProducerIdentity;
use gmeow_action_cache::executable::ExecutableReceipt;
use gmeow_errors::Diag;

use crate::{Commands, LogicCommands, TestFixtureMode};

static ADMITTED_IDENTITY: OnceLock<ProducerIdentity> = OnceLock::new();

/// Return the exact identity of the producer after executable and source admission.
pub(crate) fn admitted_identity() -> gmeow_errors::Result<ProducerIdentity> {
    ADMITTED_IDENTITY
        .get()
        .cloned()
        .ok_or_else(|| crate::error::sync("validation requires an admitted producer identity"))
}

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
        | Commands::DocsMeasureVerify
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
        | Commands::FuzzSubstrateCheck
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
    receipt
        .verify_current_sources(&root)
        .map_err(|e| fail(e.to_string()))?;
    let current = receipt
        .recipe
        .source_inventory
        .digest()
        .map_err(|e| fail(e.to_string()))?;
    if current != receipt.recipe.source_digest {
        return Err(fail(
            "producer source inventory changed; run make producer-build".into(),
        ));
    }
    let identity = ProducerIdentity {
        digest: contract.to_owned(),
        toolchain: Some(receipt.recipe.rustc.clone()),
        target: None,
        profile: Some(receipt.recipe.profile.clone()),
        features: Vec::new(),
    };
    if let Some(previous) = ADMITTED_IDENTITY.get() {
        if previous != &identity {
            return Err(fail("producer identity changed after admission".into()));
        }
    } else {
        ADMITTED_IDENTITY
            .set(identity)
            .map_err(|_| fail("concurrent producer identity admission".into()))?;
    }
    Ok(())
}

#[path = "dev_producer.tests.rs"]
#[cfg(test)]
mod tests;
