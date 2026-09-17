// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Output-bound evidence of native snapshot ingestion, outside the GTS payload.

use std::path::{Path, PathBuf};

use ciborium::value::Value;
use purrdf::gts_compose::IngestReport;

use crate::{GmeowGtsEmission, profile_error};

const SCHEMA: &str = "gmeow-gts-ingestion-v1";
const MAX_RECEIPT_DEPTH: usize = 64;

/// An original receipt admitted against the actual bytes selected as an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmeowGtsSourceReceipt {
    encoded: Vec<u8>,
}

impl GmeowGtsSourceReceipt {
    /// Bind complete prior evidence to the actual input before re-emission.
    ///
    /// # Errors
    /// Refuses a malformed receipt or any input/output identity mismatch.
    pub fn admit(input: &[u8], receipt: &[u8]) -> gmeow_errors::Result<Self> {
        read_ingestion_receipt(input, receipt)?;
        Ok(Self {
            encoded: receipt.to_vec(),
        })
    }

    /// Borrow the original complete receipt bytes without reconstructing evidence.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Recheck this admitted receipt against the selected re-emission input.
    ///
    /// # Errors
    /// Refuses a source receipt belonging to different bytes.
    pub fn validate_input(&self, input: &[u8]) -> gmeow_errors::Result<()> {
        read_ingestion_receipt(input, &self.encoded).map(|_| ())
    }
}

/// Complete decoded evidence: exact current native report and inherited chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmeowGtsReceipt {
    /// Native observations for this emission only; never fabricated cumulative counts.
    pub ingestion: IngestReport,
    /// Prior receipts individually bound to their original source outputs.
    pub source_receipts: Vec<GmeowGtsSourceReceipt>,
}

/// Encode the complete native ingestion report, bound to the exact output bytes.
///
/// The omission inventory is retained verbatim, including its native order and
/// scoped graph spellings. This receipt is operational/loss evidence, never an
/// additional RDF assertion or a change to the frozen GTS tables.
///
/// # Errors
/// Refuses a count outside the receipt's unsigned 64-bit range or an encoding failure.
pub fn ingestion_receipt(bytes: &[u8], report: &IngestReport) -> gmeow_errors::Result<Vec<u8>> {
    encode_receipt(bytes, report, &[])
}

fn encode_receipt(
    bytes: &[u8],
    report: &IngestReport,
    sources: &[GmeowGtsSourceReceipt],
) -> gmeow_errors::Result<Vec<u8>> {
    let count = |value: usize| {
        u64::try_from(value)
            .map(|value| Value::Integer(value.into()))
            .map_err(|_| profile_error("native ingestion count exceeds the receipt range"))
    };
    let record = Value::Array(vec![
        Value::Text(SCHEMA.to_owned()),
        Value::Text(purrdf::gts::writer::digest_string(bytes)),
        count(report.rows_consumed)?,
        count(report.terms_interned)?,
        count(report.scratch_bytes)?,
        Value::Array(
            report
                .declarations_omitted
                .iter()
                .cloned()
                .map(Value::Text)
                .collect(),
        ),
        Value::Array(
            sources
                .iter()
                .map(|source| Value::Bytes(source.encoded.clone()))
                .collect(),
        ),
    ]);
    let mut encoded = Vec::new();
    ciborium::ser::into_writer(&record, &mut encoded)
        .map_err(|e| profile_error(format!("encode native ingestion receipt: {e}")))?;
    // Bound the complete inherited decoding workload before publishing a receipt
    // that a consumer could not admit. This reads only compact receipt metadata.
    let mut budget = crate::archive::MAX_SELECTED_ARCHIVE_BYTES;
    decode_receipt(&encoded, None, 0, &mut budget)?;
    Ok(encoded)
}

/// Authenticate an output-bound receipt and recover every native report field.
///
/// # Errors
/// Rejects malformed/trailing CBOR, another schema, a mismatched output digest,
/// out-of-range counts, or a non-text omission name. There is one receipt codec.
pub fn read_ingestion_receipt(
    bytes: &[u8],
    receipt: &[u8],
) -> gmeow_errors::Result<GmeowGtsReceipt> {
    let mut budget = crate::archive::MAX_SELECTED_ARCHIVE_BYTES;
    decode_receipt(
        receipt,
        Some(&purrdf::gts::writer::digest_string(bytes)),
        0,
        &mut budget,
    )
}

