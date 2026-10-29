// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR of physical source ownership and selected property support.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use super::{GroundingSourceIdentity, LiteralProjectionSupport, PropertyPremise};

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
