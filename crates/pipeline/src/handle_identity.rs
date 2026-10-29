// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed payload commitments shared by live products, action keys and receipts.

use std::collections::BTreeMap;

use gmeow_action_cache::content_digest;
use purrdf::{PipelineBundle, PipelineBundleError, RdfTerm};

use crate::bundle::PipelineHandle;

/// An immutable commitment calculated once when a producer publishes its handle.
#[derive(Debug, Clone)]
pub(crate) struct HandleCommitment {
    /// The typed arm and backing graph, also used as the receipt row identity.
    pub(crate) identity: String,
    /// Both the graph pin and the full typed payload identity.
    pub(crate) digest: String,
}

/// Stable arm tags distinguish equal-looking payloads from different languages.
pub(crate) fn handle_arm_tag(handle: &PipelineHandle) -> &'static str {
    match handle {
        PipelineHandle::SourceCatalog(_) => "source-catalog",
        PipelineHandle::Logic(_) => "logic",
        PipelineHandle::CompiledLogic(_) => "compiled-logic",
        PipelineHandle::Diagnostics(_) => "diagnostics",
        PipelineHandle::Reasoning(_) => "reasoning",
        PipelineHandle::RelationalCore(_) => "relational-core",
        PipelineHandle::Correspondence(_) => "correspondence",
    }
}

/// Hash typed framing without allocating or retaining a serialized program.
#[derive(Default)]
struct TypedDigest(purrdf_hash::blake3::Hasher);

impl std::io::Write for TypedDigest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Hash every field using native typed framing, with no retained byte buffer.
pub(crate) fn typed_digest(value: &impl serde::Serialize) -> String {
    let mut sink = TypedDigest::default();
    ciborium::ser::into_writer(value, &mut sink)
        .expect("derived typed serialization into an infallible hash sink");
    purrdf_hash::hex::encode(sink.0.finalize().as_bytes())
}

/// Stream the complete relational payload through its owned native CBOR writer.
fn relational_core_digest(
    program: &gmeow_logic_compile::relational_core::RelationalCoreProgram,
) -> String {
    let mut sink = TypedDigest::default();
    program
        .write_cbor(&mut sink)
        .expect("native relational serialization into an infallible hash sink");
    purrdf_hash::hex::encode(sink.0.finalize().as_bytes())
}

/// Stream the complete original source-catalog tuple in recorded receipt order.
/// The version tag, optional base metadata and duplicate receipts are retained.
///
/// # Errors
/// Returns the sink's original failure without reporting partial success.
pub(crate) fn write_source_catalog_cbor<W: std::io::Write + ?Sized>(
    receipts: &[gmeow_logic_compile::frontend::SourceDocument],
    out: &mut W,
) -> std::io::Result<()> {
    use purrdf_lex::cbor::head;

    head::write_head(out, head::ARRAY, 2)?;
    head::write_text(out, "source-catalog-v1")?;
    head::write_head(out, head::ARRAY, receipts.len() as u64)?;
    for receipt in receipts {
        receipt.write_cbor(out)?;
    }
    Ok(())
}

/// Hash the complete original source-catalog tuple without retaining its bytes.
/// Receipt order and multiplicity remain part of the producer's source identity.
pub(crate) fn source_catalog_digest(
    receipts: &[gmeow_logic_compile::frontend::SourceDocument],
) -> String {
    let mut sink = TypedDigest::default();
    write_source_catalog_cbor(receipts, &mut sink)
        .expect("native source-catalog serialization into an infallible hash sink");
    purrdf_hash::hex::encode(sink.0.finalize().as_bytes())
}

/// Stream the complete correspondence payload through its owned native writer.
fn correspondence_digest(
    program: &gmeow_logic_compile::projections::correspondence::CorrespondenceProgram,
) -> String {
    let mut sink = TypedDigest::default();
    program
        .write_cbor(&mut sink)
        .expect("native correspondence serialization into an infallible hash sink");
    purrdf_hash::hex::encode(sink.0.finalize().as_bytes())
}

