// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lossless terminal carriage of the native execution record. Live consumers
//! borrow the typed result; this codec runs only at an RDF publication boundary.

use std::io::{Cursor, Write};

use crate::result::NativeExecutionEvidence;

use super::result_err;

const SCHEMA: &str = "gmeow-native-execution-v1";
const MAX_RECEIPT_BYTES: usize = 256 * 1024 * 1024;
const MAX_RECEIPT_DEPTH: usize = 64;

/// CBOR preserves native terms, proof DAGs and finite u128 cardinalities. The
/// enclosing content-addressed result binds these bytes; the receipt itself
/// does not confer authorship or authorize an optimization rewrite.
pub(super) fn encode(execution: &NativeExecutionEvidence) -> gmeow_errors::Result<String> {
    let mut output = ReceiptWriter(Vec::new());
    ciborium::ser::into_writer(&(SCHEMA, execution), &mut output).map_err(|error| {
        result_err(format!(
            "native execution receipt encoding failed: {error}; {}",
            receipt_sizes(execution)
        ))
    })?;
    // Bound the emitted envelope with the same decoder stack limit before it
    // becomes a published artifact. IgnoredAny walks structure without building
    // a second execution record; deep typed terms cannot mint unreadable output.
    let _: serde::de::IgnoredAny =
        ciborium::de::from_reader_with_recursion_limit(output.0.as_slice(), MAX_RECEIPT_DEPTH)
            .map_err(|error| {
                result_err(format!(
                    "native execution receipt exceeds its structural bound: {error}"
                ))
            })?;
    Ok(purrdf_hash::hex::encode(&output.0))
}

/// Decode only the selected schema and one complete receipt. Semantic proof,
/// scope and budget validation belongs to the enclosing result admission.
pub(super) fn decode(text: &str) -> gmeow_errors::Result<Box<NativeExecutionEvidence>> {
    if text.len() > MAX_RECEIPT_BYTES * 2 || !text.len().is_multiple_of(2) {
        return Err(result_err(
            "native execution receipt has an invalid byte length".into(),
        ));
    }
    let bytes = purrdf_hash::hex::decode_canonical(text).map_err(|_| {
        result_err("native execution receipt requires canonical lowercase hex".into())
    })?;
    let mut cursor = Cursor::new(bytes.as_slice());
    let (schema, execution): (String, Box<NativeExecutionEvidence>) =
        ciborium::de::from_reader_with_recursion_limit(&mut cursor, MAX_RECEIPT_DEPTH).map_err(
            |error| result_err(format!("native execution receipt decoding failed: {error}")),
        )?;
    if schema != SCHEMA || cursor.position() != bytes.len() as u64 {
        return Err(result_err(
            "native execution receipt has the wrong schema or trailing bytes".into(),
        ));
    }
    // Refuse unknown fields and alternate encodings that a permissive serde
    // decoder might otherwise discard. Compare the selected codec's emission
    // against the supplied bytes without allocating another receipt.
    let mut canonical = ReceiptVerifier {
        expected: &bytes,
        consumed: 0,
    };
    ciborium::ser::into_writer(&(SCHEMA, &execution), &mut canonical)
        .map_err(|error| result_err(format!("noncanonical native execution receipt: {error}")))?;
    if canonical.consumed != bytes.len() {
        return Err(result_err(
            "native execution receipt contains unconsumed fields".into(),
        ));
    }
    Ok(execution)
}

/// The encoded size of each receipt field, so an oversized receipt names its cause.
fn receipt_sizes(execution: &NativeExecutionEvidence) -> String {
    fn size(value: &impl serde::Serialize) -> String {
        let mut counter = ByteCounter(0);
        match ciborium::ser::into_writer(value, &mut counter) {
            Ok(()) => format!("{} B", counter.0),
            Err(error) => format!("unencodable ({error})"),
        }
    }
    format!(
        "families {} ({} ledger(s), {} outcome(s)), class_admission {}, source_coverage {}, \
         classes {} ({}), chase_certificates {}, witness_derivations {} ({}), frontier {}",
        size(&execution.families),
        execution.families.len(),
        execution
            .families
            .iter()
            .map(|ledger| ledger.outcomes.len())
            .sum::<usize>(),
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
        self.0 += bytes.len();
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
