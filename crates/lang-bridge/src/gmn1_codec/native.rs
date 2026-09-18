// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded native codebook transport. The producer serializes its already prepared
//! tables; consumers validate and hydrate them without RDF parsing or source lowering.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

use gmeow_errors::{FindingCategory, Grade, Severity, Standpoint, define_diag_kind};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{
    CurrentCodebook, DialectAcceptance, GmnDictionary, GmnGlyphRegistry, GmnGlyphSignature,
};

/// Original document whose exact bytes own the browser codebook.
pub const SOURCE_PATH: &str = "slices/grounding/lang/module.ttl";
/// Immutable original-byte BLAKE3 pin shared by both browser consumers.
/// The pure host identity contract requires updating this when the authored source changes.
pub const SOURCE_BLAKE3: &str = "7572ef1ea3b7e7403c0770bd825c181d54a651e23663a4f206dceefa7569ccbe";
/// Generated projection embedded by browser binaries after explicit production.
pub const GENERATED_PATH: &str = "generated/projections/lang/gmn-codebook.cbor";
/// Maximum encoded packet size, including its fixed integrity header.
pub const MAX_NATIVE_CODEBOOK_BYTES: usize = 8 * 1024 * 1024;

const MAGIC: &[u8; 8] = b"GMNCBK01";
const HEADER_BYTES: usize = MAGIC.len() + 32;
const PROFILE: &str = "gmn-native-dictionary-v1";
type Binding = (String, String, Option<String>, Option<u32>, String);

define_diag_kind! {
    /// A prepared native codebook cannot be admitted with its exact source identity.
    pub struct NativeCodebookInvalid { detail: String }
    code = "lang-bridge.gmn1.native-codebook";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "native codebook: {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/BundleArtifactUnreadable";
}

/// The complete prepared reader/writer tables and their original source identity.
/// Construction is only through validated packet decoding; fields remain private.
#[derive(Debug)]
pub struct NativeCodebook {
    dictionary: GmnDictionary,
    codebook: CurrentCodebook,
    source_sha256: String,
    source_blake3: String,
}

impl NativeCodebook {
    /// Consume the admitted packet without cloning either prepared table inventory.
    #[must_use]
    pub fn into_parts(self) -> (CurrentCodebook, GmnDictionary) {
        (self.codebook, self.dictionary)
    }

    /// Shared prepared alias, glyph, signature and dialect-acceptance tables.
    #[must_use]
    pub fn dictionary(&self) -> &GmnDictionary {
        &self.dictionary
    }

    /// The entire selected codebook membership and version metadata.
    #[must_use]
    pub fn codebook(&self) -> &CurrentCodebook {
        &self.codebook
    }

    /// Exact SHA-256 of the original source bytes admitted by the producer.
    #[must_use]
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// Exact original-byte BLAKE3, preserving the browser's public digest API.
    #[must_use]
    pub fn source_blake3(&self) -> &str {
        &self.source_blake3
    }
}

fn fail(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(NativeCodebookInvalid {
        detail: detail.into(),
    })
}

fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Acceptance {
    latest_major: u32,
    accept_window: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodebookWire {
    references: Vec<String>,
    dictionary_version: String,
    glyph_version: String,
    graphemes: Vec<String>,
    dictionary_entries: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DictionaryWire {
    version: String,
    term_to_alias: Vec<(String, String)>,
    alias_to_term: Vec<(String, String)>,
    glyph_version: String,
    term_to_glyph: Vec<Binding>,
    glyph_to_term: Vec<Binding>,
    fallback_to_term: Vec<Binding>,
    acceptance: Acceptance,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema_version: u32,
    profile: String,
    source_path: String,
    source_sha256: String,
    source_blake3: String,
    codebook_digest: String,
    codebook: CodebookWire,
    dictionary: DictionaryWire,
}

// Borrow native inventories during serialization. Only the final compact packet is
// allocated; no second dictionary, registry or RDF representation is materialized.
struct Pairs<'a>(&'a BTreeMap<String, String>);
impl Serialize for Pairs<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for row in self.0 {
            sequence.serialize_element(&row)?;
        }
        sequence.end()
    }
}

