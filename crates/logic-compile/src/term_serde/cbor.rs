// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR spelling of the existing retained-term wire.

use std::io::{self, Write};

use purrdf::{RdfTextDirection, TermValue};
use purrdf_lex::cbor::{self, Value, head};

/// Stream the retained term wire without cloning the term or its nested values.
/// Its externally tagged variants and field order match this owner's Serde wire.
///
/// # Errors
/// Returns the sink's failure without publishing a partial success.
pub fn write_cbor<W: Write + ?Sized>(value: &TermValue, out: &mut W) -> io::Result<()> {
    enum Item<'a> {
        Term(&'a TermValue),
        Key(&'static str),
    }
    let mut pending = vec![Item::Term(value)];
    while let Some(item) = pending.pop() {
        let term = match item {
            Item::Term(term) => term,
            Item::Key(key) => {
                head::write_text(out, key)?;
                continue;
            }
        };
        head::write_head(out, head::MAP, 1)?;
        match term {
            TermValue::Iri(iri) => {
                head::write_text(out, "Iri")?;
                head::write_text(out, iri)?;
            }
            TermValue::Blank { label, scope } => {
                head::write_text(out, "Blank")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "label")?;
                head::write_text(out, label)?;
                head::write_text(out, "scope")?;
                head::write_head(out, head::UNSIGNED, u64::from(scope.0))?;
            }
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => {
                head::write_text(out, "Literal")?;
                head::write_head(out, head::MAP, 4)?;
                head::write_text(out, "lexical_form")?;
                head::write_text(out, lexical_form)?;
                head::write_text(out, "datatype")?;
                head::write_text(out, datatype)?;
                head::write_text(out, "language")?;
                match language {
                    Some(language) => head::write_text(out, language)?,
                    None => cbor::write(&Value::Null, out)?,
                }
                head::write_text(out, "direction")?;
                match direction {
                    Some(RdfTextDirection::Ltr) => head::write_text(out, "Ltr")?,
                    Some(RdfTextDirection::Rtl) => head::write_text(out, "Rtl")?,
                    None => cbor::write(&Value::Null, out)?,
                }
            }
            TermValue::Triple { s, p, o } => {
                head::write_text(out, "Triple")?;
                head::write_head(out, head::MAP, 3)?;
                // Stack order preserves the declaration's s, p, o field order.
                pending.extend([
                    Item::Term(o.as_ref()),
                    Item::Key("o"),
                    Item::Term(p.as_ref()),
                    Item::Key("p"),
                    Item::Term(s.as_ref()),
                    Item::Key("s"),
                ]);
            }
        }
    }
    Ok(())
}

/// Stream an explicitly absent default graph or its complete retained term.
///
/// # Errors
/// Returns the sink's original failure.
pub fn write_optional_cbor<W: Write + ?Sized>(
    value: &Option<TermValue>,
    out: &mut W,
) -> io::Result<()> {
    match value {
        Some(term) => write_cbor(term, out),
        None => cbor::write(&Value::Null, out),
    }
}
