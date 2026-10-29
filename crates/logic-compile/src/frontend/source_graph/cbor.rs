// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR spelling of complete portable source-document receipts.

use std::io::{self, Write};

use purrdf_lex::cbor::{self, Value, head};

use super::{SourceBase, SourceBaseOrigin, SourceDocument};

impl SourceDocument {
    /// Stream every receipt field in its existing serialized declaration order.
    /// The original role, optional base and admitted origin remain distinct.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "path")?;
        head::write_text(out, &self.path)?;
        head::write_text(out, "content_digest")?;
        head::write_text(out, &self.content_digest)?;
        head::write_text(out, "role")?;
        head::write_text(out, &self.role)?;
        head::write_text(out, "base")?;
        match &self.base {
            Some(base) => write_base(base, out),
            None => cbor::write(&Value::Null, out),
        }
    }
}

/// Retain the exact base IRI followed by its original admission origin.
fn write_base<W: Write + ?Sized>(base: &SourceBase, out: &mut W) -> io::Result<()> {
    head::write_head(out, head::MAP, 2)?;
    head::write_text(out, "iri")?;
    head::write_text(out, &base.iri)?;
    head::write_text(out, "origin")?;
    write_origin(&base.origin, out)
}

/// Retain each origin variant and the directive's unsigned line/column order.
fn write_origin<W: Write + ?Sized>(origin: &SourceBaseOrigin, out: &mut W) -> io::Result<()> {
    match origin {
        SourceBaseOrigin::Caller => head::write_text(out, "Caller"),
        SourceBaseOrigin::Enclosing => head::write_text(out, "Enclosing"),
        SourceBaseOrigin::Directive { line, column } => {
            head::write_head(out, head::MAP, 1)?;
            head::write_text(out, "Directive")?;
            head::write_head(out, head::MAP, 2)?;
            head::write_text(out, "line")?;
            head::write_head(out, head::UNSIGNED, *line)?;
            head::write_text(out, "column")?;
            head::write_head(out, head::UNSIGNED, u64::from(*column))
        }
    }
}