fn decode_receipt(
    receipt: &[u8],
    expected_digest: Option<&str>,
    depth: usize,
    budget: &mut usize,
) -> gmeow_errors::Result<GmeowGtsReceipt> {
    *budget = budget.checked_sub(receipt.len()).ok_or_else(|| {
        profile_error("native ingestion receipt exceeds the selected aggregate decode byte bound")
    })?;
    if depth >= MAX_RECEIPT_DEPTH {
        return Err(profile_error(
            "native ingestion receipt exceeds the selected chain-depth bound",
        ));
    }
    let mut cursor = std::io::Cursor::new(receipt);
    let value: Value = ciborium::de::from_reader(&mut cursor)
        .map_err(|e| profile_error(format!("decode native ingestion receipt: {e}")))?;
    if cursor.position() != receipt.len() as u64 {
        return Err(profile_error("native ingestion receipt has trailing bytes"));
    }
    let Value::Array(fields) = value else {
        return Err(profile_error(
            "native ingestion receipt is not a fixed record",
        ));
    };
    let [
        Value::Text(schema),
        Value::Text(digest),
        rows,
        terms,
        scratch,
        Value::Array(omissions),
        Value::Array(sources),
    ] = fields.as_slice()
    else {
        return Err(profile_error(
            "native ingestion receipt has an invalid field set",
        ));
    };
    let canonical_digest = digest.strip_prefix("blake3:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    });
    if schema != SCHEMA
        || !canonical_digest
        || expected_digest.is_some_and(|expected| digest != expected)
    {
        return Err(profile_error(
            "native ingestion receipt schema or output identity mismatch",
        ));
    }
    let count = |value: &Value| match value {
        Value::Integer(value) => usize::try_from(i128::from(*value))
            .map_err(|_| profile_error("native ingestion receipt count is out of range")),
        _ => Err(profile_error(
            "native ingestion receipt count is not an integer",
        )),
    };
    let declarations_omitted = omissions
        .iter()
        .map(|value| match value {
            Value::Text(name) => Ok(name.clone()),
            _ => Err(profile_error(
                "native ingestion receipt graph name is not text",
            )),
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let source_receipts = sources
        .iter()
        .map(|source| {
            let Value::Bytes(encoded) = source else {
                return Err(profile_error(
                    "inherited ingestion receipt is not encoded bytes",
                ));
            };
            decode_receipt(encoded, None, depth + 1, budget)?;
            Ok(GmeowGtsSourceReceipt {
                encoded: encoded.clone(),
            })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    Ok(GmeowGtsReceipt {
        ingestion: IngestReport {
            rows_consumed: count(rows)?,
            terms_interned: count(terms)?,
            scratch_bytes: count(scratch)?,
            declarations_omitted,
        },
        source_receipts,
    })
}

/// Companion path for the complete ingestion receipt of an emitted file.
#[must_use]
pub fn ingestion_receipt_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".ingestion.cbor");
    PathBuf::from(name)
}

/// Persist complete ingestion evidence beside the selected GTS output.
///
/// # Errors
/// Refuses receipt encoding or file-write failures; it never drops omission names.
pub fn write_ingestion_receipt(
    output: &Path,
    bytes: &[u8],
    report: &IngestReport,
) -> gmeow_errors::Result<PathBuf> {
    let receipt = ingestion_receipt(bytes, report)?;
    let path = ingestion_receipt_path(output);
    std::fs::write(&path, receipt)
        .map_err(|e| profile_error(format!("write {}: {e}", path.display())))?;
    Ok(path)
}

impl GmeowGtsEmission {
    /// Encode this output's complete native receipt without changing its payload.
    ///
    /// # Errors
    /// Propagates receipt encoding failures.
    pub fn ingestion_receipt(&self) -> gmeow_errors::Result<Vec<u8>> {
        encode_receipt(&self.bytes, &self.ingestion, &self.source_receipts)
    }

    /// Write an already-emitted output and its mandatory companion receipt.
    ///
    /// The receipt is written before publishing the bytes. A partial I/O failure
    /// is an error; the receipt's output digest makes mismatched pairs fail closed.
    /// This is file publication, not a second snapshot authorship path.
    ///
    /// # Errors
    /// Propagates receipt encoding and either file-write failure.
    pub fn write_to(&self, output: &Path) -> gmeow_errors::Result<PathBuf> {
        let receipt = ingestion_receipt_path(output);
        std::fs::write(&receipt, self.ingestion_receipt()?)
            .map_err(|e| profile_error(format!("write {}: {e}", receipt.display())))?;
        std::fs::write(output, &self.bytes)
            .map_err(|e| profile_error(format!("write {}: {e}", output.display())))?;
        Ok(receipt)
    }
}
