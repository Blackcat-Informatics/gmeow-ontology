// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The projection **registry** — the correspondence-carrying targets that lower the
//! canonical `lang:` model out to the external ecosystems (OntoLex-Lemon, CoNLL-U, EBNF,
//! ABNF). These tests prove the registry's structural invariants directly over the
//! targets: round-trip is MEASURED (never asserted), preservation is DERIVED from the
//! carried correspondence, and "Exact" is FALSIFIABLE — a perturbed object fails exactness.

use gmeow_lang_bridge::registry::{
    EMISSION_WORTHY_CLASSES, LangProjectionInput, LangProjectionTarget, NamedSource,
    assert_registry_covers, registry,
};
use gmeow_lang_bridge::{
    ConlluBridge, EbnfBridge, Formalism, LangFailure, exact_round_trip_holds,
    is_exact_correspondence, parse_grammar, serialize_grammar,
};
use gmeow_logic_compile::ir::PreservationKind;

const TURTLE_EBNF: &str = include_str!("../../../slices/grounding/lang/grammars/turtle.ebnf");
const CONLLU_FIXTURE: &[u8] = include_bytes!("fixtures/sample.conllu");

/// A minimal `lang:` lexical A-box — the forward OntoLex target's input.
const LANG_LEXICON: &str = "\
@prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex:   <http://example.org/lang/> .\n\
ex:lexCat a lang:Lexeme ; rdfs:label \"cat\" ; lang:partOfSpeech lang:noun .\n\
ex:senseCat a lang:Sense ; rdfs:label \"the animal sense of 'cat'\" ; lang:senseOf ex:lexCat .\n\
ex:wfCats a lang:WordForm ; rdfs:label \"cats\" ; lang:inflectionOf ex:lexCat ;\n\
    lang:morphFeature ex:featPlur .\n\
ex:featPlur a lang:MorphFeature ; lang:featureKey lang:featNumber ; lang:featureValue lang:valPlur .\n";

/// A `lang:` composed form scoped to TWO co-resident analyses — the forward CoNLL-U target's
/// input for the no-silent-winner discipline.
const LANG_AMBIGUOUS: &str = "\
@prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex:   <http://example.org/lang/> .\n\
ex:wSaw a lang:WordForm ; rdfs:label \"saw\" .\n\
ex:wDuck a lang:WordForm ; rdfs:label \"duck\" .\n\
ex:sent a lang:ComposedForm ; rdfs:label \"saw duck\" ;\n\
    lang:inAnalysis ex:aBird , ex:aCrouch ; lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .\n\
ex:aBird a lang:Analysis .\n\
ex:aCrouch a lang:Analysis .\n\
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .\n\
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .\n";

/// An ABNF-expressible grammar (no verbatim/negated character class, no `A - B` difference)
/// authored in EBNF notation — the fragment the ABNF target renders exactly.
const CF_EBNF: &str = "num ::= digit+\ndigit ::= '0' | '1' | '2'\n";

fn target(name: &str) -> Box<dyn LangProjectionTarget> {
    registry()
        .into_iter()
        .find(|t| t.name() == name)
        .unwrap_or_else(|| panic!("target '{name}' must be registered"))
}

// ── Invariant 4: registry completeness (functor totality) ──────────────────────────

#[test]
fn every_emission_worthy_class_maps_to_a_registered_target() {
    for (class, _) in EMISSION_WORTHY_CLASSES {
        assert_registry_covers(class).expect("registered class must be covered");
    }
    // An unlisted class is a hard fail, never a silent gap.
    assert!(assert_registry_covers("Nonexistent").is_err());
    // Every registered target name is one this project ships (the four grammar surfaces plus
    // the lexical/morphosyntax targets).
    let names: Vec<&str> = registry().iter().map(|t| t.name()).collect();
    for expected in ["ontolex-lemon", "conllu", "ebnf", "abnf", "gbnf", "lark"] {
        assert!(names.contains(&expected), "missing target {expected}");
    }
    // Functor totality for Grammar now demands ALL FOUR grammar surfaces — dropping GBNF or Lark
    // is a hard fail, not a silent loss of that constrained-decode surface.
    let (_, grammar_targets) = EMISSION_WORTHY_CLASSES
        .iter()
        .find(|(c, _)| *c == "Grammar")
        .expect("Grammar is emission-worthy");
    for surface in ["ebnf", "abnf", "gbnf", "lark"] {
        assert!(
            grammar_targets.contains(&surface),
            "Grammar coverage must demand the {surface} surface"
        );
    }
}

