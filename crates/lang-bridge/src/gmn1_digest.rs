// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN codebook + content digest layer.
//!
//! Two content addresses back the GMN envelope's integrity header:
//!
//! * [`codebook_digest`] — the identity of the CODEBOOK a document decodes against
//!   (`gmeow:gmnCodebookDigest`). A **Merkle root** over per-part leaves of the codebook:
//!   the dialect/dictionary/glyph versions, the reference inventory, the script graphemes,
//!   the alias bijection, and the scoped glyph + fallback tables. A reader recomputes this
//!   root and refuses to decode when it disagrees with an envelope's declared value
//!   (`lang:GmnCodebookDigestMismatch`).
//! * [`content_digest`] — the byte-exact identity of the MODEL a document carries
//!   (`gmeow:contentDigest`), folded over the RDFC-1.0 canonical N-Quads. Because two GMN-1
//!   surface encodings of one model (record form vs. tabular form) canonicalize to the SAME
//!   [`Gmn0Model`], they share ONE content digest — the digest names the model, not the
//!   surface bytes.
//!
//! Both fold with blake3, mirroring the per-part hash aggregation
//! `crates/logic`'s coherence certificate uses (`certificate::content_id` /
//! `per_graph_axiom_hashes`): one primitive, so an independent implementation reproduces
//! the byte-exact digest.
//!
//! # Normative wire format (pinned — an independent implementation MUST match it)
//!
//! **Field separator.** `\u{1F}` (Unicode INFORMATION SEPARATOR ONE), the SAME unit
//! separator the codec already keys by-reference literals with ([`crate::gmn1_codec`]'s
//! `classify_literal`). Written [`FIELD_SEP`] below.
//!
//! **Per-part canonical bytes.** A part is a list of entry lines. A key→value entry is
//! `key␟value` (`␟` = [`FIELD_SEP`]); a multi-field entry joins its fields with `␟`; a
//! set entry is the bare element. Every line is NFC-normalized, the lines are sorted by
//! Rust `str` `Ord` (Unicode-scalar/codepoint order) over those NFC forms, and joined by
//! `\n` (no trailing newline). `leaf = blake3(part-bytes)`, lowercase hex.
//!
//! **Root.** The parts are laid out in this FIXED order, each contributing one leaf:
//!
//! | # | label                  | entries |
//! |---|------------------------|---------|
//! | 1 | `dialect-version`      | the codec dialect (`v:`) version |
//! | 2 | `dictionary-version`   | the pinned dictionary version |
//! | 3 | `glyph-table-version`  | the pinned glyph-table version |
//! | 4 | `codebook-references`  | the `gmeow:references` inventory IRIs (set) |
//! | 5 | `script-graphemes`     | the current script's `lang:hasGrapheme` IRIs (set) |
//! | 6 | `dictionary-aliases`   | `term␟alias` lines of the alias bijection |
//! | 7 | `glyph-table`          | `sigil␟glyph␟fixity␟arity␟term` lines |
//! | 8 | `glyph-fallback-table` | `sigil␟fallback␟fixity␟arity␟term` lines |
//!
//! The root pre-image concatenates `label␟leaf-hex\n` for each part IN THAT ORDER, and
//! `root = blake3(pre-image)`. The returned digest is `"blake3:"` followed by the 64-char
//! lowercase root hex.
//!
//! The per-part leaves are INTERNAL: no public per-part API, no ontology term. They are
//! surfaced only through the crate-internal [`codebook_digest_leaves`], so a
//! codebook-mismatch diagnostic (and this module's own sensitivity test) can name WHICH
//! leaf diverged, never as a shipped enumeration.

use unicode_normalization::UnicodeNormalization;

use crate::gmn1_codec::{CurrentCodebook, Gmn0Model, GmnDictionary, dialect_version};

/// The pinned inter-field separator (`\u{1F}`), matching the codec's by-reference literal
/// key separator so the digest never introduces a second delimiter convention.
const FIELD_SEP: char = '\u{1f}';

