// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, parse_dataset};

use super::*;
use crate::gmn1_codec::{Gmn1Error, gmn1_read, gmn1_write, gmn1_write_tabular};

use gmeow_ns::GMEOW_NS;

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

/// The conformance-pack Merkle root is a PURE FUNCTION of every ecosystem surface —
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
