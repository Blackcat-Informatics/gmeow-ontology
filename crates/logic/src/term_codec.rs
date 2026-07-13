// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native N-Triples term-surface decoding shared by rule and artifact code.

use purrdf::TermValue;

fn decode_err(context: &str, got: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir {
        detail: format!("term decode error [{context}]: {got:?}"),
    })
}

fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_literal_content(s: &str) -> gmeow_errors::Result<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == b'"' {
            return Ok((&s[..index], &s[index + 1..]));
        }
        index += 1;
    }
    Err(decode_err("unterminated literal", s))
}

pub(crate) fn decode_term(surface: &str) -> gmeow_errors::Result<TermValue> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

    if let Some(iri) = surface
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
    {
        if iri.is_empty() {
            return Err(decode_err("invalid IRI", surface));
        }
        return Ok(TermValue::iri(iri));
    }

    let Some(content) = surface.strip_prefix('"') else {
        return Err(decode_err("unrecognized term", surface));
    };
    let (raw_value, suffix) = split_literal_content(content)?;
    let lexical_form = unescape_string(raw_value);
    if suffix.is_empty() {
        return Ok(TermValue::Literal {
            lexical_form,
            datatype: XSD_STRING.to_owned(),
            language: None,
            direction: None,
        });
    }
    if let Some(language) = suffix.strip_prefix('@') {
        if language.is_empty() {
            return Err(decode_err("empty language tag", surface));
        }
        return Ok(TermValue::Literal {
            lexical_form,
            datatype: RDF_LANG_STRING.to_owned(),
            language: Some(language.to_owned()),
            direction: None,
        });
    }
    if let Some(datatype) = suffix
        .strip_prefix("^^<")
        .and_then(|value| value.strip_suffix('>'))
    {
        if datatype.is_empty() {
            return Err(decode_err("empty datatype IRI", surface));
        }
        return Ok(TermValue::Literal {
            lexical_form,
            datatype: datatype.to_owned(),
            language: None,
            direction: None,
        });
    }
    Err(decode_err("unrecognized literal suffix", suffix))
}