struct Bindings<'a>(&'a BTreeMap<(String, String, GmnGlyphSignature), String>);
impl Serialize for Bindings<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for ((scope, key, signature), value) in self.0 {
            sequence.serialize_element(&(scope, key, &signature.fixity, signature.arity, value))?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct CodebookRef<'a> {
    references: &'a BTreeSet<String>,
    dictionary_version: &'a str,
    glyph_version: &'a str,
    graphemes: &'a BTreeSet<String>,
    dictionary_entries: &'a BTreeSet<String>,
}

#[derive(Serialize)]
struct DictionaryRef<'a> {
    version: &'a str,
    term_to_alias: Pairs<'a>,
    alias_to_term: Pairs<'a>,
    glyph_version: &'a str,
    term_to_glyph: Bindings<'a>,
    glyph_to_term: Bindings<'a>,
    fallback_to_term: Bindings<'a>,
    acceptance: Acceptance,
}

#[derive(Serialize)]
struct WireRef<'a> {
    schema_version: u32,
    profile: &'a str,
    source_path: &'a str,
    source_sha256: &'a str,
    source_blake3: &'a str,
    codebook_digest: String,
    codebook: CodebookRef<'a>,
    dictionary: DictionaryRef<'a>,
}

/// Encode the producer's shared native objects, retaining every reader/writer field.
///
/// Source digests must describe the same original document used to compile the supplied
/// objects. The enclosing producer action authenticates that relationship and its recipe.
///
/// # Errors
/// Rejects malformed identities, inconsistent native tables or an oversized packet.
pub fn encode(
    dictionary: &GmnDictionary,
    codebook: &CurrentCodebook,
    source_sha256: &str,
    source_blake3: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    if !hex_digest(source_sha256) || !hex_digest(source_blake3) {
        return Err(fail("native codebook requires exact source digests"));
    }
    validate_tables(dictionary, codebook)?;
    let glyphs = &dictionary.glyphs;
    packet(&WireRef {
        schema_version: 1,
        profile: PROFILE,
        source_path: SOURCE_PATH,
        source_sha256,
        source_blake3,
        codebook_digest: crate::gmn1_digest::codebook_digest(codebook, dictionary),
        codebook: CodebookRef {
            references: &codebook.references,
            dictionary_version: &codebook.dictionary_version,
            glyph_version: &codebook.glyph_version,
            graphemes: &codebook.graphemes,
            dictionary_entries: &codebook.dictionary_entries,
        },
        dictionary: DictionaryRef {
            version: &dictionary.version,
            term_to_alias: Pairs(&dictionary.term_to_alias),
            alias_to_term: Pairs(&dictionary.alias_to_term),
            glyph_version: &glyphs.version,
            term_to_glyph: Bindings(&glyphs.term_to_glyph),
            glyph_to_term: Bindings(&glyphs.glyph_to_term),
            fallback_to_term: Bindings(&glyphs.fallback_to_term),
            acceptance: Acceptance {
                latest_major: dictionary.acceptance.latest_major,
                accept_window: dictionary.acceptance.accept_window,
            },
        },
    })
}

fn packet(value: &impl Serialize) -> gmeow_errors::Result<Vec<u8>> {
    let mut bytes = vec![0; HEADER_BYTES];
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|error| fail(format!("encode native codebook: {error}")))?;
    if bytes.len() > MAX_NATIVE_CODEBOOK_BYTES {
        return Err(fail("native codebook exceeds its 8-MiB bound"));
    }
    let digest = Sha256::digest(&bytes[HEADER_BYTES..]);
    bytes[..MAGIC.len()].copy_from_slice(MAGIC);
    bytes[MAGIC.len()..HEADER_BYTES].copy_from_slice(&digest);
    Ok(bytes)
}

