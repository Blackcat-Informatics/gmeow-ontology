// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned inventory over the original complete source compilation.
//! Decoding this observation never restores the live admission authority required
//! by the FOF writer, and a complete inventory is not a prepared native proof base.

use crate::frontend::{
    CompiledTheory, FofSemanticProfile, RootSelection, SourceAdmission, SourceSelection,
};
use gmeow_errors::DiagKind as _;

mod json;

/// Producer artifact, captured before worked-example augmentation.
pub const CANON_ADMISSION_CHANNEL: &str = "pipeline/canon-proof-admission.json";
/// Required member of the bundle's reasoning archive.
pub const CANON_ADMISSION_MEMBER: &str = "reason/canon-proof-admission.json";

/// Typed observation of every source owner's admission, including all refusals.
#[derive(Debug)]
pub struct CanonAdmissionPublication {
    schema_version: u32,
    admission: SourceAdmission,
}

impl CanonAdmissionPublication {
    /// Inventory all recorded documents, formula roots, rules and semantic owners.
    /// A blocked or malformed canon is retained explicitly, never filtered away.
    #[must_use]
    pub fn from_compiled_source(theory: &CompiledTheory) -> Self {
        Self {
            schema_version: 1,
            admission: SourceAdmission::classical_fof(
                theory,
                SourceSelection::all_classical_roots(theory),
            ),
        }
    }

    /// Read-only admission evidence; decoded observations cannot authorize proof.
    #[must_use]
    pub fn admission(&self) -> &SourceAdmission {
        &self.admission
    }

    /// Encode the complete deterministic observation, not executable source code.
    ///
    /// # Errors
    /// Retains native storage failures if the complete observation cannot be encoded.
    pub fn to_bytes(&self) -> gmeow_errors::Result<Vec<u8>> {
        let value = purrdf_lex::json::record::ToJson::to_json(self);
        let mut resident = purrdf_lex::allocation::Resident;
        let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
        purrdf_lex::json::write_with_memory(&value, purrdf_lex::json::Format::COMPACT, &mut memory)
            .map(String::into_bytes)
            .map_err(|error| codec_failure("encode canon proof admission", error))
    }

    /// Decode an observation without rebuilding source or restoring live authority.
    /// Records require named fields, including publication, admission, complexity
    /// labels and class definitions; their positional struct arrays are refused.
    /// Genuine tuple, literal and collection arrays retain their representation.
    ///
    /// # Errors
    /// Rejects corrupt bytes, unknown schema/profile, changed scope or selection identity.
    pub fn from_bytes(bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let publication: Self = purrdf_lex::json::record::from_slice(bytes)
            .map_err(|error| codec_failure("decode canon proof admission", error))?;
        let admission = &publication.admission;
        let selection = &admission.selection;
        if publication.schema_version != 1
            || admission.profile != FofSemanticProfile::default()
            || selection.formulas != RootSelection::All
            || selection.rules != RootSelection::All
            || selection.owners != RootSelection::All
            || admission.selection_digest != selection.digest()
        {
            return Err(fail(
                "canon proof admission has an incompatible schema, profile or selection",
            ));
        }
        Ok(publication)
    }
}

/// Preserve publication refusal under the shared frontend diagnostic code and grade.
fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Frontend {
        detail: detail.into(),
    })
}

fn codec_failure(
    operation: &str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> gmeow_errors::Diag {
    let kind = crate::error::Frontend {
        detail: format!("{operation}: {source}"),
    };
    let mut diagnostic =
        gmeow_errors::Diag::new(kind.code(), kind.grade(), kind.to_string()).with_source(source);
    if let Some(class) = kind.failure_class() {
        diagnostic = diagnostic.with_failure_class(class);
    }
    diagnostic
}