/// The complete payload identity is independent of its governed RDF projection.
pub(crate) fn handle_payload_digest(handle: &PipelineHandle) -> String {
    let key = match handle {
        PipelineHandle::SourceCatalog(catalog) => catalog.identity().to_owned(),
        PipelineHandle::Logic(program) => typed_digest(program.as_ref()),
        PipelineHandle::CompiledLogic(publication) => typed_digest(publication.as_ref()),
        PipelineHandle::Diagnostics(publication) => typed_digest(publication.as_ref()),
        PipelineHandle::Reasoning(result) => typed_digest(result.as_ref()),
        PipelineHandle::RelationalCore(program) => relational_core_digest(program.as_ref()),
        PipelineHandle::Correspondence(program) => correspondence_digest(program.as_ref()),
    };
    content_digest(&[
        b"typed-payload-v2",
        handle_arm_tag(handle).as_bytes(),
        key.as_bytes(),
    ])
}

/// Capture published bindings once. Scheduler keys reuse these compact records;
/// they never lower, project or serialize the typed payload again.
pub(crate) fn handle_commitments(
    bundle: &PipelineBundle<PipelineHandle>,
) -> BTreeMap<String, HandleCommitment> {
    bundle
        .handles()
        .iter()
        .map(|(graph, entry)| {
            (
                graph.clone(),
                handle_commitment(graph, &entry.content_digest.to_hex(), &entry.payload),
            )
        })
        .collect()
}

/// The single identity recipe for publishing or authenticating a selected handle.
/// Selection does not hash other handles or materialize their RDF projections.
pub(crate) fn handle_commitment(
    graph: &str,
    graph_pin: &str,
    handle: &PipelineHandle,
) -> HandleCommitment {
    let payload = handle_payload_digest(handle);
    HandleCommitment {
        identity: format!("{}#{graph}", handle_arm_tag(handle)),
        digest: content_digest(&[graph_pin.as_bytes(), payload.as_bytes()]),
    }
}

/// The whole product includes typed bindings even when their graph is lossy.
///
/// # Errors
/// Returns the carrier's canonicalization refusal; a product without a content
/// identity has no digest to publish.
pub(crate) fn product_digest(
    bundle: &PipelineBundle<PipelineHandle>,
    handles: &BTreeMap<String, HandleCommitment>,
) -> Result<String, PipelineBundleError> {
    let base = bundle.digest()?.to_hex();
    if handles.is_empty() {
        return Ok(base);
    }
    let mut fields = vec![b"typed-product-v1".as_slice(), base.as_bytes()];
    for binding in handles.values() {
        fields.extend([binding.identity.as_bytes(), binding.digest.as_bytes()]);
    }
    Ok(content_digest(&fields))
}

/// A selected graph includes its typed payload binding when one is declared.
/// The exact same fold is used with live and receipt-backed inputs.
pub(crate) fn entity_digest<'a>(
    graph_digest: &str,
    bindings: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    let bindings: BTreeMap<_, _> = bindings.into_iter().collect();
    if bindings.is_empty() {
        return graph_digest.to_owned();
    }
    let mut fields = vec![b"typed-entity-v1".as_slice(), graph_digest.as_bytes()];
    for (identity, digest) in bindings {
        fields.extend([identity.as_bytes(), digest.as_bytes()]);
    }
    content_digest(&fields)
}

/// Whether a live bundle actually carries the selected named graph.
pub(crate) fn contains_graph(bundle: &PipelineBundle<PipelineHandle>, graph: &str) -> bool {
    bundle
        .dataset()
        .owned_named_graphs()
        .any(|term| matches!(term, RdfTerm::Iri(iri) if iri == graph))
}

#[cfg(test)]
#[path = "handle_identity.tests.rs"]
mod tests;