/// Hydrate a bounded native packet for one exact, caller-selected source identity.
///
/// Validates integrity, schema/profile, the complete codebook Merkle identity, native
/// table consistency and all glyph security/scope rules. It never parses authored RDF
/// or substitutes a default dictionary. Producer receipt admission remains the caller's
/// responsibility; the internal checksum detects corruption, not producer authenticity.
///
/// # Errors
/// Rejects corrupt, trailing, oversized or incompatible packets and inconsistent tables.
pub fn decode(bytes: &[u8], expected_source_blake3: &str) -> gmeow_errors::Result<NativeCodebook> {
    if bytes.len() < HEADER_BYTES
        || bytes.len() > MAX_NATIVE_CODEBOOK_BYTES
        || &bytes[..MAGIC.len()] != MAGIC
    {
        return Err(fail("native codebook has an invalid header or size"));
    }
    if Sha256::digest(&bytes[HEADER_BYTES..])[..] != bytes[MAGIC.len()..HEADER_BYTES] {
        return Err(fail("native codebook payload checksum mismatch"));
    }
    let mut reader = Cursor::new(&bytes[HEADER_BYTES..]);
    let wire: Wire = ciborium::de::from_reader_with_recursion_limit(&mut reader, 32)
        .map_err(|error| fail(format!("decode native codebook: {error}")))?;
    if reader.position() as usize != bytes.len() - HEADER_BYTES {
        return Err(fail("native codebook contains trailing data"));
    }
    if wire.schema_version != 1
        || wire.profile != PROFILE
        || wire.source_path != SOURCE_PATH
        || !hex_digest(&wire.source_sha256)
        || !hex_digest(expected_source_blake3)
        || wire.source_blake3 != expected_source_blake3
    {
        return Err(fail(
            "native codebook profile or exact source identity mismatch",
        ));
    }
    let codebook = CurrentCodebook {
        references: ordered_set(wire.codebook.references)?,
        dictionary_version: wire.codebook.dictionary_version,
        glyph_version: wire.codebook.glyph_version,
        graphemes: ordered_set(wire.codebook.graphemes)?,
        dictionary_entries: ordered_set(wire.codebook.dictionary_entries)?,
    };
    let fields = wire.dictionary;
    let dictionary = GmnDictionary {
        version: fields.version,
        term_to_alias: ordered_map(fields.term_to_alias)?,
        alias_to_term: ordered_map(fields.alias_to_term)?,
        glyphs: GmnGlyphRegistry {
            version: fields.glyph_version,
            term_to_glyph: binding_map(fields.term_to_glyph)?,
            glyph_to_term: binding_map(fields.glyph_to_term)?,
            fallback_to_term: binding_map(fields.fallback_to_term)?,
        },
        acceptance: DialectAcceptance {
            latest_major: fields.acceptance.latest_major,
            accept_window: fields.acceptance.accept_window,
        },
    };
    validate_tables(&dictionary, &codebook)?;
    if crate::gmn1_digest::codebook_digest(&codebook, &dictionary) != wire.codebook_digest {
        return Err(fail("native codebook Merkle identity mismatch"));
    }
    Ok(NativeCodebook {
        dictionary,
        codebook,
        source_sha256: wire.source_sha256,
        source_blake3: wire.source_blake3,
    })
}

fn ordered_set(values: Vec<String>) -> gmeow_errors::Result<BTreeSet<String>> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(fail("native codebook membership is not strictly ordered"));
    }
    Ok(values.into_iter().collect())
}

fn ordered_map<K: Ord, V>(values: Vec<(K, V)>) -> gmeow_errors::Result<BTreeMap<K, V>> {
    if values.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(fail(
            "native codebook table has duplicate or unordered keys",
        ));
    }
    Ok(values.into_iter().collect())
}

