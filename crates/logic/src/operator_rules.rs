// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound operator rules prepared by the optimized producer.
//!
//! Hydration retains the original typed program, diagnostics and lowering residue.
//! It never parses authored RDF, repeats formula lowering or certifies a rewrite.

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::fixture;

use std::sync::OnceLock;

use gmeow_logic_compile::frontend::{CompiledTheory, Diagnostic, Severity};
use gmeow_logic_compile::ir::{LogicProgram, SemanticProfileId};
use purrdf::RdfDataset;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::materialize::{Materialization, MaterializationLimits, MaterializeError};
use crate::program_analysis::{PreparedProgram, prepare_program};
use crate::result::PreservationClaim;

/// Compact authenticated source export of the exact native operator rules.
pub const PREPARED_OPERATOR_CHANNEL: &str = "pipeline/prepared-operator-rules.json";
/// Required member of the captured bundle's reasoning archive.
pub const PREPARED_OPERATOR_MEMBER: &str = "reason/prepared-operator-rules.json";
/// Original standalone source, never the augmented example/import theory.
pub const OPERATOR_SOURCE_PATH: &str = "slices/grounding/logic/module.ttl";
/// The original module's explicit provenance identity.
pub const OPERATOR_SOURCE_IRI: &str = crate::reason::enactment::LOGIC_MODULE_SOURCE_IRI;

/// Immutable native lowering and its complete original-source metadata.
///
/// The caller must obtain these bytes through its authenticated producer selection
/// or captured bundle. Source validation is an identity check, not a proof that an
/// arbitrary caller's JSON faithfully implements the source.
#[derive(Debug, Serialize, Deserialize)]
pub struct PreparedOperatorRules {
    schema_version: u32,
    source_digest: String,
    profile: SemanticProfileId,
    program: LogicProgram,
    diagnostics: Vec<Diagnostic>,
    lowering: PreparedProgram,
    means_end: LogicProgram,
    means_end_lowering: PreparedProgram,
}

fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Lower {
        detail: detail.into(),
    })
}

impl PreparedOperatorRules {
    /// Retain the shared original-source compilation and its native lowerings once.
    ///
    /// # Errors
    /// Rejects a different source identity, frontend errors, unsupported rule scope,
    /// missing means-end rules or an invalid native lowering.
    pub fn from_compiled_source(
        compiled: &CompiledTheory,
        source_digest: &str,
    ) -> gmeow_errors::Result<Self> {
        let program = compiled.program();
        let means_end = crate::reason::enactment::refine::select_means_end_program(program)?;
        let prepared = Self {
            schema_version: 2,
            source_digest: source_digest.to_owned(),
            profile: SemanticProfileId::PositiveHorn,
            program: program.clone(),
            diagnostics: compiled.diagnostics().to_vec(),
            lowering: prepare_program(program)?.publication(),
            means_end_lowering: prepare_program(&means_end)?.publication(),
            means_end,
        };
        prepared.validate_source_identity()?;
        Ok(prepared)
    }

    /// Validate a producer-selected native preparation without reconstructing a source.
    ///
    /// # Errors
    /// Rejects stale identities, an unknown profile/schema or unsupported scoped rules.
    pub fn validate_source_identity(&self) -> gmeow_errors::Result<()> {
        static DIGEST: OnceLock<String> = OnceLock::new();
        let digest = DIGEST.get_or_init(|| {
            format!(
                "{:x}",
                Sha256::digest(crate::reason::enactment::LOGIC_MODULE_TTL.as_bytes())
            )
        });
        if self.schema_version != 2
            || &self.source_digest != digest
            || self.profile != SemanticProfileId::PositiveHorn
            || self.program.source_iri.as_deref() != Some(OPERATOR_SOURCE_IRI)
            || self.means_end.source_iri.as_deref() != Some(OPERATOR_SOURCE_IRI)
        {
            return Err(fail(
                "prepared operator rules differ from the selected PositiveHorn source identity",
            ));
        }
        if let Some(diagnostic) = self
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Err(fail(format!(
                "operator rule compilation carries {}: {}",
                diagnostic.code, diagnostic.message
            )));
        }
        self.lowering.admission.admit_world_local_template()?;
        self.means_end_lowering
            .admission
            .admit_world_local_template()?;
        if self.means_end.rules.is_empty()
            || self.means_end_lowering.rules.is_empty()
            || !self.means_end_lowering.existential_rules.is_empty()
            || !self
                .means_end_lowering
                .preservation
                .unsupported_constructs
                .is_empty()
        {
            return Err(fail(
                "prepared means-end rules must be a complete nonempty native Horn selection",
            ));
        }
        Ok(())
    }

    /// Decode an already-authenticated compact payload; no parser/lowerer runs here.
    ///
    /// # Errors
    /// Rejects malformed native data or mismatched source identity.
    pub fn from_bytes(bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let prepared: Self =
            serde_json::from_slice(bytes).map_err(|error| fail(error.to_string()))?;
        prepared.validate_source_identity()?;
        Ok(prepared)
    }

    /// Original source diagnostics, including every warning about omitted constructs.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Complete native lowering residue; a successful cache read cannot strengthen it.
    #[must_use]
    pub fn preservation(&self) -> &PreservationClaim {
        &self.lowering.preservation
    }

    /// The exact original authored source byte digest.
    #[must_use]
    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    /// Execute the already-lowered PositiveHorn selection in explicitly supplied worlds.
    ///
    /// # Errors
    /// Preserves native materialization refusals and limits.
    pub fn materialize(
        &self,
        input: &RdfDataset,
        limits: MaterializationLimits,
    ) -> Result<Materialization, MaterializeError> {
        crate::materialize::materialize_prepared(&self.lowering, input, limits, self.profile)
    }

    pub(crate) fn means_end_preparation(&self) -> (&LogicProgram, &PreparedProgram) {
        (&self.means_end, &self.means_end_lowering)
    }
    pub(crate) fn materialize_store(
        &self,
        store: &crate::store::WorldStore,
    ) -> Result<Materialization, MaterializeError> {
        crate::materialize::materialize_prepared_store(
            &self.lowering,
            store,
            MaterializationLimits { max_steps: None },
            self.profile,
        )
    }
}

#[path = "operator_rules.tests.rs"]
#[cfg(test)]
mod tests;
