// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded opaque transport integrity, independent of execution semantics.

use std::io::Read;

use purrdf::gts::reader::{BlobPayload, BlobRefusal, StreamingSink};
use purrdf_lex::cbor::Value;

use crate::error;

/// Explicit decoded item ceiling of the selected opaque identity operation.
pub const MAX_OPAQUE_BYTES: usize = 1024 * 1024 * 1024;
/// Existing native public-metadata ceiling, applied to each opaque blob.
pub const MAX_METADATA_BYTES: usize = 64 * 1024;
/// The native rsyncable codec emits independent blocks of this size.
const RSYNCABLE_BLOCK_BYTES: usize = 65_536;

/// Native worst-case encoded size for the mandatory independent block codec.
/// This bounds encoded payload bytes, excluding their outer frame map.
#[must_use]
const fn rsyncable_encoded_bound(decoded: usize) -> usize {
    assert!(decoded <= MAX_OPAQUE_BYTES);
    let full = decoded / RSYNCABLE_BLOCK_BYTES;
    let tail = decoded % RSYNCABLE_BLOCK_BYTES;
    full * purrdf_zstd::encoding::compress_bound(RSYNCABLE_BLOCK_BYTES)
        + if tail == 0 {
            0
        } else {
            purrdf_zstd::encoding::compress_bound(tail)
        }
}

/// Encoded headroom for a full generic opaque item, separate from its raw limit.
pub const MAX_OPAQUE_ENCODED_BYTES: usize = rsyncable_encoded_bound(MAX_OPAQUE_BYTES);
/// Encoded headroom for the separately selected 256 MiB compact receipt profile.
pub const MAX_NATIVE_RECEIPT_ENCODED_BYTES: usize = rsyncable_encoded_bound(256 * 1024 * 1024);
const _: () = assert!(MAX_OPAQUE_ENCODED_BYTES == 1_078_460_416);
const _: () = assert!(MAX_NATIVE_RECEIPT_ENCODED_BYTES == 269_615_104);

/// Transport census only; this never asserts native receipt proof validity.
#[derive(Default)]
#[must_use]
pub struct OpaqueTransport {
    has_blobs: bool,
}

impl OpaqueTransport {
    /// Whether dataset-only re-emission would discard a blob capability.
    #[must_use]
    pub fn has_blobs(&self) -> bool {
        self.has_blobs
    }
}

fn refusal(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(error::Transport::new(message.into()))
}

#[derive(Default)]
struct Audit {
    census: OpaqueTransport,
    first_refusal: gmeow_errors::FirstDiagnostic,
}

impl Audit {
    fn metadata(metadata: Option<&Value>) -> gmeow_errors::Result<()> {
        if let Some(metadata) = metadata {
            let bytes = purrdf_lex::cbor::encoded_len(metadata);
            if bytes > MAX_METADATA_BYTES {
                return Err(refusal(format!(
                    "opaque public metadata has {bytes} bytes; selected limit is {MAX_METADATA_BYTES}"
                )));
            }
        }
        Ok(())
    }

