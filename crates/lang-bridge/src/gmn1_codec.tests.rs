// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;

fn empty_dict() -> GmnDictionary {
    GmnDictionary::default()
}

/// A synthetic GMN dialect lineage with a given latest major + accept window, used to
/// prove the acceptance policy is READ OFF THE GRAPH (roleLatest → owl:versionInfo, the
/// set's gmnAcceptWindow) rather than from any Rust constant.
fn synthetic_lineage_dataset(latest: u32, window: u32) -> Arc<RdfDataset> {
    let ttl = format!(
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
gmeow:gmnDialectVersions a gmeow:VersionSet ; gmeow:gmnAcceptWindow {window} .
gmeow:vLatest a gmeow:InformationObject ; owl:versionInfo "{latest}" .
gmeow:mLatest a gmeow:VersionMembership ;
    gmeow:versionMember gmeow:vLatest ;
    gmeow:versionSet gmeow:gmnDialectVersions ;
    gmeow:versionRole gmeow:roleLatest .
"#
    );
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("synthetic lineage parses")
}

#[test]
fn dialect_acceptance_follows_an_explicit_synthetic_lineage() {
    let synth = resolve_dialect_acceptance(&synthetic_lineage_dataset(5, 2))
        .expect("resolve synthetic acceptance")
        .expect("synthetic lineage present");
    assert_eq!((synth.latest_major(), synth.accept_window()), (5, 2));
    assert!(synth.accepts(5) && synth.accepts(4) && synth.accepts(3));
    assert!(!synth.accepts(6) && !synth.accepts(2));
}

fn glyph_registry_fixture(rows: &str, version: &str) -> Arc<RdfDataset> {
    let ttl = format!(
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references ex:dict, ex:script, ex:mathRole, ex:logicRole ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnGlyphTableVersion "{version}" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ;
    lang:hasGrapheme ex:g, ex:g1, ex:g2, ex:gPlus, ex:gNot .
ex:mathRole gmeow:gmnSigilGlyph "@μ" .
ex:logicRole gmeow:gmnSigilGlyph "@ℒ" .
{rows}
"#
    );
    parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("glyph fixture parses")
}

