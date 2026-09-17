// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact prepared consumer inputs, bound to the same immutable snapshot as the view.

use std::sync::Arc;

use gmeow_bundle_view::bundle_blobs::{Bundle, REP_REASONING};
use gmeow_logic_compile::action_policy::{self, PreparedActionPolicy};

use crate::McpView;
#[cfg(all(target_arch = "wasm32", feature = "reasoning"))]
use gmeow_logic::verify::PreparedReasonedGates;
#[cfg(not(target_arch = "wasm32"))]
use gmeow_validate::data_validate::PreparedReasonedGates;

fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp {
        message: detail.into(),
    })
}

fn decode_policy(bytes: &[u8]) -> gmeow_errors::Result<PreparedActionPolicy> {
    if bytes.len() > action_policy::MAX_BYTES {
        return Err(fail("prepared action policy exceeds its size bound"));
    }
    let policy: PreparedActionPolicy = serde_json::from_slice(bytes)
        .map_err(|error| fail(format!("decode prepared action policy: {error}")))?;
    policy.validate()?;
    Ok(policy)
}

#[cfg(not(target_arch = "wasm32"))]
fn selected_artifact(expected: &str, name: &str) -> gmeow_errors::Result<Arc<[u8]>> {
    if crate::storage()
        .env_var("GMEOW_BUNDLE_IMPORT_SOURCE_SHA256")
        .as_deref()
        != Some(expected)
    {
        return Err(fail(
            "selected native artifact source differs from this retained MCP snapshot",
        ));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    gmeow_bundle_import::load_authenticated_corpus_artifact(&root, name).map(Arc::from)
}

impl McpView {
    /// Retain one fold of the captured snapshot for compact runtime artifact readers.
    fn prepared_bundle(&self) -> gmeow_errors::Result<&Bundle> {
        if let Some(bundle) = self.prepared_bundle.get() {
            return Ok(bundle);
        }
        let bundle = Bundle::from_snapshot(&self.gts)?;
        Ok(self.prepared_bundle.get_or_init(|| bundle))
    }

    fn prepared_member(
        &self,
        artifact: &str,
        representation: &str,
        member: &str,
        limit: usize,
    ) -> gmeow_errors::Result<Arc<[u8]>> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(expected) = self.selected_artifact_sha256.as_deref() {
            // A selected corpus never falls through to archive extraction on a miss.
            let bytes = selected_artifact(expected, artifact)?;
            if bytes.len() > limit {
                return Err(fail(format!(
                    "selected native artifact {artifact} exceeds its size bound"
                )));
            }
            return Ok(bytes);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = artifact;
        self.prepared_bundle()?
            .required_archive_member(representation, member, limit)
    }

    /// Reuse the retained carrier and archive fold, or exact producer-selected text.
    /// A selected corpus cannot fall through to shape assembly on a miss.
    #[cfg(feature = "core")]
    pub(super) fn prepared_tier1_shapes(
        &self,
    ) -> gmeow_errors::Result<gmeow_validate::data_validate::Tier1Shapes> {
        #[cfg(not(target_arch = "wasm32"))]
        let selected = self
            .selected_artifact_sha256
            .as_deref()
            .map(|expected| selected_artifact(expected, "validate-production-shapes.ttl"))
            .transpose()?;
        #[cfg(target_arch = "wasm32")]
        let selected: Option<Arc<[u8]>> = None;
        let shapes_ttl = match selected {
            Some(bytes) => String::from_utf8(bytes.to_vec()).map_err(|error| {
                fail(format!("selected validation shapes are not UTF-8: {error}"))
            })?,
            None => {
                let blob = self
                    .prepared_bundle()?
                    .required_blob(gmeow_bundle_view::bundle_blobs::REP_SHAPES)?;
                gmeow_validate::data_validate::data_graph_shapes_from_archive(&blob)?
            }
        };
        gmeow_validate::data_validate::Tier1Shapes::from_shapes_and_ontology(
            &shapes_ttl,
            Arc::clone(&self.dataset),
        )
    }

    pub(super) fn action_policy(&self) -> gmeow_errors::Result<&PreparedActionPolicy> {
        if let Some(policy) = self.action_policy.get() {
            return Ok(policy);
        }
        let bytes = self.prepared_member(
            action_policy::CORPUS_ARTIFACT,
            REP_REASONING,
            action_policy::BUNDLE_MEMBER,
            action_policy::MAX_BYTES,
        )?;
        let policy = decode_policy(&bytes)?;
        Ok(self.action_policy.get_or_init(|| policy))
    }

    #[cfg(feature = "core")]
    pub(super) fn native_codebook(
        &self,
    ) -> gmeow_errors::Result<&gmeow_lang_bridge::gmn1_codec::native::NativeCodebook> {
        use gmeow_lang_bridge::gmn1_codec::native;
        if let Some(codebook) = self.gmn_dictionary.get() {
            return Ok(codebook);
        }
        let bytes = self.prepared_member(
            "gmn-codebook.cbor",
            "lang-projections-archive",
            native::GENERATED_PATH,
            native::MAX_NATIVE_CODEBOOK_BYTES,
        )?;
        let codebook = native::decode(&bytes, native::SOURCE_BLAKE3)?;
        Ok(self.gmn_dictionary.get_or_init(|| codebook))
    }

    #[cfg(any(not(target_arch = "wasm32"), feature = "reasoning"))]
    pub(super) fn prepared_reasoned_gates(&self) -> gmeow_errors::Result<&PreparedReasonedGates> {
        if let Some(gates) = self.prepared_gates.get() {
            return Ok(gates);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let selected = self
            .selected_artifact_sha256
            .as_deref()
            .map(|expected| selected_artifact(expected, "prepared-verify-gates.json"))
            .transpose()?;
        #[cfg(target_arch = "wasm32")]
        let selected: Option<Arc<[u8]>> = None;
        let bytes = match selected {
            Some(bytes) => bytes,
            None => self.prepared_bundle()?.prepared_reasoned_gates()?,
        };
        if bytes.len() > gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES {
            return Err(fail(
                "prepared MCP verification laws exceed their size bound",
            ));
        }
        let gates: PreparedReasonedGates = serde_json::from_slice(&bytes)
            .map_err(|error| fail(format!("decode prepared MCP verification laws: {error}")))?;
        gates.validate_source_identity()?;
        Ok(self.prepared_gates.get_or_init(|| gates))
    }
}

/// Read the prepared action authority from one captured snapshot, without importing
/// its ontology dataset. A runner-selected corpus must supply its exact compact artifact.
///
/// # Errors
/// Rejects missing/corrupt selection, malformed archive members and stale native policy.
pub fn prepared_action_policy(snapshot: &[u8]) -> gmeow_errors::Result<PreparedActionPolicy> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(expected) = crate::storage().env_var("GMEOW_BUNDLE_IMPORT_SOURCE_SHA256") {
        if crate::storage()
            .env_var("GMEOW_BUNDLE_IMPORT_CACHE")
            .is_none()
        {
            return Err(fail(
                "bundle native artifact selection requires its exact import cache",
            ));
        }
        if purrdf::ContentDigest::of(snapshot).to_hex() != expected {
            return Err(fail(
                "selected native action policy belongs to a different snapshot",
            ));
        }
        return decode_policy(&selected_artifact(
            &expected,
            action_policy::CORPUS_ARTIFACT,
        )?);
    }
    let bundle = Bundle::from_snapshot(snapshot)?;
    decode_policy(&bundle.required_archive_member(
        REP_REASONING,
        action_policy::BUNDLE_MEMBER,
        action_policy::MAX_BYTES,
    )?)
}
