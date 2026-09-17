// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn fixity_local_strips_the_individual_prefix() {
    assert_eq!(
        fixity_local("https://blackcatinformatics.ca/gmeow/gmnFixityInfix"),
        "infix"
    );
    assert_eq!(
        fixity_local("https://blackcatinformatics.ca/gmeow/gmnFixityPrefix"),
        "prefix"
    );
}

#[test]
fn operator_body_round_trips_through_its_parser() {
    let body = operator_body("⊑", "infix", "subClassOf", "rdfs:subClassOf");
    assert_eq!(
        parse_operator_body(&body),
        Some(("infix".to_string(), "subClassOf".to_string()))
    );
    assert_eq!(sigil_glyph_of("@c — claim sigil"), Some("@c".to_string()));
}

#[test]
fn first_sentence_keeps_a_verbatim_prefix() {
    assert_eq!(
        first_sentence("A GMN @err record: a typed report. And more."),
        "A GMN @err record: a typed report."
    );
    assert_eq!(
        first_sentence("No trailing period marker"),
        "No trailing period marker"
    );
}

/// The literal-preference order, tested as the rule it is rather than only through the
/// two graph reads that consume it.
#[test]
fn the_literal_preference_order_is_english_then_lexically_least() {
    // Anything beats nothing.
    assert!(outranks(None, (false, "z")));
    // English beats a non-English literal, whatever the lexical order says.
    assert!(outranks(Some((false, "a")), (true, "z")));
    assert!(!outranks(Some((true, "z")), (false, "a")));
    // Within one language tag, the LEXICALLY-LEAST form wins — deterministically, in
    // both directions.
    assert!(outranks(Some((true, "z")), (true, "a")));
    assert!(!outranks(Some((true, "a")), (true, "z")));
    assert!(outranks(Some((false, "z")), (false, "a")));
    // An equal candidate never displaces the incumbent (the read is order-stable).
    assert!(!outranks(Some((true, "a")), (true, "a")));
}

/// A minimal-but-VALID GMN carrier: a current codebook at the codec's pinned versions,
/// one dictionary entry, one adopted operator glyph binding, the record sigils, the
/// three repair cards, and the notation definition the primer leads with.
///
/// Synthetic on purpose. `build_primer` had no direct test at all: its only exercise was
/// `crates/docs/tests/gmn1_primer_teachability.rs`, which reads the materialized
/// `generated/dist/gmeow.gts` — so every one of its branches (the hard-fail on a
/// carrier with no notation definition, the budget refusal on an oversized head, the
/// greedy operator fill and its elision accounting) was unreachable without a full
/// regenerate, and the failure modes were untested outright.
fn carrier(extra: &str) -> std::sync::Arc<RdfDataset> {
    let ttl = format!(
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix ex: <https://example.test/> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references ex:dict, ex:script ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnGlyphTableVersion "2" .
ex:dict a gmeow:GmnDictionary ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnDictionaryEntry ex:e1 .
ex:e1 gmeow:gmnDictionaryEntryTerm gmeow:relatesTo ;
    gmeow:gmnDictionaryEntryAlias "relates" .

# One executable operator binding: grapheme → denotation → adopted candidate.
ex:script a lang:Script ; lang:hasGrapheme ex:gSub .
ex:gSub gmeow:gmnCodepoints "U+2291" .
ex:den a lang:Denotation ;
    lang:denotationTarget gmeow:subsumes ;
    lang:denotedForm ex:formSub ;
    gmeow:gmnDenotationGrapheme ex:gSub .
ex:formSub gmeow:gmnFixity gmeow:gmnFixityInfix ;
    gmeow:gmnArity 2 .
ex:cand a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:den ;
    gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph ;
    gmeow:gmnAsciiFallback "subsumes" ;
    gmeow:gmnArity 2 .
gmeow:subsumes rdfs:label "subsumes"@x-gmeow-english .

# The record sigils (primer group 0).
gmeow:gmnSigilClaim a gmeow:GmnSigilRole ;
    gmeow:gmnSigilGlyph "@c" ;
    rdfs:label "claim sigil"@x-gmeow-english .
gmeow:gmnSigilQuery a gmeow:GmnSigilRole ;
    gmeow:gmnSigilGlyph "@q" ;
    rdfs:label "query sigil"@x-gmeow-english .

# The repair loop (primer group 1).
gmeow:GmnErr skos:definition "A typed error report. Second sentence."@x-gmeow-english ;
    gmeow:howToUse "Emit one per rejected record. And then stop."@x-gmeow-english .
gmeow:GmnPatch skos:definition "A minimal repair. Second sentence."@x-gmeow-english .
gmeow:GmnRetract skos:definition "A withdrawal. Second sentence."@x-gmeow-english .

{extra}
"#
    );
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("the synthetic GMN carrier parses")
}

