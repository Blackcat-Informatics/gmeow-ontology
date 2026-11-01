// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lossless terminal carriage of the native execution record. Live consumers
//! borrow the typed result; this codec runs only at an RDF publication boundary.

use std::io::Write;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{
    ExecutionCbor, MAX_RECEIPT_BYTES, MAX_RECEIPT_DEPTH, Reader, check_structure, descend,
};
use crate::result::NativeExecutionEvidence;

use super::result_err;

const SCHEMA: &str = "gmeow-native-execution-v2";

/// Preserve the typed projection kind as the direct diagnostic source and retain
/// the native refusal beneath it, rather than replacing that kind's downcast.
fn codec_err(
    message: String,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(message, cause))
}

/// One complete envelope writer owns output, replay and encoded-size accounting.
fn write_envelope<W: Write + ?Sized>(
    execution: &NativeExecutionEvidence,
    output: &mut W,
) -> std::io::Result<()> {
    head::write_head(output, head::ARRAY, 2)?;
    head::write_text(output, SCHEMA)?;
    ExecutionCbor::write_cbor(execution, output)
}

/// Preserve native terms, proof DAGs and finite u128 cardinalities at the actual
/// RDF boundary. Semantic ownership and source admission remain with the caller.
pub(super) fn encode(execution: &NativeExecutionEvidence) -> gmeow_errors::Result<String> {
    // The original directional-term boundary could pass structural validation
    // while its typed enum visitor exceeded the reader's budget. Check borrowed
    // records before recursive emission without allocating a second execution record.
    descend(MAX_RECEIPT_DEPTH)
        .and_then(|remaining| execution.check_depth(remaining))
        .map_err(|error| {
            codec_err(
                format!("native execution receipt exceeds its typed admission bound: {error}"),
                error,
            )
        })?;
    let mut output = ReceiptWriter(Vec::new());
    write_envelope(execution, &mut output).map_err(|error| {
        codec_err(
            format!(
                "native execution receipt encoding failed: {error}; {}",
                receipt_sizes(execution)
            ),
            error,
        )
    })?;
    // This is the old encoder's structural budget, independently of the typed
    // decoder's visitor budget. Walk borrowed bytes without a second value tree.
    check_structure(&output.0).map_err(|error| {
        codec_err(
            format!("native execution receipt exceeds its structural bound: {error}"),
            error,
        )
    })?;
    Ok(purrdf_hash::hex::encode(&output.0))
}

/// Admit the selected schema and exact original emitted wire. The enclosing
/// result admission remains responsible for semantic scope and source authority.
pub(super) fn decode(text: &str) -> gmeow_errors::Result<Box<NativeExecutionEvidence>> {
    if text.len() > MAX_RECEIPT_BYTES * 2 || !text.len().is_multiple_of(2) {
        return Err(result_err(
            "native execution receipt has an invalid byte length".into(),
        ));
    }
    let bytes = purrdf_hash::hex::decode_canonical(text).map_err(|_| {
        result_err("native execution receipt requires canonical lowercase hex".into())
    })?;
    let mut input = Reader::new(&bytes).map_err(|error| {
        codec_err(
            format!("native execution receipt decoding failed: {error}"),
            error,
        )
    })?;
    let (schema, execution) = <(String, Box<NativeExecutionEvidence>)>::read_cbor(&mut input)
        .map_err(|error| {
            codec_err(
                format!("native execution receipt decoding failed: {error}"),
                error,
            )
        })?;
    if schema != SCHEMA {
        return Err(result_err(
            "native execution receipt has the wrong schema or trailing bytes".into(),
        ));
    }
    input.finish().map_err(|error| {
        codec_err(
            "native execution receipt has the wrong schema or trailing bytes".into(),
            error,
        )
    })?;
    let mut canonical = ReceiptVerifier {
        expected: &bytes,
        consumed: 0,
    };
    write_envelope(&execution, &mut canonical).map_err(|error| {
        codec_err(
            format!("noncanonical native execution receipt: {error}"),
            error,
        )
    })?;
    if canonical.consumed != bytes.len() {
        return Err(result_err(
            "native execution receipt contains unconsumed fields".into(),
        ));
    }
    Ok(execution)
}

/// The encoded size of each receipt field, so an oversized receipt names its cause.
fn receipt_sizes(execution: &NativeExecutionEvidence) -> String {
    fn size(value: &impl ExecutionCbor) -> String {
        size_all(std::iter::once(value))
    }
    fn size_all<'a, T: ExecutionCbor + 'a>(values: impl Iterator<Item = &'a T>) -> String {
        let mut counter = ByteCounter(0);
        for value in values {
            if let Err(error) = value.write_cbor(&mut counter) {
                return format!("unencodable ({error})");
            }
        }
        format!("{} B", counter.0)
    }
    format!(
        "families {} ({} ledger(s), {} outcome(s), {} proof node(s), {} contextual receipt(s), \
         {} producer obstruction(s); \
         outcomes {}, proofs {} (statements {}, origins {}), \
         contextual_receipts {}, producer_obstructions {}), class_admission {}, source_coverage {}, \
         classes {} ({}), chase_certificates {}, witness_derivations {} ({}), frontier {}",
        size(&execution.families),
        execution.families.len(),
        execution
            .families
            .iter()
            .map(|ledger| ledger.outcomes.len())
            .sum::<usize>(),
        execution
            .families
            .iter()
            .map(|ledger| ledger.proofs.len())
            .sum::<usize>(),
        execution
            .families
            .iter()
            .map(|ledger| ledger.contextual_receipts.len())
            .sum::<usize>(),
        execution
            .families
            .iter()
            .map(|ledger| ledger.producer_obstructions.len())
            .sum::<usize>(),
        size_all(execution.families.iter().map(|ledger| &ledger.outcomes)),
        size_all(execution.families.iter().map(|ledger| &ledger.proofs)),
        size_all(
            execution
                .families
                .iter()
                .flat_map(|ledger| ledger.proofs.iter().map(|proof| &proof.statement)),
        ),
        size_all(
            execution
                .families
                .iter()
                .flat_map(|ledger| ledger.proofs.iter().map(|proof| &proof.origin)),
        ),
        size_all(
            execution
                .families
                .iter()
                .map(|ledger| &ledger.contextual_receipts),
        ),
        size_all(
            execution
                .families
                .iter()
                .map(|ledger| &ledger.producer_obstructions),
        ),
        size(&execution.class_admission),
        size(&execution.source_coverage),
        size(&execution.classes),
        execution.classes.len(),
        size(&execution.chase_certificates),
        size(&execution.witness_derivations),
        execution.witness_derivations.len(),
        size(&execution.frontier),
    )
}

struct ByteCounter(usize);

impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("native receipt byte accounting overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ReceiptVerifier<'a> {
    expected: &'a [u8],
    consumed: usize,
}

impl Write for ReceiptVerifier<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .consumed
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("native receipt length overflow"))?;
        if self.expected.get(self.consumed..end) != Some(bytes) {
            return Err(std::io::Error::other(
                "native receipt differs from its selected schema",
            ));
        }
        self.consumed = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ReceiptWriter(Vec<u8>);

impl Write for ReceiptWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_RECEIPT_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "native execution receipt exceeds its publication limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
