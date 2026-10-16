// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound syntactic evidence retained by the single native execution.

use std::collections::BTreeMap;

use crate::physical::{LogicalGraph, WriterInventory};
use crate::result::{EngineId, PreservationClaim};

use super::obligation_err;

/// The selected operation determines whether a complete program was analyzed.
/// A class-only diagnostic never pretends to supply an empty canonical program.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReachabilityEvidence {
    /// Actual selected canonical program, source contexts and native producers.
    Program(Box<ProgramReachability>),
    /// Explicit class diagnostic profile, with no general negative-proof authority.
    ClassDiagnostic,
}

/// Compact immutable writer analysis. The enclosing native publication supplies
/// authorship; these content identities only validate exact framing and reuse.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProgramReachability {
    identity: [u8; 32],
    input_contract: [u8; 32],
    program: [u8; 32],
    engine: EngineId,
    engine_contract: String,
    lowering: PreservationClaim,
    foundation: WriterInventory,
    worlds: BTreeMap<String, WriterInventory>,
}

impl ProgramReachability {
    pub(super) fn identity(&self) -> [u8; 32] {
        self.identity
    }

    pub(crate) fn new(
        prepared: &crate::program_analysis::PreparedProgram,
        input_contract: [u8; 32],
        worlds: BTreeMap<String, WriterInventory>,
    ) -> Self {
        let mut analysis = Self {
            identity: [0; 32],
            input_contract,
            program: prepared.identity,
            engine: EngineId::native(),
            engine_contract: crate::reason::native_contract_hash(),
            lowering: prepared.preservation.clone(),
            foundation: crate::foundation::writer_inventory().clone(),
            worlds,
        };
        analysis.identity = analysis.content_identity();
        analysis
    }

    fn content_identity(&self) -> [u8; 32] {
        crate::physical::metadata_identity(
            "gmeow-program-reachability-v1",
            &(
                self.input_contract,
                self.program,
                &self.engine,
                &self.engine_contract,
                &self.lowering,
                &self.foundation,
                &self.worlds,
            ),
        )
    }

    pub(crate) fn validate(
        &self,
        input_contract: [u8; 32],
        worlds: &BTreeMap<String, LogicalGraph>,
    ) -> gmeow_errors::Result<()> {
        self.lowering.validate()?;
        if self.input_contract != input_contract
            || self.engine != EngineId::native()
            || self.engine_contract != crate::reason::native_contract_hash()
            || self.identity != self.content_identity()
            || self.foundation != *crate::foundation::writer_inventory()
            || !self.worlds.keys().eq(worlds.keys())
        {
            return Err(obligation_err(
                "native reachability analysis has a stale or foreign program/input/engine/world binding"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) fn source_complete(&self, world: &str, observed_admission: bool) -> bool {
        self.lowering.unsupported_constructs.is_empty()
            && self
                .worlds
                .get(world)
                .is_some_and(|writers| !writers.requires_source_admission || observed_admission)
            && self.lowering.polarities.iter().any(|polarity| {
                matches!(
                    polarity,
                    gmeow_logic_compile::ir::PreservationKind::Exact
                        | gmeow_logic_compile::ir::PreservationKind::CompleteOver
                )
            })
    }

    pub(super) fn may_write(&self, world: &str, predicate: &str) -> gmeow_errors::Result<bool> {
        let writers = self.worlds.get(world).ok_or_else(|| {
            obligation_err("reachability analysis omits the selected source context".to_owned())
        })?;
        Ok(self.foundation.may_write(predicate) || writers.may_write(predicate))
    }

    pub(super) fn inventory_detail(&self, world: &str) -> gmeow_errors::Result<String> {
        let writers = self.worlds.get(world).ok_or_else(|| {
            obligation_err("reachability inventory omits the selected source context".to_owned())
        })?;
        Ok(format!(
            "analysis={}; program={}; input={}; engine={}@{}; world={world}; \
             foundation={}; theory={}; unbounded={}; requires_source_admission={}; examined producers: {}",
            blake3::Hash::from(self.identity).to_hex(),
            blake3::Hash::from(self.program).to_hex(),
            blake3::Hash::from(self.input_contract).to_hex(),
            self.engine.name,
            self.engine.version,
            blake3::Hash::from(self.foundation.theory).to_hex(),
            blake3::Hash::from(writers.theory).to_hex(),
            writers.unbounded,
            writers.requires_source_admission,
            self.foundation
                .producers
                .iter()
                .chain(&writers.producers)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }
}
