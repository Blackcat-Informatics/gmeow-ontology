// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR of physical source ownership and selected property support.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use super::{GroundingCatalog, GroundingSourceIdentity, LiteralProjectionSupport, PropertyPremise};

impl GroundingCatalog {
    /// Commit the complete admitted program, property values and physical-source authority.
    /// This payload identity is broader than the selected native catalog identity.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::ARRAY, 5)?;
        head::write_text(out, "grounding-catalog-v1")?;
        self.program.write_cbor(out)?;
        head::write_head(out, head::ARRAY, self.properties.len() as u64)?;
        for ((subject, predicate), values) in &self.properties {
            head::write_head(out, head::ARRAY, 3)?;
            head::write_text(out, subject)?;
            head::write_text(out, predicate)?;
            head::write_head(out, head::ARRAY, values.values.len() as u64)?;
            for value in &values.values {
                crate::term_serde::write_cbor(value, out)?;
            }
        }
        head::write_head(out, head::MAP, self.authority_cells.len() as u64)?;
        for (iri, commitment) in &self.authority_cells {
            head::write_text(out, iri)?;
            head::write_text(out, commitment)?;
        }
        head::write_text(out, &self.identity)?;
        Ok(())
    }
}

impl GroundingSourceIdentity {
    /// Retain the complete physical source identity in declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "owner")?;
        head::write_text(out, &self.owner)?;
        head::write_text(out, "path")?;
        head::write_text(out, &self.path)?;
        head::write_text(out, "digest")?;
        head::write_text(out, &self.digest)?;
        Ok(())
    }
}

impl PropertyPremise {
    /// Retain each native property premise without parsing or normalizing it.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "subject")?;
        head::write_text(out, &self.subject)?;
        head::write_text(out, "predicate")?;
        head::write_text(out, &self.predicate)?;
        head::write_text(out, "object")?;
        head::write_text(out, &self.object)?;
        Ok(())
    }
}

impl LiteralProjectionSupport {
    /// Retain the complete selected support and recorded schema-premise sequence.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "alias")?;
        head::write_text(out, &self.alias)?;
        head::write_text(out, "correspondence")?;
        head::write_text(out, &self.correspondence)?;
        head::write_text(out, "correspondence_key")?;
        head::write_text(out, &self.correspondence_key)?;
        head::write_text(out, "source_property")?;
        head::write_text(out, &self.source_property)?;
        head::write_text(out, "target_property")?;
        head::write_text(out, &self.target_property)?;
        head::write_text(out, "owner")?;
        head::write_text(out, &self.owner)?;
        head::write_text(out, "premises")?;
        head::write_head(out, head::ARRAY, self.premises.len() as u64)?;
        for value in &self.premises {
            value.write_cbor(out)?;
        }
        Ok(())
    }
}
