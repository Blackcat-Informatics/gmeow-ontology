// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR of complete realized projection profiles, checks and bindings.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use super::{
    ProjectionBindingProgram, ProjectionInputCheck, ProjectionProfilePayload,
    ProjectionProfileProgram,
};

impl ProjectionBindingProgram {
    /// Retain the complete realized binding and original selected-template sequence.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 10)?;
        head::write_text(out, "id")?;
        head::write_text(out, &self.id)?;
        head::write_text(out, "correspondence")?;
        head::write_text(out, &self.correspondence)?;
        head::write_text(out, "binding_key")?;
        head::write_text(out, &self.binding_key)?;
        head::write_text(out, "source_mapping")?;
        head::write_text(out, &self.source_mapping)?;
        head::write_text(out, "profile")?;
        head::write_text(out, &self.profile)?;
        head::write_text(out, "order")?;
        head::write_head(out, head::UNSIGNED, self.order as u64)?;
        head::write_text(out, "source_pattern")?;
        head::write_text(out, &self.source_pattern)?;
        head::write_text(out, "template")?;
        head::write_text(out, &self.template)?;
        head::write_text(out, "selected")?;
        head::write_head(out, head::ARRAY, self.selected.len() as u64)?;
        for value in &self.selected {
            value.write_cbor(out)?;
        }
        head::write_text(out, "lossy_drops")?;
        head::write_head(out, head::ARRAY, self.lossy_drops.len() as u64)?;
        for value in &self.lossy_drops {
            head::write_text(out, value)?;
        }
        Ok(())
    }
}

impl ProjectionInputCheck {
    /// Retain the complete mandatory input check and its native selected support.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "id")?;
        head::write_text(out, &self.id)?;
        head::write_text(out, "binding_iri")?;
        head::write_text(out, &self.binding_iri)?;
        head::write_text(out, "binding_key")?;
        head::write_text(out, &self.binding_key)?;
        head::write_text(out, "binding_program")?;
        head::write_text(out, &self.binding_program)?;
        head::write_text(out, "source_mapping")?;
        head::write_text(out, &self.source_mapping)?;
        head::write_text(out, "query")?;
        head::write_text(out, &self.query)?;
        head::write_text(out, "selected")?;
        self.selected.write_cbor(out)?;
        Ok(())
    }
}

impl ProjectionProfilePayload {
    /// Retain the profile capability payload and every mandatory input check.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "catalog_id")?;
        head::write_text(out, &self.catalog_id)?;
        head::write_text(out, "profile")?;
        head::write_text(out, &self.profile)?;
        head::write_text(out, "forward")?;
        head::write_text(out, &self.forward)?;
        head::write_text(out, "checks")?;
        head::write_head(out, head::ARRAY, self.checks.len() as u64)?;
        for value in &self.checks {
            value.write_cbor(out)?;
        }
        Ok(())
    }
}

impl ProjectionProfileProgram {
    /// Retain the profile identity and its complete capability payload.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "id")?;
        head::write_text(out, &self.id)?;
        head::write_text(out, "payload")?;
        self.payload.write_cbor(out)?;
        Ok(())
    }
}