fn binding_map(
    rows: Vec<Binding>,
) -> gmeow_errors::Result<BTreeMap<(String, String, GmnGlyphSignature), String>> {
    ordered_map(
        rows.into_iter()
            .map(|(scope, key, fixity, arity, value)| {
                ((scope, key, GmnGlyphSignature { fixity, arity }), value)
            })
            .collect(),
    )
}

fn valid_alias(alias: &str) -> bool {
    super::is_identifier(alias)
        && ![
            super::BLANK_PREFIX,
            super::REF_PREFIX,
            super::REF_IRI_PREFIX,
        ]
        .iter()
        .any(|prefix| alias.starts_with(prefix))
}

fn validate_tables(
    dictionary: &GmnDictionary,
    codebook: &CurrentCodebook,
) -> gmeow_errors::Result<()> {
    if dictionary.version != super::DICTIONARY_VERSION
        || codebook.dictionary_version != dictionary.version
        || dictionary.glyphs.version != super::GLYPH_VERSION
        || codebook.glyph_version != dictionary.glyphs.version
        || codebook.references.is_empty()
        || codebook.graphemes.is_empty()
    {
        return Err(fail(
            "native codebook has incompatible versions or missing selection metadata",
        ));
    }
    for (term, alias) in &dictionary.term_to_alias {
        if !valid_alias(alias) || dictionary.alias_to_term.get(alias) != Some(term) {
            return Err(fail(
                "native dictionary canonical alias disagrees with its read table",
            ));
        }
    }
    // Multiple read aliases for one term remain legal. The native writer chooses one
    // canonical alias; restoring must preserve all additional reader bindings as well.
    for (alias, term) in &dictionary.alias_to_term {
        if !valid_alias(alias) || !dictionary.term_to_alias.contains_key(term) {
            return Err(fail("native dictionary has an invalid read alias"));
        }
    }
    let glyphs = &dictionary.glyphs;
    if glyphs.term_to_glyph.len() != glyphs.glyph_to_term.len() {
        return Err(fail("native glyph tables differ in cardinality"));
    }
    let mut skeletons = BTreeMap::new();
    for ((scope, term, signature), glyph) in &glyphs.term_to_glyph {
        if !scope.is_empty() && !super::KNOWN_SIGILS.contains(&scope.as_str()) {
            return Err(fail("native glyph has an unknown sigil scope"));
        }
        if signature.fixity.is_some() != signature.arity.is_some()
            || glyph.is_empty()
            || glyphs
                .glyph_to_term
                .get(&(scope.clone(), glyph.clone(), signature.clone()))
                != Some(term)
            || dictionary.alias_to_term.contains_key(glyph)
        {
            return Err(fail("native glyph binding or signature is inconsistent"));
        }
        super::validate_glyph_surface(glyph).map_err(|error| fail(error.0))?;
        let skeleton = (scope, unicode_security::skeleton(glyph).collect::<String>());
        if skeletons
            .insert(skeleton, glyph)
            .is_some_and(|prior| prior != glyph)
        {
            return Err(fail("native glyphs are confusable within one scope"));
        }
    }
    let mut fallback_targets = BTreeSet::new();
    for ((scope, alias, signature), term) in &glyphs.fallback_to_term {
        let target = (scope.clone(), term.clone(), signature.clone());
        if !valid_alias(alias)
            || dictionary.alias_to_term.contains_key(alias)
            || !glyphs.term_to_glyph.contains_key(&target)
        {
            return Err(fail(
                "native glyph fallback is invalid or has no matching writer binding",
            ));
        }
        fallback_targets.insert(target);
    }
    if glyphs
        .term_to_glyph
        .keys()
        .any(|key| !fallback_targets.contains(key))
    {
        return Err(fail(
            "native glyph writer binding has no ASCII read fallback",
        ));
    }
    glyphs
        .reject_bare_signature_ambiguity()
        .map_err(|error| fail(error.0))
}

#[path = "native.tests.rs"]
#[cfg(test)]
mod tests;