/// The `blake3:` algorithm tag every digest this layer emits carries, so a consumer reads
/// the algorithm off the string rather than assuming it.
const ALGO_PREFIX: &str = "blake3:";

/// The content-addressed identity of a GMN codebook: a blake3 **Merkle root** over the
/// per-part leaves enumerated in the module's normative wire-format table. Deterministic
/// and input-only — no clock, rng, or environment — so the same codebook always yields the
/// same digest, and an independent implementation of the pinned format reproduces it.
///
/// Returns `"blake3:<64-hex>"`. The glyph registry is reached through
/// [`GmnDictionary::glyph_registry`], so the minimal carrier of a codebook's identity is
/// its resolved [`CurrentCodebook`] plus its [`GmnDictionary`].
#[must_use]
pub fn codebook_digest(codebook: &CurrentCodebook, dict: &GmnDictionary) -> String {
    let leaves = codebook_digest_leaves(codebook, dict);
    let mut preimage = String::new();
    for (label, leaf) in &leaves {
        preimage.push_str(label);
        preimage.push(FIELD_SEP);
        preimage.push_str(leaf);
        preimage.push('\n');
    }
    format!(
        "{ALGO_PREFIX}{}",
        blake3::hash(preimage.as_bytes()).to_hex()
    )
}

/// The content-addressed **conformance-pack Merkle root** (`gmeow:gmnPackRoot`): the GMN-1
/// version identity an independent decoder pins against. A blake3 root over exactly three
/// ordered part leaves, folded with the SAME wire format as [`codebook_digest`] — each part
/// contributes `label␟leaf\n` (`␟` = [`FIELD_SEP`]) to the root pre-image IN THIS ORDER,
/// `root = blake3(pre-image)`, returned as `"blake3:<64-hex>"`:
///
/// | # | label             | leaf |
/// |---|-------------------|------|
/// | 1 | `codebook-digest` | the codebook digest string VERBATIM (algorithm-tagged `blake3:<hex>`) — the pack pins the codebook by its PUBLISHED identity, so a consumer recomputes the root directly from the value the codebook already carries |
/// | 2 | `gmn-grammar`     | `blake3(authored gmn.ebnf bytes)`, lowercase hex — the authored grammar template byte-exact (the `glyphToken` seam is realized from leaf 3, so the template and the glyph table are pinned independently) |
/// | 3 | `sigil-table`     | the [`leaf_hex`] of the executable sigil→glyph binding rows (the glyph table then the fallback table), each row `sigil␟surface␟fixity␟arity␟term`, NFC-normalized + sorted — the SAME per-part canonicalization the codebook's glyph leaves use |
///
/// Deterministic and input-only (no clock, rng, or environment). The pack is self-certifying:
/// a consumer recomputes this root from the three parts the pack `gmeow:references` and
/// refuses a pack whose declared root disagrees.
#[must_use]
pub fn pack_root(codebook_digest: &str, dict: &GmnDictionary, grammar_bytes: &[u8]) -> String {
    pack_root_from_grammar_leaf(codebook_digest, dict, &grammar_leaf(grammar_bytes))
}

/// The `gmn-grammar` Merkle leaf (pack-root part 2): `blake3(authored gmn.ebnf bytes)` as
/// lowercase hex, UNPREFIXED. Pinned into the bundle as `gmeow:gmnGrammarDigest` so a
/// checkout-free consumer (the shipped `gmeow gmn verify`) recomputes the pack root from the
/// bundle alone, never needing the raw authored grammar file from a source checkout.
#[must_use]
pub fn grammar_leaf(grammar_bytes: &[u8]) -> String {
    blake3::hash(grammar_bytes).to_hex().to_string()
}