// ── EBNF: exact round-trip, DERIVED preservation ───────────────────────────────────

#[test]
fn ebnf_target_emits_exact_round_tripping_grammar() {
    let input = LangProjectionInput {
        grammars: vec![NamedSource {
            name: "turtle".to_owned(),
            bytes: TURTLE_EBNF.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let emissions = target("ebnf").emit(&input).expect("emit");
    assert_eq!(emissions.len(), 1);
    let e = &emissions[0];

    // Preservation is DERIVED, not declared: the carried correspondence is exact.
    assert!(
        is_exact_correspondence(&e.correspondence),
        "the grammar round-trip is an isomorphism with a discharged section law"
    );
    // Round-trip is MEASURED true, and the carried leg pair is the structural inverse.
    assert!(e.round_trip_holds, "the EBNF re-parse must be isomorphic");
    let (get, put) = e.leg_pair.as_ref().expect("grammar carries a leg pair");
    assert!(exact_round_trip_holds(get, put), "put ∘ get = id");

    // One EBNF artifact, and it re-parses to the same canonical grammar.
    let artifact = e
        .artifacts
        .iter()
        .find(|a| a.path_suffix.starts_with("ebnf/"))
        .expect("an EBNF artifact");
    let reparsed = parse_grammar(&artifact.bytes, Formalism::Ebnf).expect("re-parse");
    let source = EbnfBridge.to_grammar(TURTLE_EBNF.as_bytes()).unwrap();
    assert_eq!(reparsed.canonicalize(), source.canonicalize());
}

// ── ABNF: exact for the CF fragment, honest-lossy for EBNF-only constructs ──────────

#[test]
fn abnf_target_is_exact_for_the_cf_fragment() {
    let input = LangProjectionInput {
        grammars: vec![NamedSource {
            name: "num".to_owned(),
            bytes: CF_EBNF.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let emissions = target("abnf").emit(&input).expect("emit");
    let e = &emissions[0];
    assert!(
        is_exact_correspondence(&e.correspondence),
        "an ABNF-expressible grammar carries an exact correspondence"
    );
    assert!(e.round_trip_holds, "the ABNF re-parse must be isomorphic");
    let artifact = e
        .artifacts
        .iter()
        .find(|a| a.path_suffix.starts_with("abnf/"))
        .expect("an ABNF artifact");
    // The emitted ABNF uses the ABNF repetition prefix (`1*digit`), not the EBNF postfix.
    let text = std::str::from_utf8(&artifact.bytes).unwrap();
    assert!(text.contains("1*digit"), "ABNF prefix repetition: {text:?}");
}

#[test]
fn abnf_target_is_honest_lossy_for_char_class_grammars() {
    // The real Turtle grammar carries verbatim/negated character classes ABNF cannot hold.
    let input = LangProjectionInput {
        grammars: vec![NamedSource {
            name: "turtle".to_owned(),
            bytes: TURTLE_EBNF.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &target("abnf").emit(&input).expect("emit")[0];
    // NOT exact — the carried correspondence is lossy, and the driver derives SoundUnder.
    assert!(!is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    // No fabricated artifact — a partial ABNF that cannot round-trip is never emitted.
    assert!(
        e.artifacts.is_empty(),
        "a non-expressible grammar emits no ABNF artifact"
    );
    // The blocking constructs are enumerated (carried and flagged, never a silent skip).
    assert!(
        e.unsupported.iter().any(|u| u.contains("character class")),
        "the EBNF character classes must be enumerated unsupported"
    );
    assert!(!e.round_trip_holds);
}

// ── GBNF / Lark: version-keyed GMN glyph-grammar surfaces ───────────────────────────

/// A representable GMN glyph grammar (pure alternation of string-terminal glyphs, an entry rule
/// referencing it) — no hex, numeric-range, set-difference, bounded repetition, or left-recursion,
/// so it is faithful under BOTH the GBNF and Lark surfaces. Named `gmn` because the GBNF/Lark
/// targets are scoped to the graph-derived `gmn` glyph grammar alone.
const GMN_REPRESENTABLE_EBNF: &str = "gmnDoc ::= glyphToken\nglyphToken ::= '+' | 'π' | '¬'\n";
/// A GMN grammar whose `EOL` production carries the `#xD? #xA` hex terminals the real
/// `grammars/gmn.ebnf` uses — a hex codepoint terminal, which BOTH surfaces render as a
/// fixed-width char-class escape (`[\x0D]` / `/\x0D/`) and round-trip faithfully.
const GMN_HEX_EBNF: &str =
    "gmnDoc ::= glyphToken EOL\nglyphToken ::= '+' | 'π'\nEOL ::= #xD? #xA\n";
/// A GMN grammar carrying the set-difference operator `A - B` — the genuinely unrepresentable
/// construct (neither surface has a difference operator), so its projection is an honest
/// SoundUnder with NO artifact. This keeps the registry's SoundUnder path falsifiable now that
/// hex/range are representable.
const GMN_DIFF_EBNF: &str =
    "gmnDoc ::= glyphToken body\nglyphToken ::= '+' | 'π'\nbody ::= any - '\"'\nany ::= [a-z]\n";

#[test]
fn gbnf_and_lark_emit_versioned_exact_glyph_grammar_for_the_representable_fragment() {
    for surface in ["gbnf", "lark"] {
        let input = LangProjectionInput {
            grammars: vec![NamedSource {
                name: "gmn".to_owned(),
                bytes: GMN_REPRESENTABLE_EBNF.as_bytes().to_vec(),
            }],
            gmn_dialect_major: Some("1".to_owned()),
            ..Default::default()
        };
        let emissions = target(surface).emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1, "{surface}: one gmn glyph-grammar emission");
        let e = &emissions[0];
        // Representable ⇒ exact correspondence, measured round-trip, structural-inverse leg pair.
        assert!(
            is_exact_correspondence(&e.correspondence),
            "{surface}: a representable glyph grammar carries an exact correspondence"
        );
        assert!(e.round_trip_holds, "{surface}: the re-parse must be isomorphic");
        let (get, put) = e.leg_pair.as_ref().expect("leg pair");
        assert!(exact_round_trip_holds(get, put), "{surface}: put ∘ get = id");
        // The artifact is keyed UNDER the version subtree (gmn1/v<major>/<surface>/gmn.<surface>),
        // never the flat surface path the EBNF/ABNF targets use.
        let artifact = e.artifacts.first().expect("one artifact");
        assert_eq!(
            artifact.path_suffix,
            format!("gmn1/v1/{surface}/gmn.{surface}"),
            "{surface}: artifact lives under the version-keyed GMN subtree"
        );
        assert!(!artifact.is_rdf, "{surface}: a grammar surface is not RDF");
        // The emitted grammar CARRIES the graph-derived glyph production and every glyph.
        let text = std::str::from_utf8(&artifact.bytes).unwrap();
        assert!(
            text.contains("glyphToken"),
            "{surface}: the glyph production is present:\n{text}"
        );
        for glyph in ["+", "π", "¬"] {
            assert!(text.contains(glyph), "{surface}: missing {glyph:?} in:\n{text}");
        }
        // Re-parses to the SAME canonical tree as the EBNF source (identity is the tree).
        let formalism = if surface == "gbnf" {
            Formalism::Gbnf
        } else {
            Formalism::Lark
        };
        let reparsed = parse_grammar(&artifact.bytes, formalism).expect("re-parse");
        let source = EbnfBridge.to_grammar(GMN_REPRESENTABLE_EBNF.as_bytes()).unwrap();
        assert_eq!(reparsed.canonicalize().rules, source.canonicalize().rules);
    }
}

#[test]
fn gbnf_and_lark_emit_versioned_exact_artifact_for_the_hex_bearing_real_gmn_shape() {
    for surface in ["gbnf", "lark"] {
        let input = LangProjectionInput {
            grammars: vec![NamedSource {
                name: "gmn".to_owned(),
                bytes: GMN_HEX_EBNF.as_bytes().to_vec(),
            }],
            gmn_dialect_major: Some("1".to_owned()),
            ..Default::default()
        };
        let e = &target(surface).emit(&input).expect("emit")[0];
        // Representable ⇒ exact correspondence and a measured round-trip: the `#xD? #xA` hex line
        // renders as fixed-width char-class escapes that reparse to the SAME tree.
        assert!(
            is_exact_correspondence(&e.correspondence),
            "{surface}: the hex-bearing gmn grammar is representable and exact"
        );
        assert_eq!(e.lossy_kind, PreservationKind::Exact);
        assert!(e.round_trip_holds, "{surface}: the re-parse must be isomorphic");
        let artifact = e.artifacts.first().expect("one artifact");
        assert_eq!(
            artifact.path_suffix,
            format!("gmn1/v1/{surface}/gmn.{surface}"),
            "{surface}: the hex-bearing grammar ships a real versioned artifact"
        );
        // The hex escapes are present in BOTH directions (rendered, and re-parseable to `Hex`).
        let text = std::str::from_utf8(&artifact.bytes).unwrap();
        let cr_escape = if surface == "gbnf" { "[\\x0D]" } else { "/\\x0D/" };
        assert!(
            text.contains(cr_escape),
            "{surface}: the `#xD` terminal renders as {cr_escape}:\n{text}"
        );
        let formalism = if surface == "gbnf" {
            Formalism::Gbnf
        } else {
            Formalism::Lark
        };
        let reparsed = parse_grammar(&artifact.bytes, formalism).expect("re-parse");
        let source = EbnfBridge.to_grammar(GMN_HEX_EBNF.as_bytes()).unwrap();
        assert_eq!(
            reparsed.canonicalize().rules,
            source.canonicalize().rules,
            "{surface}: the hex-bearing artifact re-parses to the SAME canonical tree"
        );
    }
}

#[test]
fn gbnf_and_lark_are_honest_soundunder_for_a_difference_bearing_gmn_grammar() {
    for surface in ["gbnf", "lark"] {
        let input = LangProjectionInput {
            grammars: vec![NamedSource {
                name: "gmn".to_owned(),
                bytes: GMN_DIFF_EBNF.as_bytes().to_vec(),
            }],
            gmn_dialect_major: Some("1".to_owned()),
            ..Default::default()
        };
        let e = &target(surface).emit(&input).expect("emit")[0];
        // NOT exact — the set-difference operator has no round-trip-faithful GBNF/Lark node.
        assert!(!is_exact_correspondence(&e.correspondence), "{surface}: lossy");
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
        // No fabricated artifact — a partial rendering that cannot round-trip is never emitted.
        assert!(
            e.artifacts.is_empty(),
            "{surface}: a non-expressible grammar emits no artifact"
        );
        // The blocking construct is enumerated (carried and flagged, never a silent skip).
        assert!(
            e.unsupported.iter().any(|u| u.contains("set-difference")),
            "{surface}: the set-difference operator must be enumerated unsupported: {:?}",
            e.unsupported
        );
        assert!(!e.round_trip_holds);
    }
}

#[test]
fn gbnf_and_lark_hard_fail_when_the_version_major_is_absent() {
    for surface in ["gbnf", "lark"] {
        // A gmn glyph grammar is present but the graph-resolved dialect major is not — the
        // version-keyed path is a mandatory capability, so this is a HARD FAIL, never a default.
        let input = LangProjectionInput {
            grammars: vec![NamedSource {
                name: "gmn".to_owned(),
                bytes: GMN_REPRESENTABLE_EBNF.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let error = target(surface)
            .emit(&input)
            .expect_err("a version-keyed GMN surface cannot default its major");
        assert_eq!(error.failure_class, LangFailure::SilentIngestDrop);
        assert!(
            error.construct.contains("dialect major is absent"),
            "{surface}: the missing major must hard-fail explicitly: {error:?}"
        );
    }
}

#[test]
fn gbnf_and_lark_fold_a_no_source_row_when_no_gmn_grammar_is_present() {
    for surface in ["gbnf", "lark"] {
        // No `gmn` grammar in the composed model ⇒ honest empty emission (the driver folds one
        // no-source row); crucially NOT a hard fail on the absent major.
        let input = LangProjectionInput {
            grammars: vec![NamedSource {
                name: "turtle".to_owned(),
                bytes: TURTLE_EBNF.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let emissions = target(surface).emit(&input).expect("emit");
        assert!(
            emissions.is_empty(),
            "{surface}: no gmn grammar ⇒ no emission (turtle is not a GMN constrained-decode surface)"
        );
    }
}

// ── OntoLex: SoundUnder, DERIVED from the lossy-lens correspondence ─────────────────

#[test]
fn ontolex_target_carries_soundunder_over_the_lang_lexicon() {
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "en".to_owned(),
            bytes: LANG_LEXICON.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &target("ontolex-lemon").emit(&input).expect("emit")[0];
    // The forward projection is a lossy lens — never an exact correspondence.
    assert!(
        !is_exact_correspondence(&e.correspondence),
        "the OntoLex projection flattens the epistemic strata: never exact"
    );
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    // The flattened epistemic strata are enumerated.
    assert!(
        e.unsupported
            .iter()
            .any(|u| u.contains("vantage") || u.contains("epistemic"))
    );
    // The emission carries the emitted OntoLex-Lemon RDF as an artifact.
    let ttl = e
        .artifacts
        .iter()
        .find(|a| a.is_rdf)
        .map(|a| String::from_utf8_lossy(&a.bytes).into_owned())
        .expect("an OntoLex RDF artifact");
    assert!(ttl.contains("lemon/ontolex#LexicalEntry"), "{ttl}");
    assert!(ttl.contains("lemon/ontolex#LexicalSense"), "{ttl}");
}

#[test]
fn gmn1_target_never_defaults_a_missing_current_codebook() {
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "prefix-only".to_owned(),
            bytes: b"@prefix lang: <https://blackcatinformatics.ca/lang/> .\nlang:Form lang:denotedForm lang:Form .\n"
                .to_vec(),
        }],
        ..Default::default()
    };
    let error = target("gmn1")
        .emit(&input)
        .expect_err("a selected GMN target cannot run without its pinned dictionary");
    assert_eq!(error.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        error
            .construct
            .contains("version-pinned resolution cannot default"),
        "a missing carrier dictionary must hard-fail with an explicit diagnostic: {error:?}"
    );
}

// ── CoNLL-U: one artifact per co-resident reading (no silent winner) ────────────────

#[test]
fn conllu_target_emits_one_artifact_per_reading() {
    // A composed form scoped to two co-resident analyses — never collapsed to one tree.
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "saw-duck".to_owned(),
            bytes: LANG_AMBIGUOUS.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &target("conllu").emit(&input).expect("emit")[0];
    assert_eq!(e.emitted_reading_count, Some(2));
    assert_eq!(
        e.artifacts.len(),
        2,
        "two co-resident analyses must emit two artifacts, never one"
    );
    // The emitted CoNLL-U byte-round-trips, so single-reading morphosyntax is Exact.
    assert!(is_exact_correspondence(&e.correspondence));
    assert!(e.round_trip_holds);
}

// ── Invariant 5: Exact-negative teeth (falsifiable exactness) ───────────────────────

#[test]
fn grammar_exactness_is_falsifiable_under_perturbation() {
    // A grammar whose canonical form is perturbed no longer round-trips to the original —
    // so "Exact" is a demonstrated property, not a fiat claim (mirrors the CL fixpoint
    // non-idempotence teeth).
    let g = EbnfBridge.to_grammar(CF_EBNF.as_bytes()).unwrap();
    let canon = g.canonicalize();
    let text = serialize_grammar(&canon);
    // Perturb the emitted grammar: rename a nonterminal in ONE place only, breaking the
    // reference/definition agreement, and confirm the re-parse is NOT the original canon.
    let perturbed = text.replacen("digit+", "digit9+", 1);
    let reparsed = parse_grammar(perturbed.as_bytes(), Formalism::Ebnf).expect("still parses");
    assert_ne!(
        reparsed.canonicalize(),
        canon,
        "a perturbed grammar must fail canonical equality (exactness is falsifiable)"
    );
    // The unperturbed round-trip still holds — the teeth cut only the mutation.
    let clean = parse_grammar(text.as_bytes(), Formalism::Ebnf).unwrap();
    assert_eq!(clean.canonicalize(), canon);
}

#[test]
fn conllu_exactness_is_falsifiable_under_token_perturbation() {
    // The CoNLL-U round-trip is a genuine identity, not a constant: mutating the parsed
    // model (a token's lemma) makes the re-serialization differ from the source bytes.
    let doc = gmeow_lang_bridge::parse_conllu(CONLLU_FIXTURE).expect("fixture parses");
    let clean = gmeow_lang_bridge::serialize_conllu(&doc);
    assert_eq!(clean, CONLLU_FIXTURE, "the honest round-trip is byte-exact");

    let mut mutated = doc.clone();
    let token = mutated
        .sentences
        .iter_mut()
        .find_map(|s| s.tokens.first_mut())
        .expect("a token to perturb");
    token.lemma.push_str("_PERTURBED");
    let serialized = gmeow_lang_bridge::serialize_conllu(&mutated);
    assert_ne!(
        serialized, CONLLU_FIXTURE,
        "a perturbed token must break byte-exactness (round-trip is falsifiable)"
    );
    // The bridge round-trip of the ORIGINAL bytes is still exact — teeth cut only the mutation.
    assert_eq!(
        ConlluBridge.round_trip(CONLLU_FIXTURE).unwrap(),
        CONLLU_FIXTURE
    );
    // A malformed input HARD FAILS rather than silently returning a repaired-exact result.
    assert!(gmeow_lang_bridge::parse_conllu(b"not\tenough\tcols\n\n").is_err());
}
