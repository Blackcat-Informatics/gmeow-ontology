// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime admission of the producer selected by the explicit build stage.

use std::sync::OnceLock;

use gmeow_action_cache::ProducerIdentity;
use gmeow_action_cache::executable::ExecutableReceipt;
use gmeow_errors::Diag;

use crate::{Commands, LogicCommands, TestFixtureMode, TestFixtureScope};

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
/// Metadata inspection and fixture verification remain read-only. Compiler-owner and
/// docs verification also require current executable and source admission. Console assembly
/// performs its own admission after refusing output paths owned by synchronization.
pub(crate) fn requires_admission(command: &Commands) -> bool {
    match command {
        Commands::Sync(crate::SyncArgs {
            metadata,
            list_paths,
            ..
        }) => !metadata && !list_paths,
        Commands::TestFixtures(crate::TestFixturesArgs { mode, scope, .. }) => {
            matches!(mode, TestFixtureMode::Produce)
                || matches!(
                    scope,
                    TestFixtureScope::CompilerOwner | TestFixtureScope::Docs
                )
        }
        Commands::Fanout(_)
        | Commands::NativeReceiptVectors
        | Commands::MediumSweep(_)
        | Commands::PipelineReads(_)
        | Commands::PipelineScopeCheck
        | Commands::MediumSeed(_)
        | Commands::TermReleaseAuthority(_)
        | Commands::DocsMeasure
        | Commands::DocsMeasureVerify
        | Commands::DocsPackage(_)
        | Commands::DocLint
        | Commands::Normalize
        | Commands::Transform(_)
        | Commands::UpProject(_)
        | Commands::Crossref
        | Commands::ImportFoundation(_)
        | Commands::SliceSpecWorker(_)
        | Commands::ReleaseBundle(_)
        | Commands::Validate(_)
        | Commands::Reason(_)
        | Commands::Verify(_)
        | Commands::ReasonVerify(_)
        | Commands::Explain
        | Commands::Mappings
        | Commands::Build
        | Commands::Project(_)
        | Commands::Acceptance(_)
        | Commands::CompileGts(_)
        | Commands::Feedback(_)
        | Commands::Logic(crate::LogicArgs {
            command: LogicCommands::Compile { .. },
        })
        | Commands::Mcp
        | Commands::SliceQuality(_)
        | Commands::SliceQualityGate
        | Commands::SliceQualitySeedFloors(_)
        | Commands::SliceQualitySeedCeilings { .. }
        | Commands::SliceQualityRelocationPreview(_) => true,
        // Console output-path refusal precedes admission; the handler admits
        // the producer before opening the bundle or rendering any output.
        Commands::ConsoleAssemble(_) => false,
        // Source inspection, user-input queries, and authoring maintenance do
        // not produce the corpus. Keep this exhaustive so new commands must
        // explicitly declare their producer boundary.
        Commands::BuildIdentity
        | Commands::Version
        | Commands::Info
        | Commands::GtsFrameProfile(_)
        | Commands::MediumGate(_)
        | Commands::ExternalTool(_)
        | Commands::ConstitutionCheck
        | Commands::Audit(_)
        | Commands::ComplianceReport(_)
        | Commands::Temporal(_)
        | Commands::Extract(_)
        | Commands::LintAlignment(_)
        | Commands::CrateCheck
        | Commands::FuzzSubstrateCheck
        | Commands::RefreshTargetAxioms(_)
        | Commands::Wikidata(_)
        | Commands::WikidataCoverage(_)
        | Commands::DcCoverage(_)
        | Commands::UpProjectionAudit(_)
        | Commands::Coverage(_)
        | Commands::Quality(_)
        | Commands::Describe(_)
        | Commands::ShapeEquivalence(_)
        | Commands::ShapeLift(_)
        | Commands::ShapeMigrate(_)
        | Commands::ConformanceBless(_)
        | Commands::Certify(_)
        | Commands::SliceQualityProjectionDebt { .. }
        | Commands::SliceFixDeps(_)
        | Commands::BoxRoles(_)
        | Commands::Logic(crate::LogicArgs {
            command: LogicCommands::Query { .. },
        })
        | Commands::I18n(_) => false,
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