/// [`pack_root`] over a PRECOMPUTED [`grammar_leaf`] rather than the raw grammar bytes — the leg a
/// bundle-only consumer takes, reading the leaf from `gmeow:gmnGrammarDigest`. Folds the SAME three
/// ordered parts as [`pack_root`], so both legs agree byte-for-byte.
#[must_use]
pub fn pack_root_from_grammar_leaf(
    codebook_digest: &str,
    dict: &GmnDictionary,
    grammar_leaf: &str,
) -> String {
    let glyphs = dict.glyph_registry();
    let five = |(a, b, c, d, e): (String, String, String, String, String)| {
        format!("{a}{FIELD_SEP}{b}{FIELD_SEP}{c}{FIELD_SEP}{d}{FIELD_SEP}{e}")
    };
    let mut sigil_rows: Vec<String> = glyphs.glyph_binding_rows().into_iter().map(five).collect();
    sigil_rows.extend(glyphs.fallback_binding_rows().into_iter().map(five));
    let parts: [(&'static str, String); 3] = [
        ("codebook-digest", codebook_digest.to_owned()),
        ("gmn-grammar", grammar_leaf.to_owned()),
        ("sigil-table", leaf_hex(sigil_rows)),
    ];
    let mut preimage = String::new();
    for (label, leaf) in &parts {
        preimage.push_str(label);
        preimage.push(FIELD_SEP);
        preimage.push_str(leaf);
        preimage.push('\n');
    }
    format!(
        "{ALGO_PREFIX}{}",
        blake3::hash(preimage.as_bytes()).to_hex()
    )
}

/// The byte-exact identity of a GMN-0 model: blake3 over its RDFC-1.0 canonical N-Quads
/// ([`Gmn0Model::canonical_nquads`], already NFC per the RDFC-1.0 pipeline). This is the
/// envelope's `gmeow:contentDigest` domain.
///
/// Two DIFFERENT GMN-1 surface encodings of one model (record form and tabular form) read
/// back to the SAME [`Gmn0Model`] and therefore share this digest: it names the model, not
/// the surface bytes. Returns `"blake3:<64-hex>"`.
#[must_use]
pub fn content_digest(model: &Gmn0Model) -> String {
    let canonical = model.canonical_nquads();
    format!(
        "{ALGO_PREFIX}{}",
        blake3::hash(canonical.as_bytes()).to_hex()
    )
}

/// The labeled per-part Merkle leaves of [`codebook_digest`], in the fixed part order — the
/// ONE construction the root folds over and a mismatch diagnostic names the divergent leaf
/// from. Crate-internal by design (no shipped per-part API); the returned `leaf` values are
/// lowercase blake3 hex.
pub(crate) fn codebook_digest_leaves(
    codebook: &CurrentCodebook,
    dict: &GmnDictionary,
) -> Vec<(&'static str, String)> {
    let glyphs = dict.glyph_registry();
    let five = |(a, b, c, d, e): (String, String, String, String, String)| {
        format!("{a}{FIELD_SEP}{b}{FIELD_SEP}{c}{FIELD_SEP}{d}{FIELD_SEP}{e}")
    };
    let parts: [(&'static str, Vec<String>); 8] = [
        ("dialect-version", vec![dialect_version().to_owned()]),
        ("dictionary-version", vec![dict.version().to_owned()]),
        ("glyph-table-version", vec![glyphs.version().to_owned()]),
        (
            "codebook-references",
            codebook.references.iter().cloned().collect(),
        ),
        (
            "script-graphemes",
            codebook.graphemes.iter().cloned().collect(),
        ),
        (
            "dictionary-aliases",
            dict.alias_entries()
                .iter()
                .map(|(term, alias)| format!("{term}{FIELD_SEP}{alias}"))
                .collect(),
        ),
        (
            "glyph-table",
            glyphs.glyph_binding_rows().into_iter().map(five).collect(),
        ),
        (
            "glyph-fallback-table",
            glyphs
                .fallback_binding_rows()
                .into_iter()
                .map(five)
                .collect(),
        ),
    ];
    parts
        .into_iter()
        .map(|(label, lines)| (label, leaf_hex(lines)))
        .collect()
}

/// Fold one part's entry lines into its leaf hex: NFC-normalize each line, sort by `str`
/// `Ord` over the NFC forms, join with `\n`, and blake3 the bytes.
fn leaf_hex(lines: Vec<String>) -> String {
    let mut lines: Vec<String> = lines.into_iter().map(|l| l.nfc().collect()).collect();
    lines.sort();
    blake3::hash(lines.join("\n").as_bytes())
        .to_hex()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, parse_dataset};

    use super::*;
    use crate::gmn1_codec::{Gmn1Error, gmn1_read, gmn1_write, gmn1_write_tabular};

    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

    /// A minimal-but-valid current codebook: one dictionary entry (`term → alias`) and one
    /// script grapheme, at the codec's pinned versions (dictionary `3`, glyph-table `2`).
    /// With no denotations the glyph table loads empty — enough to exercise the digest's
    /// version, reference, grapheme, and alias leaves.
    fn codebook_fixture(alias: &str, grapheme_local: &str) -> Arc<RdfDataset> {
        let ttl = format!(
            r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references ex:dict, ex:script ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnGlyphTableVersion "2" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnDictionaryEntry ex:e1 .
ex:e1 gmeow:gmnDictionaryEntryTerm <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDictionaryEntryAlias "{alias}" .
ex:script a lang:Script ; lang:hasGrapheme ex:{grapheme_local} .
"#
        );
        parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("codebook fixture parses")
    }

    fn load(ds: &RdfDataset) -> (CurrentCodebook, GmnDictionary) {
        let codebook = resolve(ds);
        let dict = GmnDictionary::from_dataset(ds).expect("dictionary loads");
        (codebook, dict)
    }

    fn resolve(ds: &RdfDataset) -> CurrentCodebook {
        crate::gmn1_codec::resolve_current_codebook(ds).expect("codebook resolves")
    }

    /// A small `@c`-sigil gmeow-namespace model (uniform schema, so tabular form applies).
    fn gmeow_model(objects: &[&str]) -> Gmn0Model {
        let mut builder = RdfDatasetBuilder::new();
        let predicate = builder.intern_iri(&format!("{GMEOW_NS}relatesTo"));
        for (i, object) in objects.iter().enumerate() {
            let subject = builder.intern_iri(&format!("{GMEOW_NS}subject{i}"));
            let object = builder.intern_iri(&format!("{GMEOW_NS}{object}"));
            builder.push_quad(subject, predicate, object, None);
        }
        Gmn0Model::from_dataset(&builder.freeze().expect("model freezes"))
    }

    #[test]
    fn codebook_digest_is_deterministic_and_well_formed() {
        let ds = codebook_fixture("add", "g1");
        let (codebook, dict) = load(&ds);
        let first = codebook_digest(&codebook, &dict);
        let second = codebook_digest(&codebook, &dict);
        assert_eq!(first, second, "same codebook must digest identically");

        let hex = first
            .strip_prefix("blake3:")
            .expect("digest carries the blake3: algorithm tag");
        assert_eq!(hex.len(), 64, "blake3 hex is 32 bytes = 64 chars: {first}");
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "digest hex is lowercase: {first}"
        );
    }

    #[test]
    fn codebook_digest_is_sensitive_to_each_perturbed_part() {
        let base_ds = codebook_fixture("add", "g1");
        let (base_cb, base_dict) = load(&base_ds);
        let base = codebook_digest(&base_cb, &base_dict);

        // Perturb ONE alias-bijection entry.
        let alias_ds = codebook_fixture("plus", "g1");
        let (alias_cb, alias_dict) = load(&alias_ds);
        let perturbed_alias = codebook_digest(&alias_cb, &alias_dict);
        assert_ne!(
            base, perturbed_alias,
            "a changed dictionary alias must change the Merkle root"
        );

        // Perturb ONE script grapheme.
        let grapheme_ds = codebook_fixture("add", "g2");
        let (grapheme_cb, grapheme_dict) = load(&grapheme_ds);
        let perturbed_grapheme = codebook_digest(&grapheme_cb, &grapheme_dict);
        assert_ne!(
            base, perturbed_grapheme,
            "a changed script grapheme must change the Merkle root"
        );

        // The divergent leaf is nameable: exactly the perturbed part's leaf differs.
        let base_leaves = codebook_digest_leaves(&base_cb, &base_dict);
        let alias_leaves = codebook_digest_leaves(&alias_cb, &alias_dict);
        let differing: Vec<&str> = base_leaves
            .iter()
            .zip(&alias_leaves)
            .filter(|((_, a), (_, b))| a != b)
            .map(|((label, _), _)| *label)
            .collect();
        assert_eq!(
            differing,
            vec!["dictionary-aliases"],
            "only the dictionary-aliases leaf diverges when just the alias changes"
        );
    }

    #[test]
    fn content_digest_names_the_model_not_the_surface() {
        let ds = codebook_fixture("add", "g1");
        let (_codebook, dict) = load(&ds);
        let model = gmeow_model(&["objectA", "objectB"]);

        // Two DIFFERENT surface encodings of one model.
        let record_doc = gmn1_write(&model, &dict).expect("record-form write");
        let tabular_doc = gmn1_write_tabular(&model, &dict).expect("tabular-form write");
        assert_ne!(
            record_doc.text, tabular_doc.text,
            "the two surfaces must genuinely differ in bytes"
        );

        let from_record = gmn1_read(&record_doc, &dict).expect("record-form read");
        let from_tabular = gmn1_read(&tabular_doc, &dict).expect("tabular-form read");

        // Both surfaces canonicalize to the same model → one content digest.
        assert_eq!(
            content_digest(&from_record),
            content_digest(&from_tabular),
            "two surface encodings of one model share a content digest"
        );
        assert_eq!(content_digest(&from_record), content_digest(&model));

        // A different model yields a different content digest.
        let other = gmeow_model(&["objectA", "objectC"]);
        assert_ne!(
            content_digest(&model),
            content_digest(&other),
            "a different model must take a different content digest"
        );

        // Well-formed algorithm tag + 64 lowercase hex.
        let digest = content_digest(&model);
        let hex = digest.strip_prefix("blake3:").expect("blake3: tag");
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }

    #[test]
    fn non_nfc_literal_hard_fails_the_writer() {
        let ds = codebook_fixture("add", "g1");
        let (_codebook, dict) = load(&ds);

        // "e" + U+0301 COMBINING ACUTE ACCENT is NFD, not NFC (NFC is U+00E9 "é").
        let non_nfc = "e\u{0301}";
        let model = Gmn0Model {
            quads: vec![RdfQuad {
                subject: RdfTerm::Iri(format!("{GMEOW_NS}subject0")),
                predicate: format!("{GMEOW_NS}label"),
                object: RdfTerm::Literal(RdfLiteral::typed(
                    non_nfc,
                    "http://www.w3.org/2001/XMLSchema#string",
                )),
                graph_name: None,
                location: None,
            }],
        };

        let error = gmn1_write(&model, &dict).expect_err("a non-NFC literal must hard-fail");
        assert_eq!(
            error,
            Gmn1Error::NonNfcLiteral {
                lexical: non_nfc.to_owned(),
            }
        );
        assert_eq!(
            error.failure_class(),
            Gmn1Error::CLASS_NON_CANONICAL_CODEPOINT
        );
        // The tabular writer applies the SAME gate.
        assert!(matches!(
            gmn1_write_tabular(&model, &dict),
            Err(Gmn1Error::NonNfcLiteral { .. })
        ));
    }
}
