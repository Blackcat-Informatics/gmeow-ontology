// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The codebook's named-map domain codec over the shared native CBOR substrate.
//! Writers borrow the prepared inventories; readers consume the decoded values.

use super::{
    Binding, CurrentCodebook, GmnDictionary, GmnGlyphSignature, PROFILE, SOURCE_PATH, fail,
};
use purrdf_lex::cbor::{
    Value,
    head::{self, ARRAY, MAP, SIMPLE, UNSIGNED},
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

type Result<T> = gmeow_errors::Result<T>;

pub(super) struct Acceptance {
    pub(super) latest_major: u32,
    pub(super) accept_window: u32,
}

pub(super) struct CodebookWire {
    pub(super) references: Vec<String>,
    pub(super) dictionary_version: String,
    pub(super) glyph_version: String,
    pub(super) graphemes: Vec<String>,
    pub(super) dictionary_entries: Vec<String>,
}

pub(super) struct DictionaryWire {
    pub(super) version: String,
    pub(super) term_to_alias: Vec<(String, String)>,
    pub(super) alias_to_term: Vec<(String, String)>,
    pub(super) glyph_version: String,
    pub(super) term_to_glyph: Vec<Binding>,
    pub(super) glyph_to_term: Vec<Binding>,
    pub(super) fallback_to_term: Vec<Binding>,
    pub(super) acceptance: Acceptance,
}

pub(super) struct Wire {
    pub(super) schema_version: u32,
    pub(super) profile: String,
    pub(super) source_path: String,
    pub(super) source_sha256: String,
    pub(super) source_blake3: String,
    pub(super) codebook_digest: String,
    pub(super) codebook: CodebookWire,
    pub(super) dictionary: DictionaryWire,
}

pub(super) fn write(
    out: &mut impl Write,
    dictionary: &GmnDictionary,
    codebook: &CurrentCodebook,
    source_sha256: &str,
    source_blake3: &str,
    codebook_digest: &str,
) -> io::Result<()> {
    head::write_head(out, MAP, 8)?;
    head::write_text(out, "schema_version")?;
    head::write_head(out, UNSIGNED, 1)?;
    for (name, value) in [
        ("profile", PROFILE),
        ("source_path", SOURCE_PATH),
        ("source_sha256", source_sha256),
        ("source_blake3", source_blake3),
        ("codebook_digest", codebook_digest),
    ] {
        head::write_text(out, name)?;
        head::write_text(out, value)?;
    }
    head::write_text(out, "codebook")?;
    write_codebook(out, codebook)?;
    head::write_text(out, "dictionary")?;
    write_dictionary(out, dictionary)
}

fn write_strings(out: &mut impl Write, values: &BTreeSet<String>) -> io::Result<()> {
    head::write_head(out, ARRAY, values.len() as u64)?;
    for value in values {
        head::write_text(out, value)?;
    }
    Ok(())
}

fn write_codebook(out: &mut impl Write, codebook: &CurrentCodebook) -> io::Result<()> {
    head::write_head(out, MAP, 5)?;
    head::write_text(out, "references")?;
    write_strings(out, &codebook.references)?;
    head::write_text(out, "dictionary_version")?;
    head::write_text(out, &codebook.dictionary_version)?;
    head::write_text(out, "glyph_version")?;
    head::write_text(out, &codebook.glyph_version)?;
    head::write_text(out, "graphemes")?;
    write_strings(out, &codebook.graphemes)?;
    head::write_text(out, "dictionary_entries")?;
    write_strings(out, &codebook.dictionary_entries)
}

fn write_dictionary(out: &mut impl Write, dictionary: &GmnDictionary) -> io::Result<()> {
    let glyphs = &dictionary.glyphs;
    head::write_head(out, MAP, 8)?;
    head::write_text(out, "version")?;
    head::write_text(out, &dictionary.version)?;
    head::write_text(out, "term_to_alias")?;
    write_pairs(out, &dictionary.term_to_alias)?;
    head::write_text(out, "alias_to_term")?;
    write_pairs(out, &dictionary.alias_to_term)?;
    head::write_text(out, "glyph_version")?;
    head::write_text(out, &glyphs.version)?;
    for (name, values) in [
        ("term_to_glyph", &glyphs.term_to_glyph),
        ("glyph_to_term", &glyphs.glyph_to_term),
        ("fallback_to_term", &glyphs.fallback_to_term),
    ] {
        head::write_text(out, name)?;
        write_bindings(out, values)?;
    }
    head::write_text(out, "acceptance")?;
    head::write_head(out, MAP, 2)?;
    head::write_text(out, "latest_major")?;
    head::write_head(out, UNSIGNED, u64::from(dictionary.acceptance.latest_major))?;
    head::write_text(out, "accept_window")?;
    head::write_head(
        out,
        UNSIGNED,
        u64::from(dictionary.acceptance.accept_window),
    )
}

fn write_pairs(out: &mut impl Write, values: &BTreeMap<String, String>) -> io::Result<()> {
    head::write_head(out, ARRAY, values.len() as u64)?;
    for (left, right) in values {
        head::write_head(out, ARRAY, 2)?;
        head::write_text(out, left)?;
        head::write_text(out, right)?;
    }
    Ok(())
}

fn write_bindings(
    out: &mut impl Write,
    values: &BTreeMap<(String, String, GmnGlyphSignature), String>,
) -> io::Result<()> {
    head::write_head(out, ARRAY, values.len() as u64)?;
    for ((scope, key, signature), value) in values {
        head::write_head(out, ARRAY, 5)?;
        head::write_text(out, scope)?;
        head::write_text(out, key)?;
        match &signature.fixity {
            Some(fixity) => head::write_text(out, fixity)?,
            None => head::write_head(out, SIMPLE, 22)?,
        }
        match signature.arity {
            Some(arity) => head::write_head(out, UNSIGNED, u64::from(arity))?,
            None => head::write_head(out, SIMPLE, 22)?,
        }
        head::write_text(out, value)?;
    }
    Ok(())
}

// Fixed domain fields only. The shared decoder has already parsed CBOR syntax.
// Moving each retained value avoids cloning text or a second inventory tree.
fn record<const N: usize>(value: Value, name: &str, names: [&str; N]) -> Result<[Value; N]> {
    let entries = value
        .into_map()
        .map_err(|_| fail(format!("{name} must be a named CBOR map")))?;
    let mut fields: [Value; N] = std::array::from_fn(|_| Value::Null);
    let mut seen = [false; N];
    for (key, value) in entries {
        let key = text(key, name)?;
        let Some(index) = names.iter().position(|name| *name == key.as_str()) else {
            return Err(fail(format!("{name} has unknown field {key:?}")));
        };
        if seen[index] {
            return Err(fail(format!("{name} has duplicate field {key:?}")));
        }
        seen[index] = true;
        fields[index] = value;
    }
    for (index, present) in seen.into_iter().enumerate() {
        if !present {
            return Err(fail(format!("{name} is missing field {:?}", names[index])));
        }
    }
    Ok(fields)
}

fn text(value: Value, context: &str) -> Result<String> {
    value
        .into_text()
        .map_err(|_| fail(format!("{context} must be CBOR text")))
}

fn array(value: Value, context: &str) -> Result<Vec<Value>> {
    value
        .into_array()
        .map_err(|_| fail(format!("{context} must be a CBOR array")))
}

fn row<const N: usize>(value: Value, context: &str) -> Result<[Value; N]> {
    array(value, context)?
        .try_into()
        .map_err(|_| fail(format!("{context} must contain exactly {N} cells")))
}

fn unsigned(value: Value, context: &str) -> Result<u32> {
    if let Value::Integer(value) = value {
        return u32::try_from(value.get())
            .map_err(|_| fail(format!("{context} must be an unsigned u32")));
    }
    Err(fail(format!("{context} must be an unsigned u32")))
}

fn optional<T>(value: Value, read: impl FnOnce(Value) -> Result<T>) -> Result<Option<T>> {
    match value {
        Value::Null => Ok(None),
        value => read(value).map(Some),
    }
}

fn strings(value: Value, context: &str) -> Result<Vec<String>> {
    array(value, context)?
        .into_iter()
        .map(|value| text(value, context))
        .collect()
}

fn pairs(value: Value, context: &str) -> Result<Vec<(String, String)>> {
    array(value, context)?
        .into_iter()
        .map(|value| {
            let [left, right] = row(value, context)?;
            Ok((text(left, context)?, text(right, context)?))
        })
        .collect()
}

fn bindings(value: Value, context: &str) -> Result<Vec<Binding>> {
    array(value, context)?
        .into_iter()
        .map(|value| {
            let [scope, key, fixity, arity, value] = row(value, context)?;
            Ok((
                text(scope, context)?,
                text(key, context)?,
                optional(fixity, |value| text(value, context))?,
                optional(arity, |value| unsigned(value, context))?,
                text(value, context)?,
            ))
        })
        .collect()
}

impl Wire {
    pub(super) fn from_value(value: Value) -> Result<Self> {
        let [
            schema_version,
            profile,
            source_path,
            source_sha256,
            source_blake3,
            codebook_digest,
            codebook,
            dictionary,
        ] = record(
            value,
            "codebook packet",
            [
                "schema_version",
                "profile",
                "source_path",
                "source_sha256",
                "source_blake3",
                "codebook_digest",
                "codebook",
                "dictionary",
            ],
        )?;
        Ok(Self {
            schema_version: unsigned(schema_version, "schema_version")?,
            profile: text(profile, "profile")?,
            source_path: text(source_path, "source_path")?,
            source_sha256: text(source_sha256, "source_sha256")?,
            source_blake3: text(source_blake3, "source_blake3")?,
            codebook_digest: text(codebook_digest, "codebook_digest")?,
            codebook: CodebookWire::from_value(codebook)?,
            dictionary: DictionaryWire::from_value(dictionary)?,
        })
    }
}

impl CodebookWire {
    fn from_value(value: Value) -> Result<Self> {
        let [
            references,
            dictionary_version,
            glyph_version,
            graphemes,
            dictionary_entries,
        ] = record(
            value,
            "codebook",
            [
                "references",
                "dictionary_version",
                "glyph_version",
                "graphemes",
                "dictionary_entries",
            ],
        )?;
        Ok(Self {
            references: strings(references, "references")?,
            dictionary_version: text(dictionary_version, "dictionary_version")?,
            glyph_version: text(glyph_version, "glyph_version")?,
            graphemes: strings(graphemes, "graphemes")?,
            dictionary_entries: strings(dictionary_entries, "dictionary_entries")?,
        })
    }
}

impl DictionaryWire {
    fn from_value(value: Value) -> Result<Self> {
        let [
            version,
            term_to_alias,
            alias_to_term,
            glyph_version,
            term_to_glyph,
            glyph_to_term,
            fallback_to_term,
            acceptance,
        ] = record(
            value,
            "dictionary",
            [
                "version",
                "term_to_alias",
                "alias_to_term",
                "glyph_version",
                "term_to_glyph",
                "glyph_to_term",
                "fallback_to_term",
                "acceptance",
            ],
        )?;
        Ok(Self {
            version: text(version, "dictionary.version")?,
            term_to_alias: pairs(term_to_alias, "term_to_alias")?,
            alias_to_term: pairs(alias_to_term, "alias_to_term")?,
            glyph_version: text(glyph_version, "dictionary.glyph_version")?,
            term_to_glyph: bindings(term_to_glyph, "term_to_glyph")?,
            glyph_to_term: bindings(glyph_to_term, "glyph_to_term")?,
            fallback_to_term: bindings(fallback_to_term, "fallback_to_term")?,
            acceptance: Acceptance::from_value(acceptance)?,
        })
    }
}

impl Acceptance {
    fn from_value(value: Value) -> Result<Self> {
        let [latest_major, accept_window] =
            record(value, "acceptance", ["latest_major", "accept_window"])?;
        Ok(Self {
            latest_major: unsigned(latest_major, "latest_major")?,
            accept_window: unsigned(accept_window, "accept_window")?,
        })
    }
}