    fn payload(mut payload: BlobPayload<'_>) -> gmeow_errors::Result<()> {
        Self::metadata(payload.metadata)?;
        if !payload.payload_present {
            return Ok(());
        }
        if payload.encoded_len > MAX_OPAQUE_ENCODED_BYTES {
            return Err(refusal(format!(
                "opaque payload {} has {} encoded bytes; selected limit is {MAX_OPAQUE_ENCODED_BYTES}",
                payload.digest, payload.encoded_len
            )));
        }
        let source = payload.bytes.reader().ok_or_else(|| {
            refusal(format!(
                "opaque payload {} has no readable bytes",
                payload.digest
            ))
        })?;
        let mut decoded =
            purrdf::gts::codec::decode_chain_reader(payload.codecs, source, MAX_OPAQUE_BYTES)
                .map_err(|cause| {
                    gmeow_errors::Diag::of_kind(error::Transport::caused_by(
                        format!(
                            "decode opaque payload {} with decoded limit {MAX_OPAQUE_BYTES}",
                            payload.digest
                        ),
                        cause,
                    ))
                })?;
        let mut hash = purrdf_hash::blake3::Hasher::new();
        let mut buffer = [0_u8; 65_536];
        let mut total = 0_usize;
        loop {
            let read = decoded.read(&mut buffer).map_err(|cause| {
                gmeow_errors::Diag::of_kind(error::Transport::caused_by(
                    format!(
                        "read complete opaque payload {} with decoded limit {MAX_OPAQUE_BYTES}",
                        payload.digest
                    ),
                    cause,
                ))
            })?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read)
                .ok_or_else(|| refusal("opaque decoded length overflow"))?;
            if total > MAX_OPAQUE_BYTES {
                return Err(refusal(format!(
                    "opaque payload {} exceeds selected decoded limit {MAX_OPAQUE_BYTES}",
                    payload.digest
                )));
            }
            hash.update(&buffer[..read]);
        }
        if purrdf::gts::wire::digest_label(hash.finalize().as_bytes()) != payload.digest {
            return Err(refusal(format!(
                "opaque payload bytes do not match {}",
                payload.digest
            )));
        }
        Ok(())
    }
}

impl StreamingSink for Audit {
    fn blob(&mut self, _segment: u64, _digest: &str, metadata: Option<&Value>) {
        self.census.has_blobs = true;
        self.first_refusal.record(Self::metadata(metadata));
    }

    fn blob_payload(&mut self, payload: BlobPayload<'_>) {
        // Never deduplicate before physical bytes have been decoded and hashed.
        // Even after refusal, the native reader must complete its integrity audit.
        let result = Self::payload(payload);
        self.first_refusal.record(result);
    }

    fn blob_refused(&mut self, refusal: BlobRefusal<'_>) {
        self.first_refusal.record(Err(self::refusal(format!("opaque payload {} refused under selected encoded/decoded limits {MAX_OPAQUE_ENCODED_BYTES}/{MAX_OPAQUE_BYTES}: {}", refusal.digest.unwrap_or("<undeclared>"), refusal.detail))));
    }

    fn blob_encoded_limit(&self) -> Option<usize> {
        Some(MAX_OPAQUE_ENCODED_BYTES)
    }
    fn blob_decode_limit(&self) -> Option<usize> {
        Some(MAX_OPAQUE_BYTES)
    }
    fn frame_decode_limit(&self) -> Option<usize> {
        Some(MAX_OPAQUE_BYTES)
    }
}

/// Authenticate every physical opaque payload and the complete native chain.
/// Decoded payloads are hashed and released one at a time, with no aggregate
/// history ceiling or retained decoded-payload collector. The captured input and
/// native RDF/frame bookkeeping are outside this payload-retention bound.
///
/// # Errors
/// Refuses corrupt or over-budget bytes, unavailable codecs, every native
/// diagnostic, torn input and missing/incomplete segment heads. This operation
/// does not assert the mandatory frame profile or native proof semantics.
pub fn audit_opaque_transport(bytes: &[u8]) -> gmeow_errors::Result<OpaqueTransport> {
    let mut audit = Audit::default();
    let result = purrdf::gts::reader::read_to_sink(bytes, true, None, &mut audit);
    if let Some(diagnostic) = result.diagnostics.into_iter().next() {
        return Err(gmeow_errors::Diag::of_kind(error::TransportDiagnostic {
            detail: format!("{}: {}", diagnostic.code, diagnostic.detail),
            diagnostic: std::sync::Arc::new(diagnostic),
        }));
    }
    if let Some(refusal) = audit.first_refusal.take() {
        return Err(refusal);
    }
    if result.torn.is_some() {
        return Err(refusal("opaque input ends with a torn frame"));
    }
    if result.segment_heads.is_empty()
        || result.segment_heads.len() != result.segment_profiles.len()
        || result.segment_heads.iter().any(Vec::is_empty)
    {
        return Err(refusal(
            "opaque input has no complete native segment-head evidence",
        ));
    }
    Ok(audit.census)
}
