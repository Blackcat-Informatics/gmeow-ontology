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

/// The content-addressed Merkle leaves of the GMN **ecosystem** surfaces the conformance pack
/// certifies BEYOND the codec core (the codebook, grammar template, and sigil table). Each is
/// `blake3(view-artifact-bytes)` as lowercase hex, UNPREFIXED — the SAME content-addressing
/// [`view_leaf`]/[`grammar_leaf`] use — so perturbing ANY ecosystem surface's emitted bytes
/// changes the pack root and, via `gmeow gmn verify`, reds the bundle. Folded into
/// [`pack_root`] as parts 4–7 in this field order (gbnf, lark, token-metrics, verbalizations).
///
/// An ABSENT view (no artifact emitted — a blocking-construct grammar, or an empty corpus)
/// contributes `view_leaf(&[])`, a stable leaf, so the fold is total and deterministic and a
/// later deletion of a once-present view flips the leaf and reds the pack (tamper-evidence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcosystemLeaves {
    /// `blake3` of the emitted GBNF artifact bytes (`gmn1/v<major>/gbnf/gmn.gbnf`).
    pub gbnf: String,
    /// `blake3` of the emitted Lark artifact bytes (`gmn1/v<major>/lark/gmn.lark`).
    pub lark: String,
    /// `blake3` of the emitted token-metrics artifact bytes (`gmn1/v<major>/token-metrics.ttl`).
    pub token_metrics: String,
    /// `blake3` of the emitted verbalizations artifact bytes (`gmn1/v<major>/verbalizations.ttl`).
    pub verbalizations: String,
}

impl EcosystemLeaves {
    /// The four ecosystem leaves as `view_leaf(view-bytes)` over each surface's EMITTED artifact
    /// bytes — the SAME primitive the grammar leaf uses, so a bundle consumer recomputes each from
    /// the pinned digest. Empty view bytes yield the stable empty leaf.
    #[must_use]
    pub fn from_view_bytes(
        gbnf: &[u8],
        lark: &[u8],
        token_metrics: &[u8],
        verbalizations: &[u8],
    ) -> Self {
        Self {
            gbnf: view_leaf(gbnf),
            lark: view_leaf(lark),
            token_metrics: view_leaf(token_metrics),
            verbalizations: view_leaf(verbalizations),
        }
    }
}

/// The content-addressed **conformance-pack Merkle root** (`gmeow:gmnPackRoot`): the GMN-1
/// version identity an independent decoder pins against. A blake3 root over exactly SEVEN
/// ordered part leaves, folded with the SAME wire format as [`codebook_digest`] — each part
/// contributes `label␟leaf\n` (`␟` = [`FIELD_SEP`]) to the root pre-image IN THIS ORDER,
/// `root = blake3(pre-image)`, returned as `"blake3:<64-hex>"`:
///
/// | # | label                | leaf |
/// |---|----------------------|------|
/// | 1 | `codebook-digest`    | the codebook digest string VERBATIM (algorithm-tagged `blake3:<hex>`) — the pack pins the codebook by its PUBLISHED identity, so a consumer recomputes the root directly from the value the codebook already carries |
/// | 2 | `gmn-grammar`        | `blake3(authored gmn.ebnf bytes)`, lowercase hex — the authored grammar template byte-exact (the `glyphToken` seam is realized from leaf 3, so the template and the glyph table are pinned independently) |
/// | 3 | `sigil-table`        | the [`leaf_hex`] of the executable sigil→glyph binding rows (the glyph table then the fallback table), each row `sigil␟surface␟fixity␟arity␟term`, NFC-normalized + sorted — the SAME per-part canonicalization the codebook's glyph leaves use |
/// | 4 | `gmn-gbnf`           | [`EcosystemLeaves::gbnf`] — `blake3` of the emitted GBNF constrained-decode grammar artifact |
/// | 5 | `gmn-lark`           | [`EcosystemLeaves::lark`] — `blake3` of the emitted Lark constrained-parse grammar artifact |
/// | 6 | `gmn-token-metrics`  | [`EcosystemLeaves::token_metrics`] — `blake3` of the emitted token-metric measurement artifact |
/// | 7 | `gmn-verbalizations` | [`EcosystemLeaves::verbalizations`] — `blake3` of the emitted GMN⇄controlled-NL verbalization artifact |
///
/// Deterministic and input-only (no clock, rng, or environment). The pack is self-certifying:
/// a consumer recomputes this root from the seven parts the pack `gmeow:references` (each pinned
/// into the bundle as a Merkle leaf) and refuses a pack whose declared root disagrees — so the
/// pack certifies the WHOLE GMN ecosystem, tamper-evident, from the bundle alone.
#[must_use]
pub fn pack_root(
    codebook_digest: &str,
    dict: &GmnDictionary,
    grammar_bytes: &[u8],
    ecosystem: &EcosystemLeaves,
) -> String {
    pack_root_from_grammar_leaf(
        codebook_digest,
        dict,
        &grammar_leaf(grammar_bytes),
        ecosystem,
    )
}

