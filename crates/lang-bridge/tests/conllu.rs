// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 2 — the CoNLL-U lens complement: a fully-parsed form projection that ALSO round-trips
//! its bytes exactly. The fixture is ORIGINAL repo content (no external CC BY-SA import), so
//! it carries its own SPDX header as CoNLL-U comment lines.

use gmeow_lang_bridge::conllu::{self, ConlluBridge, TokenId};
use gmeow_lang_bridge::{exact_round_trip_holds, is_exact_correspondence, Bridge, LangFailure};
use gmeow_lang_form::{AnalysisLevel, Form};
use gmeow_logic_compile::ir::{MorphismClass, PreservationKind};

/// The original, self-attributed CoNLL-U fixture (well-formed, already in the declared
/// normal form).
const FIXTURE: &[u8] = include_bytes!("fixtures/sample.conllu");

#[test]
fn gate2_round_trip_is_byte_exact() {
    let out = ConlluBridge
        .round_trip(FIXTURE)
        .expect("well-formed CoNLL-U round-trips");
    assert_eq!(
        out, FIXTURE,
        "serialize(parse(bytes)) must equal bytes byte-for-byte"
    );
}

#[test]
fn carried_correspondence_is_exact() {
    let lifted = ConlluBridge.lift(FIXTURE).expect("fixture lifts");
    assert!(
        is_exact_correspondence(&lifted.correspondence),
        "the byte round-trip is an isomorphism with discharged laws"
    );
    assert_eq!(
        lifted.correspondence.morphism_class,
        MorphismClass::Isomorphism
    );
    assert!(
        lifted.correspondence.mnemomorphic,
        "the full-fidelity model retains the whole source witness"
    );
    // The declared preservation is Exact with no drops — the complement is retained in the
    // ConlluDoc and reproduced on emit, not charged as a loss.
    assert_eq!(lifted.ledger.len(), 1);
    assert_eq!(lifted.ledger[0].preservation, PreservationKind::Exact);
    assert!(lifted.ledger[0].actual_drops.is_empty());
    assert!(lifted.ledger[0].lossy_drops.is_empty());
}

#[test]
fn carried_leg_pair_round_trips_at_the_leg_level() {
    let (get, put) = conllu::conllu_leg_pair();
    assert!(
        exact_round_trip_holds(&get, &put),
        "the put leg is the structural inverse of the get leg"
    );
}

#[test]
fn form_projection_captures_words_mwt_and_features() {
    let doc = conllu::parse(FIXTURE).expect("parses");
    assert_eq!(doc.sentences.len(), 1);
    let form = conllu::to_forms(&doc.sentences[0]);
    let Form::Composed { level, slots, .. } = &form else {
        panic!("a sentence projects to a Composed form");
    };
    assert_eq!(level, "sentence");
    // Reading order: Their, cats, [cannot = can+not], sleep, .  → 5 top-level slots.
    assert_eq!(slots.len(), 5, "MWT collapses its two words into one slot");

    // The multiword token became an OrthographicWord spanning its two syntactic words.
    let mwt_slot = &slots[2];
    let Form::OrthographicWord { spans, .. } = &mwt_slot.form else {
        panic!("the 3-4 range projects to an OrthographicWord");
    };
    assert_eq!(spans.len(), 2, "the range covers 'can' and 'not'");
    assert!(matches!(spans[0], Form::WordForm { .. }));

    // FEATS parsed into MorphFeatures, values as a SET, layer captured: 'Their' carries a
    // layered Number[psor] feature.
    let their = &slots[0];
    assert_eq!(their.dep_relation.as_deref(), Some("det"));
    assert_eq!(their.depends_on, Some(2), "HEAD landed on the slot");
    let Form::WordForm {
        lexeme, features, ..
    } = &their.form
    else {
        panic!("'Their' is a WordForm");
    };
    let Form::Lexeme {
        lemma,
        part_of_speech,
        ..
    } = lexeme.as_ref()
    else {
        panic!("the word form inflects a Lexeme");
    };
    assert_eq!(lemma, "they");
    assert_eq!(part_of_speech.as_deref(), Some("PRON"));
    let psor = features
        .iter()
        .find(|f| f.key == "Number" && f.layer.as_deref() == Some("psor"))
        .expect("Number[psor] parsed with its layer");
    assert_eq!(psor.values, vec!["Plur".to_owned()]);

    // The composed head is the root (sleep, slot index 3).
    let Form::Composed { head, .. } = &form else {
        unreachable!()
    };
    assert_eq!(*head, Some(3));

    // The lift reaches the parsed analysis level.
    assert_eq!(conllu::analysis_level(), AnalysisLevel::Parsed);
    assert_eq!(conllu::analysis_level().rank(), 4);
}

