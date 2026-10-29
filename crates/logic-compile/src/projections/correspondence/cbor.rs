// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete borrowed CBOR of the correspondence handle and its source declarations.

use std::io::{self, Write};

use purrdf_lex::cbor::{self, Value, head};

use super::{CorrespondenceProgram, NativeCorrespondenceSource};

impl CorrespondenceProgram {
    /// Stream every correspondence-program field without sorting or cloning its payload.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "correspondences")?;
        head::write_head(out, head::ARRAY, self.correspondences.len() as u64)?;
        for value in &self.correspondences {
            value.write_cbor(out)?;
        }
        head::write_text(out, "compositions")?;
        head::write_head(out, head::ARRAY, self.compositions.len() as u64)?;
        for value in &self.compositions {
            value.write_cbor(out)?;
        }
        head::write_text(out, "preservation")?;
        self.preservation.write_cbor(out)?;
        head::write_text(out, "leg_programs")?;
        head::write_head(out, head::ARRAY, self.leg_programs.len() as u64)?;
        for value in &self.leg_programs {
            value.write_cbor(out)?;
        }
        head::write_text(out, "native_sources")?;
        head::write_head(out, head::ARRAY, self.native_sources.len() as u64)?;
        for value in &self.native_sources {
            value.write_cbor(out)?;
        }
        head::write_text(out, "projection_profiles")?;
        head::write_head(out, head::ARRAY, self.projection_profiles.len() as u64)?;
        for value in &self.projection_profiles {
            value.write_cbor(out)?;
        }
        head::write_text(out, "projection_bindings")?;
        head::write_head(out, head::ARRAY, self.projection_bindings.len() as u64)?;
        for value in &self.projection_bindings {
            value.write_cbor(out)?;
        }
        Ok(())
    }
}

impl NativeCorrespondenceSource {
    /// Retain the original named source declaration and optional physical ownership.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 5)?;
        head::write_text(out, "alias")?;
        head::write_text(out, &self.alias)?;
        head::write_text(out, "correspondence")?;
        head::write_text(out, &self.correspondence)?;
        head::write_text(out, "predicate")?;
        head::write_text(out, &self.predicate)?;
        head::write_text(out, "sssom_file")?;
        head::write_text(out, &self.sssom_file)?;
        head::write_text(out, "ownership")?;
        match &self.ownership {
            Some(value) => value.write_cbor(out)?,
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}
