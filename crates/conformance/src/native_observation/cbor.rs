// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Streaming wire for a source digest and its retained admission observation.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use super::SourceAdmissionObservation;

impl SourceAdmissionObservation {
    /// Stream the persisted observation without cloning its complete evidence.
    /// The fixed digest remains an array of integers, never a byte string.
    ///
    /// # Errors
    /// Returns sink failures or an unsupported source-admission boundary.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "input_blake3")?;
        head::write_head(out, head::ARRAY, self.input_blake3.len() as u64)?;
        for byte in &self.input_blake3 {
            head::write_head(out, head::UNSIGNED, u64::from(*byte))?;
        }
        head::write_text(out, "admission")?;
        self.admission.write_cbor(out)
    }
}
