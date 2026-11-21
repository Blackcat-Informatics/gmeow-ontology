// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded access to required native artifacts in an already imported GMEOW bundle.

use purrdf::gts::model::{BlobEntry, Graph};
use purrdf_lex::cbor::Value;
use std::borrow::Cow;
use std::sync::Arc;

/// Archive representation containing native reasoning reports.
pub const REASONING_REP: &str = "reasoning-archive";
/// Archive representation containing the prepared native reasoning inputs.
pub const PREPARED_REASONING_REP: &str = "prepared-reasoning-archive";
/// Standalone JSON representation of the complete canon proof admission.
pub const CANON_ADMISSION_REP: &str = "canon-proof-admission";
/// Native verification laws produced with the matching reasoning result.
pub const REASONED_GATES_MEMBER: &str = "reason/prepared-verify-gates.json";
/// Aggregate decoded archive budget for a selected native bundle view.
pub const MAX_SELECTED_ARCHIVE_BYTES: usize = 256 * 1024 * 1024;
/// Hard upper bound for a compact native law or policy member.
pub const MAX_NATIVE_MEMBER_BYTES: usize = 16 * 1024 * 1024;

/// Select one required member, authenticating its containing archive blob.
///
/// Borrows decoded archive bytes and scans member headers without copying unrelated
/// bodies. The returned allocation contains only the bounded selected payload.
/// The caller owns typed decoding and keeps its cache scoped to this exact graph.
/// Archive traversal uses PurRDF's regular-member iterator and propagates its
/// errors, including errors encountered after the selected member. It does not
/// add tar-format validation beyond that upstream reader's contract.
///
/// # Errors
/// Rejects graph diagnostics, ambiguous or missing representations, corrupt blob
/// digests, upstream archive-reader errors, duplicate occurrences of the selected
/// member and oversized selected payloads.
pub fn required_member(
    graph: &Graph,
    representation: &str,
    member_path: &str,
    max_bytes: usize,
) -> gmeow_errors::Result<Arc<[u8]>> {
    let bytes = required_blob(graph, representation)?;
    archive_member(&bytes, member_path, max_bytes).map(Arc::from)
}

/// Authenticate the unique required representation in a retained bundle graph.
/// Uses the upstream decoder; the caller owns any retained decoded-byte budget.
pub fn required_blob<'a>(
    graph: &'a Graph,
    representation: &str,
) -> gmeow_errors::Result<Cow<'a, [u8]>> {
    required_blob_bounded(graph, representation, usize::MAX)
}

/// Authenticate a unique representation and enforce the caller's decoded byte bound.
///
/// # Errors
/// Rejects reader diagnostics, missing or ambiguous representations, oversized
/// payloads, codec failures and content digest mismatches.
pub fn required_blob_bounded<'a>(
    graph: &'a Graph,
    representation: &str,
    max_bytes: usize,
) -> gmeow_errors::Result<Cow<'a, [u8]>> {
    let fail = |message| {
        gmeow_errors::Diag::of_kind(crate::error::Archive {
            message,
            cause: None,
        })
    };
    if !graph.diagnostics.is_empty() {
        return Err(fail(format!(
            "snapshot has reader diagnostics: {:?}",
            graph.diagnostics
        )));
    }
    let mut archives = graph.blob_meta.iter().filter(|(_, meta)| {
        let Value::Map(fields) = meta else {
            return false;
        };
        fields.iter().any(|(key, value)| {
            key.as_text() == Some("rep") && value.as_text() == Some(representation)
        })
    });
    let (digest, _) = archives
        .next()
        .ok_or_else(|| fail(format!("required archive {representation} is missing")))?;
    if archives.next().is_some() {
        return Err(fail(format!("archive {representation} is ambiguous")));
    }
    let mut blobs = graph
        .blobs
        .iter()
        .filter(|(identity, _)| identity == digest);
    let (_, entry) = blobs
        .next()
        .ok_or_else(|| fail(format!("archive blob {digest} is missing")))?;
    if blobs.next().is_some() {
        return Err(fail(format!("archive blob {digest} is ambiguous")));
    }
    let bytes = match entry {
        BlobEntry::Bytes(bytes) if bytes.len() <= max_bytes => Cow::Borrowed(bytes.as_slice()),
        BlobEntry::Bytes(_) => {
            return Err(fail(format!(
                "blob {digest} exceeds {max_bytes} decoded bytes"
            )));
        }
        BlobEntry::Lazy { raw, chain } => Cow::Owned(
            purrdf::gts::codec::decode_chain_bounded(chain, raw, max_bytes).map_err(|cause| {
                gmeow_errors::Diag::of_kind(crate::error::Archive {
                    message: format!("decode blob {digest}: {cause}"),
                    cause: Some(Arc::new(cause)),
                })
            })?,
        ),
    };
    let actual = purrdf::gts::wire::digest_str(&bytes);
    if &actual != digest {
        return Err(fail(format!(
            "archive digest {actual} differs from {digest}"
        )));
    }
    Ok(bytes)
}

/// Select a bounded member from already authenticated, decoded archive bytes.
/// All errors from PurRDF's regular-member iterator propagate, including after
/// the selected member. The returned payload borrows the caller's archive.
pub fn archive_member<'a>(
    bytes: &'a [u8],
    member_path: &str,
    max_bytes: usize,
) -> gmeow_errors::Result<&'a [u8]> {
    let fail = |message| {
        gmeow_errors::Diag::of_kind(crate::error::Archive {
            message,
            cause: None,
        })
    };
    let mut selected = None;
    for member in purrdf::ustar::archive_members(bytes) {
        let member = member.map_err(|cause| {
            gmeow_errors::Diag::of_kind(crate::error::NativeArchive {
                cause: Arc::new(cause),
            })
        })?;
        if member.name == member_path {
            if selected.is_some() {
                return Err(fail(format!(
                    "archive repeats required member {member_path}"
                )));
            }
            if member.data.len() > max_bytes {
                return Err(fail(format!(
                    "member {member_path} exceeds {max_bytes} bytes"
                )));
            }
            selected = Some(member.data);
        }
    }
    selected.ok_or_else(|| fail(format!("archive omits required member {member_path}")))
}

/// Borrow one required representation retained by the authoritative native importer.
/// Payload and metadata provenance stay on the upstream value. The importer already
/// checked the selected digest, decoder bounds and final representation identity.
/// This lookup neither copies the payload nor repeats that format conformance work.
///
/// # Errors
/// Rejects an absent or ambiguous representation in the supplied selection.
pub fn required_imported_blob<'a>(
    imported: &'a purrdf::GtsImportWithBlobs,
    representation: &str,
) -> gmeow_errors::Result<&'a purrdf::GtsImportedBlob> {
    let mut selected = imported.blobs.iter().filter(|blob| {
        blob.metadata
            .as_ref()
            .and_then(Value::as_map)
            .is_some_and(|fields| {
                fields.iter().any(|(key, value)| {
                    key.as_text() == Some("rep") && value.as_text() == Some(representation)
                })
            })
    });
    let fail = |message| {
        gmeow_errors::Diag::of_kind(crate::error::Archive {
            message,
            cause: None,
        })
    };
    let blob = selected.next().ok_or_else(|| {
        fail(format!(
            "required imported archive {representation} is missing"
        ))
    })?;
    if selected.next().is_some() {
        return Err(fail(format!(
            "imported archive {representation} is ambiguous"
        )));
    }
    Ok(blob)
}