#[test]
fn wrong_column_count_hard_fails_naming_the_construct() {
    // Nine columns instead of ten (DEPS+MISC merged): a spec violation, never repaired.
    let bad = b"# text = x\n1\tx\tx\tX\tX\t_\t0\troot\t_\n\n";
    let diag = conllu::parse(bad).expect_err("wrong column count must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        diag.construct.contains("10 tab-separated columns"),
        "the diagnostic names the offending construct: {}",
        diag.construct
    );
}

#[test]
fn malformed_id_hard_fails_naming_the_construct() {
    let bad = b"1x\tx\tx\tX\tX\t_\t0\troot\t_\t_\n\n";
    let diag = conllu::parse(bad).expect_err("malformed ID must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        diag.construct.contains("token ID"),
        "the diagnostic names the malformed ID: {}",
        diag.construct
    );
}

#[test]
fn malformed_feats_hard_fails_never_repaired() {
    // `Number=` has an empty value; a repaired/partial feature set would be dishonest.
    let bad = b"1\tx\tx\tX\tX\tNumber=\t0\troot\t_\t_\n\n";
    let diag = conllu::parse(bad).expect_err("bad FEATS must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        diag.construct.contains("FEATS"),
        "the diagnostic names FEATS: {}",
        diag.construct
    );
}

#[test]
fn non_utf8_hard_fails() {
    let diag = conllu::parse(&[0x31, 0xff, 0x0a]).expect_err("non-UTF-8 must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::NonUtf8Surface);
}

#[test]
fn complement_comment_and_misc_are_preserved_verbatim() {
    let text = std::str::from_utf8(FIXTURE).unwrap();
    // The fixture carries exactly the complement material the form view drops.
    assert!(text.contains("# text = Their cats cannot sleep."));
    assert!(text.contains("SpaceAfter=No"));

    // Both survive the round-trip byte-for-byte (they live in the ConlluDoc complement).
    let out = ConlluBridge.round_trip(FIXTURE).expect("round-trips");
    let out_text = String::from_utf8(out).unwrap();
    assert!(out_text.contains("# text = Their cats cannot sleep."));
    assert!(out_text.contains("SpaceAfter=No"));

    // And the parsed model actually holds them on the right structures.
    let doc = conllu::parse(FIXTURE).expect("parses");
    let s = &doc.sentences[0];
    assert!(s
        .comments
        .iter()
        .any(|c| c == "# text = Their cats cannot sleep."));
    let sleep = s
        .tokens
        .iter()
        .find(|t| t.id == TokenId::Simple(5))
        .expect("token 5");
    assert_eq!(sleep.misc, "SpaceAfter=No");
}

/// R5 — the off-gate treebank sweep. Marked `#[ignore]` so it is NOT part of the default
/// `cargo nextest` gate. GIVEN a real UD treebank via `GMEOW_UD_TREEBANK`, it round-trips
/// the whole file and asserts byte-equivalence. No treebank data ships with the repo (that
/// would import CC BY-SA content), so an unset env var is a HARD FAIL that tells the
/// maintainer to point it at a checkout — an honest off-gate failure, never a silent skip.
#[test]
#[ignore = "off-gate maint sweep: set GMEOW_UD_TREEBANK to a real .conllu file"]
fn maint_conllu_treebank_sweep() {
    let path = std::env::var("GMEOW_UD_TREEBANK").unwrap_or_else(|_| {
        panic!(
            "maint_conllu_treebank_sweep requires GMEOW_UD_TREEBANK set to a CoNLL-U file path; \
             no treebank data ships with the repo (it would import CC BY-SA content). Point it \
             at a local UD checkout, e.g. GMEOW_UD_TREEBANK=/path/to/xx.conllu"
        )
    });
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("cannot read GMEOW_UD_TREEBANK '{path}': {e}"));
    let out = ConlluBridge
        .round_trip(&bytes)
        .unwrap_or_else(|d| panic!("treebank '{path}' failed to parse: {d:?}"));
    assert_eq!(
        out, bytes,
        "every sentence in '{path}' must round-trip byte-for-byte"
    );
}
