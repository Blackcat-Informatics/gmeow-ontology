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
const HEX: &[u8; 16] = b"0123456789abcdef";

/// CBOR preserves native terms, proof DAGs and finite u128 cardinalities. The
/// enclosing content-addressed result binds these bytes; the receipt itself
/// does not confer authorship or authorize an optimization rewrite.
pub(super) fn encode(execution: &NativeExecutionEvidence) -> gmeow_errors::Result<String> {
    let mut output = ReceiptWriter(Vec::new());
    ciborium::ser::into_writer(&(SCHEMA, execution), &mut output).map_err(|error| {
        result_err(format!("native execution receipt encoding failed: {error}"))
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
    let mut text = String::with_capacity(output.0.len() * 2);
    for byte in output.0 {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Ok(text)
}

/// Decode only the selected schema and one complete receipt. Semantic proof,
/// scope and budget validation belongs to the enclosing result admission.
pub(super) fn decode(text: &str) -> gmeow_errors::Result<Box<NativeExecutionEvidence>> {
    if text.len() > MAX_RECEIPT_BYTES * 2 || !text.len().is_multiple_of(2) {
        return Err(result_err(
            "native execution receipt has an invalid byte length".into(),
        ));
    }
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(result_err(
            "native execution receipt requires canonical lowercase hex".into(),
        )),
    };
    let bytes = text
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((digit(pair[0])? << 4) | digit(pair[1])?))
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
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