/// The notation definition the primer leads with — split out so the hard-fail test can
/// build the SAME carrier without it.
const NOTATION: &str = r#"gmeow:gmnModelNotation skos:definition
    "GMN-1 is a compact record notation. It has more sentences."@x-gmeow-english ."#;

#[test]
fn build_primer_derives_every_row_group_from_the_carrier() {
    let ds = carrier(NOTATION);
    let primer = build_primer(&ds).expect("the synthetic carrier builds a primer");

    // The intro is the notation term's own FIRST SENTENCE, verbatim from the graph.
    assert_eq!(primer.intro, "GMN-1 is a compact record notation.");

    let sigils: Vec<&PrimerRow> = primer
        .rows
        .iter()
        .filter(|r| matches!(r.source, PrimerRowSource::Sigil { .. }))
        .collect();
    assert_eq!(
        sigils.iter().map(|r| r.body.as_str()).collect::<Vec<_>>(),
        ["@c — claim sigil", "@q — query sigil"],
        "every gmeow:GmnSigilRole individual becomes one glyph/label row, CURIE-sorted"
    );

    let repairs: Vec<&PrimerRow> = primer
        .rows
        .iter()
        .filter(|r| matches!(r.source, PrimerRowSource::Repair { .. }))
        .collect();
    assert_eq!(
        repairs.iter().map(|r| r.body.as_str()).collect::<Vec<_>>(),
        [
            "gmeow:GmnErr — A typed error report. Emit one per rejected record.",
            "gmeow:GmnPatch — A minimal repair.",
            "gmeow:GmnRetract — A withdrawal.",
        ],
        "each repair card is its first-sentence definition plus the first sentence of \
             its howToUse, when authored"
    );

    // The operator row is DERIVED from the glyph registry: glyph, fixity, and the
    // adopted candidate's ASCII fallback as the typable alias.
    let operators: Vec<&PrimerRow> = primer
        .rows
        .iter()
        .filter(|r| matches!(r.source, PrimerRowSource::Operator { .. }))
        .collect();
    assert_eq!(operators.len(), 1, "{operators:?}");
    assert_eq!(
        parse_operator_body(&operators[0].body),
        Some(("infix".to_string(), "subsumes".to_string())),
        "body was {:?}",
        operators[0].body
    );
    assert!(
        operators[0].body.starts_with('⊑'),
        "{:?}",
        operators[0].body
    );

    // Nothing elided at this scale, and the rendered card fits the shipped budget.
    assert_eq!((primer.elided_operators, primer.total_operators), (0, 1));
    assert!(primer.fits_budget(), "{} tokens", primer.token_count());

    // Deterministic: the same carrier builds the byte-identical card.
    let again = build_primer(&ds).expect("rebuild");
    let bodies = |p: &Gmn1Primer| -> Vec<String> {
        p.section()
            .bullets
            .iter()
            .map(|b| format!("{}{}: {}", b.text, b.signature, b.note))
            .collect()
    };
    assert_eq!(bodies(&primer), bodies(&again));
}

/// A carrier with no `gmeow:gmnModelNotation` definition HARD-FAILS. No-optionality: a
/// carrier that ships GMN must ship the primer's sources, and a silently-introless card
/// would be exactly the degradation the rule forbids.
#[test]
fn build_primer_hard_fails_on_a_carrier_with_no_notation_definition() {
    let ds = carrier("");
    let error = build_primer(&ds).expect_err("a carrier with no notation definition");
    assert!(
        error.0.contains("gmeow:gmnModelNotation"),
        "the refusal must name the missing term: {error}"
    );
}

/// A carrier with no resolvable codebook HARD-FAILS too, and names the codebook — the
/// other of the primer's two required sources.
#[test]
fn build_primer_hard_fails_on_a_carrier_with_no_codebook() {
    let ttl = format!(
        "@prefix gmeow: <{NAMESPACE}> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             gmeow:gmnModelNotation skos:definition \"A notation.\"@x-gmeow-english .\n"
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let error = build_primer(&ds).expect_err("a carrier with no GMN codebook");
    assert!(
        error.0.contains("codebook"),
        "the refusal must name the unresolvable codebook: {error}"
    );
}