#[test]
fn glyph_registry_is_graph_derived_scoped_and_longest_match_ordered() {
    let ds = glyph_registry_fixture(
        r#"
ex:g1 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+00AC U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ;
    lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ;
    gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "not" ;
    gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let registry = GmnGlyphRegistry::from_dataset(&ds).expect("registry loads");
    assert_eq!(
        registry.glyph_for(&format!("{MATH_NS}Addition"), "@μ"),
        Some("+")
    );
    assert_eq!(
        registry.term_for("+", "@μ"),
        Some(format!("{MATH_NS}Addition").as_str())
    );
    assert_eq!(
        registry.glyph_for(&format!("{MATH_NS}Addition"), "@ℒ"),
        None
    );
    assert_eq!(
        registry.glyph_for_signature(
            &format!("{MATH_NS}Addition"),
            "@μ",
            Some(&format!("{GMEOW_NS}gmnFixityInfix")),
            Some(2),
        ),
        Some("+")
    );
    assert_eq!(
        registry.glyph_for_signature(
            &format!("{MATH_NS}Addition"),
            "@μ",
            Some(&format!("{GMEOW_NS}gmnFixityPrefix")),
            Some(2),
        ),
        None,
        "wrong fixity must not resolve"
    );
    assert_eq!(
        registry.term_for_signature(
            "+",
            "@μ",
            Some(&format!("{GMEOW_NS}gmnFixityInfix")),
            Some(1),
        ),
        None,
        "wrong arity must not resolve"
    );
    assert_eq!(registry.glyph_tokens(), vec!["¬¬", "+"]);
    assert_eq!(
        registry.render_glyph_token_production(),
        "glyphToken ::= '¬¬' | '+'"
    );
    let grammar = registry
        .render_grammar(b"referenceToken ::= identifier | glyphToken\nglyphToken ::= 'stale'\n")
        .expect("the graph-derived production renders");
    assert_eq!(
        String::from_utf8(grammar).unwrap(),
        "referenceToken ::= identifier | glyphToken\nglyphToken ::= '¬¬' | '+'\n"
    );
}

#[test]
fn removing_a_denotation_removes_writer_reader_and_generated_grammar_binding() {
    let full = glyph_registry_fixture(
        r#"
ex:gPlus gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:fPlus gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:dPlus a lang:Denotation ; lang:denotedForm ex:fPlus ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:gPlus .
ex:cPlus a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dPlus ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:gNot gmeow:gmnCodepoints "U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:fNot gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:dNot a lang:Denotation ; lang:denotedForm ex:fNot ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:gNot .
ex:cNot a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dNot ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let pruned = glyph_registry_fixture(
        r#"
ex:gPlus gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:gNot gmeow:gmnCodepoints "U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:fNot gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:dNot a lang:Denotation ; lang:denotedForm ex:fNot ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:gNot .
ex:cNot a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dNot ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let full_dict = GmnDictionary::from_dataset(&full).expect("full dictionary loads");
    let pruned_dict = GmnDictionary::from_dataset(&pruned).expect("pruned dictionary loads");

    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri(&format!("{MATH_NS}Expression"));
    let predicate = builder.intern_iri(&format!("{MATH_NS}operator"));
    let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
    builder.push_quad(subject, predicate, addition, None);
    let model = Gmn0Model::from_dataset(&builder.freeze().expect("freeze"));

    let full_doc = gmn1_write(&model, &full_dict).expect("full writer uses glyph");
    assert!(full_doc.text.contains("o: +"), "{}", full_doc.text);
    let pruned_doc = gmn1_write(&model, &pruned_dict).expect("fallback writer remains total");
    assert!(
        pruned_doc.text.contains("o: math__Addition"),
        "{}",
        pruned_doc.text
    );
    assert!(
        matches!(
            gmn1_read(&full_doc, &pruned_dict),
            Err(Gmn1Error::Uncovered(_))
        ),
        "the pruned reader must reject the now-unknown scoped glyph"
    );

    let template = b"referenceToken ::= identifier | glyphToken\nglyphToken ::= 'stale'\n";
    let full_grammar = String::from_utf8(
        full_dict
            .glyph_registry()
            .render_grammar(template)
            .expect("full grammar"),
    )
    .unwrap();
    let pruned_grammar = String::from_utf8(
        pruned_dict
            .glyph_registry()
            .render_grammar(template)
            .expect("pruned grammar"),
    )
    .unwrap();
    assert!(full_grammar.contains("'+'"));
    assert!(!pruned_grammar.contains("'+'"));
}

#[test]
fn glyph_registry_rejects_scope_collision_and_uts39_confusable_pair() {
    let exact = glyph_registry_fixture(
        r#"
ex:g1 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/PositiveSign> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "positive" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&exact).expect_err("exact collision rejects");
    assert!(error.0.contains("collides in scope"));

    let confusable = glyph_registry_fixture(
        r#"
ex:g1 gmeow:gmnCodepoints "U+0041" ; gmeow:gmnSigilScope ex:mathRole .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/LatinA> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "latinA" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+0391" ; gmeow:gmnSigilScope ex:mathRole .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/GreekAlpha> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "greekAlpha" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&confusable)
        .expect_err("a UTS #39 skeleton collision rejects");
    assert!(error.0.contains("UTS #39-confusable"), "{}", error.0);
}

// ── The in-band repair resolver (`@patch`/`@retract` → effective store) ──────────

const EX: &str = "https://example.test/";

fn iri(local: &str) -> RdfTerm {
    RdfTerm::Iri(format!("{EX}{local}"))
}

fn lit(text: &str) -> RdfTerm {
    RdfTerm::Literal(RdfLiteral::typed(text, XSD_STRING))
}

fn quad(s: RdfTerm, p: &str, o: RdfTerm) -> RdfQuad {
    RdfQuad::new(s, format!("{EX}{p}"), o)
}

/// AC (a): a `@patch` over a stable target id restates ONLY the named field in the
/// effective store, leaves the target's other fields intact, drops the reified patch
/// record from the materialized view, and never mutates the append-only input.
#[test]
fn patch_over_stable_id_is_reflected_in_effective_store() {
    let r1 = iri("r1");
    let model = Gmn0Model {
        quads: {
            let mut quads = vec![
                quad(r1.clone(), "fieldA", lit("oldA")),
                quad(r1.clone(), "fieldB", lit("keepB")),
                // The reified @patch: its own identity, naming r1, restating fieldA.
                RdfQuad::new(
                    iri("patch1"),
                    RDF_TYPE,
                    RdfTerm::Iri(CLASS_GMN_PATCH.to_owned()),
                ),
                RdfQuad::new(iri("patch1"), PRED_GMN_REPAIR_ID, lit(&format!("{EX}r1"))),
                quad(iri("patch1"), "fieldA", lit("newA")),
            ];
            quads.sort_by_key(quad_sort_key);
            quads
        },
    };

    let effective = resolve_effective(&model).expect("patch materializes");

    // fieldA is RESTATED; fieldB (an unpatched field of the same record) survives.
    assert!(
        effective
            .quads
            .iter()
            .any(|q| q.subject == r1 && q.predicate.ends_with("fieldA") && q.object == lit("newA")),
        "effective r1.fieldA must be the restated value"
    );
    assert!(
        !effective.quads.iter().any(|q| q.object == lit("oldA")),
        "the pre-patch fieldA value must be gone from the effective store"
    );
    assert!(
        effective.quads.iter().any(|q| q.subject == r1
            && q.predicate.ends_with("fieldB")
            && q.object == lit("keepB")),
        "an unpatched field of r1 must survive"
    );
    // The reified patch record is meta — consumed, not present as base data.
    assert!(
        !effective
            .quads
            .iter()
            .any(|q| matches!(&q.object, RdfTerm::Iri(i) if i == CLASS_GMN_PATCH)),
        "the reified @patch record must not appear in the effective store"
    );

    // Append-only: the ORIGINAL model still carries r1's old value AND the patch record.
    assert!(
        model.quads.iter().any(|q| q.object == lit("oldA")),
        "the input model is append-only — the original r1.fieldA survives"
    );
    assert!(
        model
            .quads
            .iter()
            .any(|q| matches!(&q.object, RdfTerm::Iri(i) if i == CLASS_GMN_PATCH)),
        "the input model still carries the unchanged @patch record"
    );
}

/// AC (b): a `@retract` over a stable target id withdraws its whole target from the
/// effective store while leaving sibling records and the append-only input intact.
#[test]
fn retract_over_stable_id_removes_from_effective_store() {
    let r1 = iri("r1");
    let s1 = iri("s1");
    let model = Gmn0Model {
        quads: {
            let mut quads = vec![
                quad(r1.clone(), "fieldA", lit("valA")),
                quad(r1.clone(), "fieldB", lit("valB")),
                quad(s1.clone(), "fieldC", lit("valC")),
                RdfQuad::new(
                    iri("ret1"),
                    RDF_TYPE,
                    RdfTerm::Iri(CLASS_GMN_RETRACT.to_owned()),
                ),
                RdfQuad::new(iri("ret1"), PRED_GMN_REPAIR_ID, lit(&format!("{EX}r1"))),
            ];
            quads.sort_by_key(quad_sort_key);
            quads
        },
    };

    let effective = resolve_effective(&model).expect("retract materializes");

    assert!(
        !effective.quads.iter().any(|q| q.subject == r1),
        "the retracted record r1 must be gone from the effective store"
    );
    assert!(
        effective.quads.iter().any(|q| q.subject == s1),
        "an unrelated sibling record must survive the retraction"
    );
    // Append-only: the input still carries r1 and the retract record.
    assert!(
        model.quads.iter().any(|q| q.subject == r1),
        "the input model is append-only — the withdrawn record survives in it"
    );
    assert!(
        model
            .quads
            .iter()
            .any(|q| matches!(&q.object, RdfTerm::Iri(i) if i == CLASS_GMN_RETRACT)),
        "the input model still carries the unchanged @retract record"
    );
}

/// AC (c): a repair naming a target id absent from the base record set is a HARD FAIL
/// with a named failure class — never a silent drop — for both `@patch` and `@retract`.
#[test]
fn repair_naming_unknown_target_hard_fails() {
    for (class, sigil) in [
        (CLASS_GMN_PATCH, SIGIL_PATCH),
        (CLASS_GMN_RETRACT, SIGIL_RETRACT),
    ] {
        let model = Gmn0Model {
            quads: {
                let mut quads = vec![
                    quad(iri("r1"), "fieldA", lit("valA")),
                    RdfQuad::new(iri("repair1"), RDF_TYPE, RdfTerm::Iri(class.to_owned())),
                    RdfQuad::new(
                        iri("repair1"),
                        PRED_GMN_REPAIR_ID,
                        lit(&format!("{EX}does-not-exist")),
                    ),
                ];
                quads.sort_by_key(quad_sort_key);
                quads
            },
        };

        let error =
            resolve_effective(&model).expect_err("a repair naming an absent target must hard-fail");
        assert_eq!(
            error,
            Gmn1RepairError::DanglingRepairTarget {
                kind: sigil,
                target: format!("{EX}does-not-exist"),
            }
        );
        assert_eq!(
            error.failure_class(),
            Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR
        );
    }
}

/// The resolver is a no-op fixed point on a model with no repair records — the
/// effective store is canonically equal to the input.
#[test]
fn effective_store_is_identity_on_a_repair_free_model() {
    let model = Gmn0Model {
        quads: {
            let mut quads = vec![
                quad(iri("r1"), "fieldA", lit("valA")),
                quad(iri("s1"), "fieldB", lit("valB")),
            ];
            quads.sort_by_key(quad_sort_key);
            quads
        },
    };
    let effective = resolve_effective(&model).expect("repair-free model resolves");
    assert!(gmn0_canonically_equal(&model, &effective));
}

#[test]
fn glyph_registry_rejects_bare_token_signature_ambiguity() {
    let ambiguous = glyph_registry_fixture(
        r#"
ex:g1 gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Negation> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "neg" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:mathRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Subtraction> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "sub" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&ambiguous)
        .expect_err("bare scoped minus cannot choose between two signatures");
    assert!(
        error.0.contains("bare GMN token would be ambiguous"),
        "{}",
        error.0
    );
}

#[test]
fn glyph_registry_rejects_noncanonical_unicode_and_version_drift() {
    for (codepoints, needle) in [
        ("U+03c0", "canonical uppercase"),
        ("U+0065 U+0301", "not NFC-normalized"),
        ("U+2066 U+00AC", "bidi/default-ignorable"),
    ] {
        let rows = format!(
            r#"ex:g gmeow:gmnCodepoints "{codepoints}" ; gmeow:gmnSigilScope ex:logicRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d a lang:Denotation ; lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph ."#
        );
        let ds = glyph_registry_fixture(&rows, GLYPH_VERSION);
        let error = GmnGlyphRegistry::from_dataset(&ds).expect_err("invalid glyph rejects");
        assert!(
            error.0.contains(needle),
            "expected {needle:?} in {}",
            error.0
        );
    }

    let ds = glyph_registry_fixture("", "1");
    let error = GmnGlyphRegistry::from_dataset(&ds).expect_err("version drift rejects");
    assert!(error.0.contains("does not match codec version"));
}

#[test]
fn glyph_registry_rejects_scope_outside_the_closed_sigil_set() {
    let unsupported_scope = glyph_registry_fixture(
        r#"
gmeow:gmnCodebookCurrent gmeow:references ex:deadRole .
ex:script lang:hasGrapheme ex:gDead .
ex:deadRole gmeow:gmnSigilGlyph "@x" .
ex:gDead gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:deadRole .
ex:fDead gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:dDead a lang:Denotation ; lang:denotedForm ex:fDead ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:gDead .
ex:cDead a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:dDead ; gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&unsupported_scope)
        .expect_err("a role outside the reader/writer's closed sigil set must reject");
    assert!(
        error.0.contains("unsupported GMN sigil \"@x\""),
        "{}",
        error.0
    );
}

#[test]
fn glyph_registry_requires_typed_denotation_and_complete_operator_signature() {
    let untyped = glyph_registry_fixture(
        r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&untyped)
        .expect_err("an untyped denotation must not enter executable resolution");
    assert!(error.0.contains("not typed lang:Denotation"), "{}", error.0);

    let incomplete_signature = glyph_registry_fixture(
        r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityInfix .
ex:d a lang:Denotation ; lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let error = GmnGlyphRegistry::from_dataset(&incomplete_signature)
        .expect_err("fixity without arity must not create a partial executable key");
    assert!(
        error
            .0
            .contains("must author gmnFixity and gmnArity together"),
        "{}",
        error.0
    );
}

#[test]
fn current_codebook_versions_are_required_and_unrelated_history_is_ignored() {
    let missing_glyph_version = parse_dataset(
            br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ; gmeow:references ex:dict, ex:script ; gmeow:gmnDictionaryVersion "3" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ; lang:hasGrapheme ex:g .
"#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
    let error = GmnDictionary::from_dataset(&missing_glyph_version)
        .expect_err("a missing current glyph version must never default");
    assert!(
        error
            .0
            .contains("gmnGlyphTableVersion must be declared exactly once"),
        "{}",
        error.0
    );

    let missing_dictionary_version = parse_dataset(
            br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ; gmeow:references ex:dict, ex:script ; gmeow:gmnGlyphTableVersion "2" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ; lang:hasGrapheme ex:g .
"#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
    let error = GmnDictionary::from_dataset(&missing_dictionary_version)
        .expect_err("a missing current dictionary version must never default");
    assert!(
        error
            .0
            .contains("gmnDictionaryVersion must be declared exactly once"),
        "{}",
        error.0
    );

    let with_history = glyph_registry_fixture(
        r#"
ex:oldCodebook a gmeow:GmnCodebook ; gmeow:references ex:oldDict, ex:oldScript, ex:oldRole ; gmeow:gmnDictionaryVersion "1" ; gmeow:gmnGlyphTableVersion "1" .
ex:oldDict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "1" ; gmeow:gmnDictionaryEntry ex:oldEntry .
ex:oldEntry gmeow:gmnDictionaryEntryTerm <https://blackcatinformatics.ca/math/Obsolete> ; gmeow:gmnDictionaryEntryAlias "obsolete" .
ex:oldScript a lang:Script ; lang:hasGrapheme ex:oldGlyph .
ex:oldRole gmeow:gmnSigilGlyph "@μ" .
ex:oldGlyph gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:oldRole .
ex:oldForm gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:oldDenotation a lang:Denotation ; lang:denotedForm ex:oldForm ; lang:denotationTarget <https://blackcatinformatics.ca/math/Obsolete> ; gmeow:gmnDenotationGrapheme ex:oldGlyph .
ex:oldCandidate a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:oldDenotation ; gmeow:gmnAsciiFallback "obsoleteOp" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
        GLYPH_VERSION,
    );
    let dictionary = GmnDictionary::from_dataset(&with_history)
        .expect("unrelated historical versions do not contaminate current resolution");
    assert_eq!(dictionary.version(), DICTIONARY_VERSION);
    assert!(dictionary.term_for("obsolete").is_none());
    assert!(dictionary.glyph_registry().glyph_tokens().is_empty());
}

#[test]
fn unrelated_denotation_functionality_does_not_poison_current_registry() {
    let dataset = glyph_registry_fixture(
        r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:currentForm gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:currentDenotation a lang:Denotation ;
    lang:denotedForm ex:currentForm ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:g .
ex:currentCandidate a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:currentDenotation ;
    gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ;
    gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .

# Slice-quality carriers merge conformance fixtures with the module. A fixture
# denotation outside the current script may be deliberately non-functional and
# must not contaminate executable glyph-registry construction.
ex:fixtureDenotation a lang:Denotation ;
    lang:denotedForm ex:fixtureFormOne, ex:fixtureFormTwo .
"#,
        GLYPH_VERSION,
    );

    let registry = GmnGlyphRegistry::from_dataset(&dataset)
        .expect("unrelated fixture denotation is outside current glyph inventory");
    assert_eq!(
        registry.glyph_for(&format!("{MATH_NS}Addition"), "@μ"),
        Some("+")
    );
}

#[test]
fn identifier_shape_recognizes_grammar_productions() {
    assert!(is_identifier("gate1"));
    assert!(is_identifier("_leading"));
    assert!(!is_identifier(""));
    assert!(!is_identifier("1leading"));
    assert!(!is_identifier("has:colon"));
    assert!(!is_identifier("has space"));
}

#[test]
fn number_tokens_recognize_grammar_productions() {
    assert!(is_integer_token("42"));
    assert!(is_integer_token("-3"));
    assert!(!is_integer_token("3.14"));
    assert!(is_decimal_token("0.95"));
    assert!(is_decimal_token("-1.00"));
    assert!(!is_decimal_token("0.9"));
    assert!(!is_decimal_token("0.951"));
    assert!(!is_decimal_token("9.5e-1"));
}

#[test]
fn simple_triple_round_trips_with_empty_dictionary() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW_NS}gate1"));
    let p = b.intern_iri(&format!("{GMEOW_NS}hasState"));
    let o = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dict = empty_dict();
    round_trip_check(&model, &dict).expect("plain triple round-trips");
}

#[test]
fn iri_under_no_registered_namespace_rides_by_reference() {
    // G11: an IRI under no registered namespace no longer hard-fails as
    // `lang:GmnUncoveredTerm` — it rides LOSSLESSLY by reference as an `x_<hash>` token,
    // its full IRI carried out-of-band in the reference table. This mirrors the
    // literal-by-reference machinery and lets realistic external URLs round-trip.
    let external = "https://mastodon.social";
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW_NS}account1"));
    let p = b.intern_iri(&format!("{GMEOW_NS}accountServiceHomepage"));
    let o = b.intern_iri(external);
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dict = empty_dict();

    // It round-trips losslessly (no hard fail, canonical equality holds).
    round_trip_check(&model, &dict).expect("an external IRI rides by reference losslessly");

    // The surface text names the IRI via an `x_` token and NEVER inlines the raw URL.
    let doc = gmn1_write(&model, &dict).expect("write");
    assert!(
        doc.text.contains("x_"),
        "the surface must carry an x_ by-reference IRI token: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("mastodon.social"),
        "the raw external URL must never appear inline in the surface text: {}",
        doc.text
    );

    // The object classifies as the new IriExternalByReference category.
    let classes = classify_model(&model, &dict);
    assert!(
        classes.iter().any(|c| matches!(
            c,
            QuadCoverage::Covered {
                object: Gmn1ConstructCategory::IriExternalByReference,
                ..
            }
        )),
        "the external-IRI object must classify as IriExternalByReference: {classes:?}"
    );
}

#[test]
fn distinct_external_iris_mint_distinct_tokens() {
    // The collision assumption: two DIFFERENT external IRIs must mint two DIFFERENT
    // `x_<hash>` tokens (a digest collision would surface as a round-trip mismatch).
    let dict = empty_dict();
    let one = "https://mastodon.social";
    let two = "https://bsky.app";
    let mut refs = BTreeMap::new();
    let (tok_one, cat_one) = classify_iri(one, &dict, &ns_to_prefix_table(), &mut refs, "@c");
    let (tok_two, cat_two) = classify_iri(two, &dict, &ns_to_prefix_table(), &mut refs, "@c");
    assert_eq!(cat_one, Gmn1ConstructCategory::IriExternalByReference);
    assert_eq!(cat_two, Gmn1ConstructCategory::IriExternalByReference);
    assert_ne!(
        tok_one, tok_two,
        "distinct external IRIs must mint distinct x_ tokens"
    );
    // Both payloads coexist in the one out-of-band table, each resolving to its IRI.
    assert_eq!(refs.get(&tok_one), Some(&RefPayload::Iri(one.to_owned())));
    assert_eq!(refs.get(&tok_two), Some(&RefPayload::Iri(two.to_owned())));
    // Interning the same IRI again is first-wins (one shared entry, stable token).
    let (tok_one_again, _) = classify_iri(one, &dict, &ns_to_prefix_table(), &mut refs, "@c");
    assert_eq!(tok_one, tok_one_again);
    assert_eq!(
        refs.len(),
        2,
        "first-wins interning shares one entry per IRI"
    );
}

#[test]
fn boundary_predicate_uses_the_logic_namespace() {
    // Sanity: the constant matches the real logic: namespace, not a typo.
    assert!(PRED_OCCURRENT_BOUNDARY.starts_with("https://blackcatinformatics.ca/logic/"));
}

#[test]
fn coverage_report_fraction_is_vacuously_full_on_empty_model() {
    let report = CoverageReport {
        covered: 0,
        total: 0,
    };
    assert_eq!(report.fraction(), 1.0, "nothing to cover is vacuously 1.0");
}

#[test]
fn measure_coverage_is_full_over_a_covered_model() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW_NS}gate1"));
    let p = b.intern_iri(&format!("{GMEOW_NS}hasState"));
    let o = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dict = empty_dict();
    let report = measure_coverage(&model, &dict);
    assert_eq!(report.total, 1);
    assert_eq!(report.covered, 1);
    assert_eq!(report.fraction(), 1.0);
}

#[test]
fn measure_coverage_counts_an_uncovered_quad_without_hard_failing() {
    // A first, fully covered quad (registered-namespace IRIs).
    let covered = RdfQuad::new(
        RdfTerm::Iri(format!("{GMEOW_NS}gate1")),
        format!("{GMEOW_NS}hasState"),
        RdfTerm::Iri(format!("{GMEOW_NS}doorGate1")),
    );
    // A second, deliberately uncovered quad: a blank-node subject whose label carries
    // the `__` separator, so `is_safe_token_body` rejects it and the blank arm raises
    // UncoveredTerm. (An external-namespace IRI is NO LONGER uncovered — it now rides
    // by reference — so the still-uncovered witness must be an unsafe blank label.)
    let uncovered = RdfQuad::new(
        RdfTerm::BlankNode("a__b".to_owned()),
        format!("{GMEOW_NS}hasState"),
        RdfTerm::Iri(format!("{GMEOW_NS}doorGate1")),
    );
    let model = Gmn0Model {
        quads: vec![covered, uncovered],
    };
    let dict = empty_dict();
    let report = measure_coverage(&model, &dict);
    assert_eq!(report.total, 2, "both quads are measured");
    assert_eq!(
        report.covered, 1,
        "only the registered-namespace quad is covered"
    );
    assert!((report.fraction() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn parse_header_tolerates_trailing_cr_and_incidental_whitespace() {
    let dict = empty_dict();
    // GMN-1 is an LLM-first interchange dialect: the reader must parse text an
    // external author/tool emits, not only gmn1_write's own canonical whitespace.
    // A trailing '\r' can survive std::str::lines()'s built-in CRLF handling (e.g.
    // a doubled '\r\r\n', or a lone trailing '\r' with no following '\n') — before
    // the fix, parse_header matched the un-trimmed line exactly against
    // "@gmn{...}" and hard-failed as Malformed on any such residue.
    assert!(parse_header("@gmn{v: 1, aliases: dict-v3, glyphs: 2}\r", &dict).is_ok());
    assert!(parse_header("  @gmn{v: 1, aliases: dict-v3, glyphs: 2}  ", &dict).is_ok());
    assert!(parse_header("@gmn{v: 1, aliases: dict-v3, glyphs: 2}", &dict).is_ok());
}

#[test]
fn gmn1_read_parses_externally_authored_whitespace_variants() {
    // Hand-written text simulating an externally-authored GMN-1 document —
    // deliberately NOT produced via this crate's own `gmn1_write` (which always
    // emits canonical single-space, LF-only whitespace) — so this proves the
    // reader tolerates real external variance, not merely its own writer's output.
    let dict = empty_dict();
    let text = concat!(
        // A doubled '\r' before the line feed: std::str::lines() strips exactly
        // one trailing '\r' per line, so one '\r' still reaches parse_header
        // un-trimmed without the fix.
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\r\r\n",
        // A stray space between the sigil and its opening brace.
        "@c {s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n",
        "@claims[s p o]\n",
        // A tab-delimited tabular row (mixed with normal spacing elsewhere).
        "gmeow__gate2\tgmeow__hasState\tgmeow__doorGate2\n",
    );
    let doc = Gmn1Document {
        text: text.to_owned(),
        refs: BTreeMap::new(),
    };
    let model = gmn1_read(&doc, &dict)
        .expect("CRLF header, spaced sigil, and tab-delimited row must all parse");
    assert_eq!(
        model.quads.len(),
        2,
        "one @c record plus one tabular row decode to two quads"
    );
}

#[test]
fn gmn1_read_unknown_sigil_hard_fails_as_non_decodable_grammar_after_trim() {
    // Negative control: trimming whitespace must never broaden the grammar. An unknown
    // sigil is not a dictionary-coverage gap (that is a grammar-VALID token the alias
    // table does not mint) — it is a structural defect the parse table has no production
    // for, so it is `lang:GmnNonDecodableGrammar`, never silently parsed.
    let dict = empty_dict();
    let text = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@x{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n";
    let doc = Gmn1Document {
        text: text.to_owned(),
        refs: BTreeMap::new(),
    };
    let err = gmn1_read(&doc, &dict).expect_err("an unknown sigil must still hard-fail");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
    assert!(matches!(err, Gmn1Error::NonDecodableGrammar { .. }));
}

#[test]
fn failure_class_returns_the_exact_lang_iri_for_every_variant() {
    assert_eq!(
        Gmn1Error::Uncovered(UncoveredTerm("x".to_owned())).failure_class(),
        "https://blackcatinformatics.ca/lang/GmnUncoveredTerm"
    );
    assert_eq!(
        Gmn1Error::NonCanonicalOrder {
            detail: "x".to_owned()
        }
        .failure_class(),
        "https://blackcatinformatics.ca/lang/GmnNonCanonicalOrder"
    );
    assert_eq!(
        Gmn1Error::MalformedNumber {
            token: "0.951".to_owned()
        }
        .failure_class(),
        "https://blackcatinformatics.ca/lang/GmnMalformedNumber"
    );
    assert_eq!(
        Gmn1Error::UndeclaredDialectVersion {
            detail: "x".to_owned()
        }
        .failure_class(),
        "https://blackcatinformatics.ca/lang/GmnUndeclaredDialectVersion"
    );
    assert_eq!(
        Gmn1Error::NonDecodableGrammar {
            detail: "x".to_owned()
        }
        .failure_class(),
        "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar"
    );
    // A per-claim mismatch REUSES the whole-model round-trip class, never a new one.
    assert_eq!(
        Gmn1Error::PerClaimMismatch {
            subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned()
        }
        .failure_class(),
        "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar"
    );
}

#[test]
fn number_shape_predicate_separates_numbers_from_identifiers() {
    // Number-shaped tokens (a malformed one is GmnMalformedNumber):
    assert!(is_number_shaped("9.5e-1"));
    assert!(is_number_shaped("0.951"));
    assert!(is_number_shaped("0.95"));
    assert!(is_number_shaped("50"));
    assert!(is_number_shaped("-1.00"));
    assert!(is_number_shaped(".5"));
    // Identifier-shaped tokens containing `e`/`E` must NOT be number-shaped (the naive
    // "contains e" test is wrong) — they stay dictionary-coverage (Uncovered):
    assert!(!is_number_shaped("open"));
    assert!(!is_number_shaped("state"));
    assert!(!is_number_shaped("sensorCrew"));
    assert!(!is_number_shaped("gate1"));
    assert!(!is_number_shaped("e12"));

    // The malformed-number predicate: exactly the non-canonical number lexemes.
    assert!(is_malformed_number("9.5e-1"));
    assert!(is_malformed_number("0.951"));
    assert!(!is_malformed_number("0.95"));
    assert!(!is_malformed_number("50"));
    assert!(!is_malformed_number("-1.00"));
    assert!(!is_malformed_number("open"));
}

/// A value token drives the number-form classification end-to-end through the reader:
/// `9.5e-1` and `0.951` become `GmnMalformedNumber`, `0.95` decodes, and an identifier
/// not in the dictionary stays `GmnUncoveredTerm`.
#[test]
fn reader_classifies_malformed_numbers_and_keeps_identifiers_uncovered() {
    let dict = empty_dict();
    let read = |q: &str| {
        let text = format!(
            "@gmn{{v: 1, aliases: dict-v3, glyphs: 2}}\n@c{{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: {q}}}\n"
        );
        gmn1_read(
            &Gmn1Document {
                text,
                refs: BTreeMap::new(),
            },
            &dict,
        )
    };
    assert_eq!(
        read("9.5e-1")
            .expect_err("scientific notation")
            .failure_class(),
        Gmn1Error::CLASS_MALFORMED_NUMBER
    );
    assert_eq!(
        read("0.951")
            .expect_err("three-digit fraction")
            .failure_class(),
        Gmn1Error::CLASS_MALFORMED_NUMBER
    );
    read("0.95").expect("a canonical two-digit confidence decodes");
    read("50").expect("a canonical integer decodes");

    // A grammar-valid identifier in an object slot the empty dictionary does not cover
    // stays Uncovered (dictionary-coverage), NOT MalformedNumber.
    let text = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: unregistered, p: unregistered, o: unregistered}\n";
    let err = gmn1_read(
        &Gmn1Document {
            text: text.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("an uncovered identifier must still hard-fail as Uncovered");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_UNCOVERED_TERM);
}

/// The detection-precedence linearization: an input that violates ≥2 classes resolves to
/// exactly the higher-precedence one. Here a document with NO `@gmn` header (header-
/// presence) AND a three-fractional-digit confidence (number-form) resolves to
/// `GmnMalformedNumber`, because number-form precedes header-presence — number
/// well-formedness is lexical, decidable without the dialect version.
#[test]
fn detection_precedence_number_form_wins_over_missing_header() {
    let dict = empty_dict();
    let text = concat!(
        "@c{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: 9.5e-1}\n",
        "@c{s: gmeow__gate2, p: gmeow__hasState, o: gmeow__doorGate2, q: 0.951}\n",
    );
    let err = gmn1_read(
        &Gmn1Document {
            text: text.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("both no-header and a malformed number are violated");
    assert_eq!(
        err.failure_class(),
        Gmn1Error::CLASS_MALFORMED_NUMBER,
        "number-form (pass 2) must win over header-presence (pass 4)"
    );

    // Control: the SAME headerless document with only canonical numbers resolves to the
    // lower-precedence header-presence class, proving the precedence is real (not that
    // MalformedNumber always wins).
    let ok_numbers = "@c{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: 0.95}\n";
    let err = gmn1_read(
        &Gmn1Document {
            text: ok_numbers.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("a headerless document still fails");
    assert_eq!(
        err.failure_class(),
        Gmn1Error::CLASS_UNDECLARED_DIALECT_VERSION
    );
}

/// Grammar (pass 1) dominates key-order (pass 3): a record with BOTH a non-canonical key
/// order AND a duplicate key resolves to `GmnNonDecodableGrammar`; a clean-grammar
/// misordered record resolves to `GmnNonCanonicalOrder`.
#[test]
fn detection_precedence_grammar_wins_over_key_order() {
    let dict = empty_dict();
    // Non-canonical order (q before s) is the sole defect → NonCanonicalOrder.
    let misordered = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{q: 0.95, s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n";
    let err = gmn1_read(
        &Gmn1Document {
            text: misordered.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("q before s is non-canonical");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_CANONICAL_ORDER);

    // A duplicate key is a grammar defect that dominates the misorder.
    let duplicate = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: gmeow__gate1, s: gmeow__gate2, p: gmeow__hasState, o: gmeow__doorGate1}\n";
    let err = gmn1_read(
        &Gmn1Document {
            text: duplicate.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("a duplicate key is non-decodable grammar");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
}

/// A tabular `@claims[...]` schema that repeats a column (e.g. `o` twice) must
/// hard-fail as `GmnNonDecodableGrammar`, mirroring `lex_sigil_record`'s duplicate-key
/// guard. Without this guard, `lex_tabular_row` zips the repeated column against two
/// DIFFERENT row values, and pass-5 assembly's `.collect()` into a `BTreeMap` silently
/// keeps only the last one — a quad is dropped with no error.
#[test]
fn tabular_schema_with_duplicate_column_is_non_decodable_grammar() {
    let dict = empty_dict();
    let text = concat!(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n",
        "@claims[s p o o]\n",
        "gmeow__gate1 gmeow__hasState gmeow__doorGate1 gmeow__doorGate2\n",
    );
    let err = gmn1_read(
        &Gmn1Document {
            text: text.to_owned(),
            refs: BTreeMap::new(),
        },
        &dict,
    )
    .expect_err("a duplicate tabular column must not silently drop a quad");
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
    assert!(matches!(err, Gmn1Error::NonDecodableGrammar { .. }));
}