/// The content-addressed Merkle leaf of ONE view artifact: `blake3(view-bytes)` as lowercase hex,
/// UNPREFIXED. The single primitive every pack leaf beyond the codebook/sigil tables uses, so a
/// checkout-free consumer (the shipped `gmeow gmn verify`) recomputes each leaf from the digest
/// the bundle pins, never needing the raw artifact from a source checkout.
#[must_use]
pub fn view_leaf(view_bytes: &[u8]) -> String {
    blake3::hash(view_bytes).to_hex().to_string()
}

/// The `gmn-grammar` Merkle leaf (pack-root part 2): [`view_leaf`] over the authored `gmn.ebnf`
/// bytes, pinned into the bundle as `gmeow:gmnGrammarDigest`.
#[must_use]
pub fn grammar_leaf(grammar_bytes: &[u8]) -> String {
    view_leaf(grammar_bytes)
}

/// [`pack_root`] over a PRECOMPUTED [`grammar_leaf`] rather than the raw grammar bytes — the leg a
/// bundle-only consumer takes, reading the leaf from `gmeow:gmnGrammarDigest`. Folds the SAME seven
/// ordered parts as [`pack_root`], so both legs agree byte-for-byte.
#[must_use]
pub fn pack_root_from_grammar_leaf(
    codebook_digest: &str,
    dict: &GmnDictionary,
    grammar_leaf: &str,
    ecosystem: &EcosystemLeaves,
) -> String {
    let glyphs = dict.glyph_registry();
    let five = |(a, b, c, d, e): (String, String, String, String, String)| {
        format!("{a}{FIELD_SEP}{b}{FIELD_SEP}{c}{FIELD_SEP}{d}{FIELD_SEP}{e}")
    };
    let mut sigil_rows: Vec<String> = glyphs.glyph_binding_rows().into_iter().map(five).collect();
    sigil_rows.extend(glyphs.fallback_binding_rows().into_iter().map(five));
    let parts: [(&'static str, String); 7] = [
        ("codebook-digest", codebook_digest.to_owned()),
        ("gmn-grammar", grammar_leaf.to_owned()),
        ("sigil-table", leaf_hex(sigil_rows)),
        ("gmn-gbnf", ecosystem.gbnf.clone()),
        ("gmn-lark", ecosystem.lark.clone()),
        ("gmn-token-metrics", ecosystem.token_metrics.clone()),
        ("gmn-verbalizations", ecosystem.verbalizations.clone()),
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

    /// Task 15: the conformance-pack Merkle root is a PURE FUNCTION of every ecosystem surface —
    /// the gbnf + lark grammar artifacts, the token-metrics measurement, and the verbalizations —
    /// beside the existing codebook / grammar / sigil coverage. Falsifiable PER SURFACE: perturbing
    /// exactly one view's bytes changes the root, so a view that was NOT folded would leave the root
    /// unchanged and RED this test. Also pins determinism (two computations agree).
    #[test]
    fn pack_root_covers_every_ecosystem_surface() {
        let ds = codebook_fixture("add", "g1");
        let (codebook, dict) = load(&ds);
        let digest = codebook_digest(&codebook, &dict);

        let grammar = b"root ::= glyphToken ;\n".as_slice();
        let gbnf = b"root ::= glyph-token\n".as_slice();
        let lark = b"start: glyph_token\n".as_slice();
        let metrics = b"<s> <p> \"7\" .\n".as_slice();
        let verbal = b"<u> <a> <b> .\n".as_slice();

        let base_leaves = EcosystemLeaves::from_view_bytes(gbnf, lark, metrics, verbal);
        let base = pack_root(&digest, &dict, grammar, &base_leaves);

        // Determinism: the root is input-only, so two computations agree byte-for-byte.
        assert_eq!(
            base,
            pack_root(&digest, &dict, grammar, &base_leaves),
            "pack_root must be a deterministic function of its inputs"
        );
        // Well-formed algorithm tag + 64 lowercase hex.
        let hex = base.strip_prefix("blake3:").expect("blake3: tag");
        assert_eq!(hex.len(), 64, "pack root is 32 bytes = 64 hex: {base}");
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "pack root hex is lowercase: {base}"
        );

        // ── existing coverage still holds: codebook + grammar are folded ──
        let (other_cb, other_dict) = load(&codebook_fixture("plus", "g1"));
        let other_digest = codebook_digest(&other_cb, &other_dict);
        assert_ne!(
            base,
            pack_root(&other_digest, &dict, grammar, &base_leaves),
            "a changed codebook digest must change the pack root"
        );
        assert_ne!(
            base,
            pack_root(&digest, &dict, b"root ::= other ;\n", &base_leaves),
            "a changed grammar template must change the pack root"
        );

        // ── new ecosystem coverage: EACH view is folded, falsifiable per surface ──
        let perturbations: [(&str, EcosystemLeaves); 4] = [
            (
                "gbnf",
                EcosystemLeaves::from_view_bytes(b"root ::= X\n", lark, metrics, verbal),
            ),
            (
                "lark",
                EcosystemLeaves::from_view_bytes(gbnf, b"start: X\n", metrics, verbal),
            ),
            (
                "token-metrics",
                EcosystemLeaves::from_view_bytes(gbnf, lark, b"<s> <p> \"8\" .\n", verbal),
            ),
            (
                "verbalizations",
                EcosystemLeaves::from_view_bytes(gbnf, lark, metrics, b"<u> <a> <c> .\n"),
            ),
        ];
        for (surface, perturbed) in &perturbations {
            assert_ne!(
                base,
                pack_root(&digest, &dict, grammar, perturbed),
                "perturbing the {surface} view bytes must change the pack root \
                 (if it does not, that surface is NOT folded into the root)"
            );
            // Exactly the perturbed surface's leaf differs — the divergence is nameable.
            let differing: Vec<&str> = [
                ("gbnf", &base_leaves.gbnf, &perturbed.gbnf),
                ("lark", &base_leaves.lark, &perturbed.lark),
                (
                    "token-metrics",
                    &base_leaves.token_metrics,
                    &perturbed.token_metrics,
                ),
                (
                    "verbalizations",
                    &base_leaves.verbalizations,
                    &perturbed.verbalizations,
                ),
            ]
            .into_iter()
            .filter(|(_, a, b)| a != b)
            .map(|(label, _, _)| label)
            .collect();
            assert_eq!(
                &differing,
                &[*surface],
                "only the {surface} leaf diverges when just that view changes"
            );
        }

        // The two legs (raw grammar bytes vs. precomputed grammar leaf) agree byte-for-byte.
        assert_eq!(
            base,
            pack_root_from_grammar_leaf(&digest, &dict, &grammar_leaf(grammar), &base_leaves),
            "the raw-bytes and grammar-leaf legs must fold to the same root"
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
