// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Portable complete-source native lowering. Reading it does not restore the live
//! compiler selection, claim ingress, source accounting, or logical world.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use gmeow_errors::DiagKind as _;
use gmeow_logic_compile::frontend::SourceSelection;
use gmeow_logic_compile::ir::TheorySelection;
use gmeow_logic_compile::source_inventory::{SourceInventory, TheorySelectionObservation};
use purrdf_hash::hex::Digest32;
use serde::{Deserialize, Serialize};

use crate::native_semantics::SourceAdmissionProfile;
use crate::program_analysis::{PreparedProgram, selected_source_proof_profile};

/// The compiler-owned native proof artifact before projection augmentation.
pub const CHANNEL: &str = "pipeline/prepared-native-canon.cbor";

const VERSION: u32 = 1;
const MAX_NESTING: usize = 128;

/// Portable identities for one lowering. These do not admit a source or a proof.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCanonBinding {
    /// Exact authored owner and formula lowering trace.
    pub compiler_trace: String,
    /// Source, program, import closure and execution masks of the resolution.
    pub selection: TheorySelectionObservation,
    /// Exact native reasoning kernel and substrate.
    pub native_contract: String,
}

type Denials = BTreeMap<String, String>;
type FormulaIndices = BTreeSet<usize>;
type AuthoredFormulas = BTreeMap<usize, BTreeSet<String>>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record<P, D, I, A> {
    version: u32,
    binding: PreparedCanonBinding,
    lowering: P,
    // These are intentionally not part of the shared operator-rules codec.
    denial_owners: D,
    incomplete_formulas: I,
    authored_formulas: A,
}

/// The decoded lowering and its evidence, not an executable selected program.
#[derive(Debug)]
pub struct PreparedCanonProof {
    record: Record<PreparedProgram, Denials, FormulaIndices, AuthoredFormulas>,
}

impl PreparedCanonProof {
    /// Prepare the complete original native selection once and encode its lowering.
    /// This does not ingest facts or issue a source completion.
    ///
    /// # Errors
    /// Refuses a narrowed source inventory, unresolved imports, lowering failure,
    /// or an encoded artifact exceeding the selected byte budget.
    pub fn encode(
        inventory: &SourceInventory<'_>,
        max_bytes: usize,
    ) -> gmeow_errors::Result<Vec<u8>> {
        if *inventory.selection() != SourceSelection::all_classical_roots(inventory.theory()) {
            return Err(super::reason_err(
                "prepared native canon requires the complete original source inventory".into(),
            ));
        }
        let selection = inventory.resolve_theory_selection(&TheorySelection::complete_source())?;
        let profile = selected_source_proof_profile(inventory, &selection);
        let selected = super::prepare_selected_with_profile(inventory, &selection, profile)?;
        let prepared = selected.prepared.as_ref();
        let record = Record {
            version: VERSION,
            binding: PreparedCanonBinding {
                compiler_trace: inventory.compiler_trace_digest().to_owned(),
                selection: selection.observation().clone(),
                native_contract: super::native_contract_hash(),
            },
            lowering: prepared,
            denial_owners: &prepared.denial_owners,
            incomplete_formulas: &prepared.incomplete_formulas,
            authored_formulas: &selected.authored,
        };
        let mut output = BoundedOutput(Vec::new(), max_bytes);
        ciborium::ser::into_writer(&record, &mut output)
            .map_err(|cause| codec_error("encode prepared native canon", cause))?;
        Ok(output.0)
    }

    /// Decode a bounded native artifact without granting execution authority.
    ///
    /// # Errors
    /// Refuses oversized, malformed, trailing, or incompatible data.
    pub fn decode(bytes: &[u8], max_bytes: usize) -> gmeow_errors::Result<Self> {
        if bytes.len() > max_bytes {
            return Err(super::reason_err(format!(
                "prepared native canon exceeds {max_bytes} bytes"
            )));
        }
        let mut remaining = bytes;
        let record: Record<PreparedProgram, Denials, FormulaIndices, AuthoredFormulas> =
            ciborium::de::from_reader_with_recursion_limit(&mut remaining, MAX_NESTING)
                .map_err(|cause| codec_error("decode prepared native canon", cause))?;
        if !remaining.is_empty()
            || record.version != VERSION
            || record.binding.native_contract != super::native_contract_hash()
            || !record.binding.selection.includes_top_level
            || ![
                &record.binding.compiler_trace,
                &record.binding.selection.source_digest,
                &record.binding.selection.program_digest,
                &record.binding.selection.document_selection_digest,
                &record.binding.selection.effective_masks_digest,
            ]
            .into_iter()
            .all(|digest| Digest32::from_hex(digest).is_some())
            || !matches!(
                &record.lowering.admission.profile,
                SourceAdmissionProfile::SelectedSourceProofV2 {
                    theory, execution_identity, compiler_trace,
                } if *theory == TheorySelection::complete_source()
                    && theory == &record.binding.selection.request
                    && execution_identity == &record.binding.selection.identity
                    && compiler_trace == &record.binding.compiler_trace
            )
        {
            return Err(super::reason_err(
                "prepared native canon has trailing bytes or an incompatible schema, contract or selection".into(),
            ));
        }
        record.binding.selection.verify_identity()?;
        Ok(Self { record })
    }

    /// Inspect the immutable lowering identities without authorizing a proof.
    #[must_use]
    pub fn binding(&self) -> &PreparedCanonBinding {
        &self.record.binding
    }

    /// Producer-recorded preparation key, not a new proof of source authority.
    #[must_use]
    pub fn preparation_identity(&self) -> Digest32 {
        Digest32::new(self.record.lowering.identity)
    }

    /// Named source owner for a denial retained in the payload.
    #[must_use]
    pub fn denial_owner(&self, rule: &str) -> Option<&str> {
        self.record.denial_owners.get(rule).map(String::as_str)
    }

    /// Original formula positions with incomplete lowering.
    #[must_use]
    pub fn incomplete_formulas(&self) -> &BTreeSet<usize> {
        &self.record.incomplete_formulas
    }
}

struct BoundedOutput(Vec<u8>, usize);

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let available = self.1 - self.0.len();
        if available < bytes.len() {
            return Err(io::Error::other(format!(
                "prepared native canon exceeds {} bytes",
                self.1
            )));
        }
        let end = self.0.len() + bytes.len();
        let capacity = self.0.capacity();
        if end > capacity {
            let target = capacity.saturating_mul(2).max(end).min(self.1);
            self.0
                .try_reserve_exact(target - self.0.len())
                .map_err(io::Error::other)?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn codec_error(
    operation: &str,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> gmeow_errors::Diag {
    let kind = crate::error::Reason {
        detail: format!("{operation}: {cause}"),
    };
    gmeow_errors::Diag::new(kind.code(), kind.grade(), kind.to_string()).with_source(cause)
}

#[cfg(test)]
#[path = "prepared_canon.tests.rs"]
mod tests;
