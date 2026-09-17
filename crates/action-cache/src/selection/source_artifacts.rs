// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Small source-derived artifacts exported from authenticated producer-stage outputs.
//! Publication follows the producer's complete parent-product authentication. Consumers
//! authenticate only the selected export; no pipeline dependency or extraction fallback.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{SelectedAction, load_manifest};
use crate::{
    ActionCacheError, ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity,
    STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};

const CODEC: &str = "selected-source-artifact-v1";
/// Maximum payload of a compact source observation; larger outputs stay in their stage.
pub const MAX_SOURCE_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;

/// Exact origin selected by the producer after authenticating the complete parent output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceArtifactOrigin {
    /// Owning production stage.
    pub stage: String,
    /// Complete source-stage action identity.
    pub action_key: String,
    /// Digest of the caller-owned source-stage receipt.
    pub receipt_digest: String,
    /// Complete source-stage product identity.
    pub product_digest: String,
    /// Source producer's exact code, toolchain, target, profile and feature identity.
    pub implementation: ProducerIdentity,
}

/// One exported artifact's source commitment and immutable action selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedSourceArtifact {
    /// Original source-stage identity; independent of any derived distribution bundle.
    pub source: SourceArtifactOrigin,
    /// Exact logical artifact name from the authenticated source-stage receipt.
    pub artifact: String,
    /// SHA-256 from that artifact's source-stage commitment.
    pub digest: String,
    /// Exact committed artifact byte count.
    pub bytes: u64,
    /// Export action admitted before the consumer starts.
    pub action: SelectedAction,
}

/// Selector extension, keyed first by source stage and then by logical artifact name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceArtifactSelector {
    /// Mandatory membership for a selected source-artifact consumer.
    pub source_artifacts: BTreeMap<String, BTreeMap<String, SelectedSourceArtifact>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Payload {
    source: SourceArtifactOrigin,
    artifact: String,
    digest: String,
    bytes: u64,
}

impl Payload {
    fn validate(&self) -> Result<(), ActionCacheError> {
        let digest =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if self.source.stage.is_empty()
            || self.artifact.is_empty()
            || !digest(&self.source.action_key)
            || !digest(&self.source.receipt_digest)
            || !digest(&self.source.product_digest)
            || !digest(&self.digest)
            || self.source.implementation.digest.is_empty()
            || self.bytes > MAX_SOURCE_ARTIFACT_BYTES
        {
            return Err(ActionCacheError::message(
                "invalid selected source-artifact commitment",
            ));
        }
        Ok(())
    }

    fn context(&self) -> ActionContext {
        ActionContext::new(
            "source-artifact",
            &self.artifact,
            self.source.implementation.clone(),
            CODEC,
            vec![
                ActionInput::Upstream {
                    producer: self.source.stage.clone(),
                    entity: Some(self.artifact.clone()),
                    receipt_digest: Some(self.source.receipt_digest.clone()),
                    product_digest: self.source.product_digest.clone(),
                },
                ActionInput::Raw {
                    logical_path: self.artifact.clone(),
                    file_kind: FileKind::File,
                    executable: false,
                    digest: self.digest.clone(),
                },
            ],
        )
        .with_dimension("source-action", &self.source.action_key)
        .with_dimension("artifact-bytes", self.bytes.to_string())
    }
}

/// Export already-authenticated source-stage bytes through an admitted producer store.
///
/// The caller must first authenticate the entire selected parent stage and obtain the
/// artifact digest/count from that receipt. This function verifies those commitments;
/// it does not run or restore a pipeline stage. Never call it from corpus tests.
///
/// # Errors
/// Rejects mismatched source commitments, oversized artifacts, read-only stores or any
/// immutable-store publication failure.
pub fn publish(
    store: &ActionStore,
    source: SourceArtifactOrigin,
    artifact: &str,
    expected_digest: &str,
    expected_bytes: u64,
    bytes: &[u8],
) -> Result<SelectedSourceArtifact, ActionCacheError> {
    let payload = Payload {
        source,
        artifact: artifact.to_owned(),
        digest: expected_digest.to_owned(),
        bytes: expected_bytes,
    };
    payload.validate()?;
    if bytes.len() as u64 != payload.bytes || bytes_digest(bytes) != payload.digest {
        return Err(ActionCacheError::message(
            "source-artifact bytes differ from the authenticated parent commitment",
        ));
    }
    let receipt = store.publish(
        &payload.context(),
        payload.digest.clone(),
        payload.clone(),
        bytes,
    )?;
    Ok(SelectedSourceArtifact {
        source: payload.source,
        artifact: payload.artifact,
        digest: payload.digest,
        bytes: payload.bytes,
        action: SelectedAction::from_receipt(&receipt),
    })
}

/// Read one exact source artifact from the runner-authenticated selector.
///
/// The producer's identity determines the export key, even when the consumer has a
/// different Cargo profile. The small blob is authenticated without reading its large
/// parent product. Missing selectors, membership, receipts or blobs are terminal.
///
/// # Errors
/// Returns selector, source-identity, byte-commitment or immutable-store errors. There
/// is no extraction callback, source discovery, producer invocation or cache mutation.
pub fn load(root: &Path, stage: &str, artifact: &str) -> Result<Vec<u8>, ActionCacheError> {
    let selector: SourceArtifactSelector = load_manifest(root)?;
    load_selected(root, &selector, stage, artifact)
}

fn load_selected(
    root: &Path,
    selector: &SourceArtifactSelector,
    stage: &str,
    artifact: &str,
) -> Result<Vec<u8>, ActionCacheError> {
    let selected = selector
        .source_artifacts
        .get(stage)
        .and_then(|artifacts| artifacts.get(artifact))
        .ok_or_else(|| {
            ActionCacheError::message(format!(
                "source artifact {stage}/{artifact} is absent from the producer selector"
            ))
        })?;
    let payload = Payload {
        source: selected.source.clone(),
        artifact: selected.artifact.clone(),
        digest: selected.digest.clone(),
        bytes: selected.bytes,
    };
    payload.validate()?;
    if payload.source.stage != stage
        || payload.artifact != artifact
        || selected.action.context != payload.context()
        || selected.action.product_digest != payload.digest
    {
        return Err(ActionCacheError::message(
            "source-artifact selector identity differs from its source commitment",
        ));
    }
    let store = ActionStore::open_existing_read_only(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits {
            max_entry_bytes: MAX_SOURCE_ARTIFACT_BYTES,
            ..StoreLimits::default()
        },
    )?;
    let entry = store
        .get::<Payload>(&selected.action.context)?
        .ok_or_else(|| {
            ActionCacheError::message(
                "selected source-artifact action is missing; consumers cannot rebuild it",
            )
        })?;
    selected.action.verify(&entry.receipt)?;
    // ActionStore already authenticated the small blob's SHA-256 and byte count.
    // Compare that verified commitment instead of hashing the same bytes again.
    if entry.receipt.payload != payload
        || entry.receipt.product_blob.bytes != payload.bytes
        || entry.receipt.product_blob.digest != payload.digest
    {
        return Err(ActionCacheError::message(
            "source-artifact export differs from the selected parent commitment",
        ));
    }
    Ok(entry.bytes)
}

#[path = "source_artifacts.tests.rs"]
#[cfg(test)]
mod tests;
