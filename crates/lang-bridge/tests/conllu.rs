// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 2 — the CoNLL-U lens complement: a fully-parsed form projection that ALSO round-trips
//! its bytes exactly. Two round-trip surfaces are exercised on-gate over the SAME production
//! [`ConlluBridge::round_trip`]: the ORIGINAL, self-attributed `sample.conllu` (repo content,
//! its own SPDX header carried as CoNLL-U comment lines), and a REAL, ring-fenced + fully
//! attributed vendored UD_English-EWT fragment (CC BY-SA 4.0, upstream bytes verbatim, cleared
//! for vendoring by the native license CATEGORY). The section law (`serialize∘parse = id`) is
//! additionally grounded over a deterministic grammar-edge mutation generator.

use gmeow_lang_bridge::conllu::{self, ConlluBridge, TokenId};
use gmeow_lang_bridge::{Bridge, LangFailure, exact_round_trip_holds, is_exact_correspondence};
use gmeow_lang_form::{AnalysisLevel, Form};
use gmeow_logic_compile::ir::{MorphismClass, PreservationKind};

/// The original, self-attributed CoNLL-U fixture (well-formed, already in the declared
/// normal form).
const FIXTURE: &[u8] = include_bytes!("fixtures/sample.conllu");

/// The RING-FENCED vendored UD_English-EWT fragment — upstream bytes verbatim (CC BY-SA 4.0,
/// attributed in the sidecar `corpus.json` + `/NOTICE`). A real treebank fragment that
/// exercises real UD structure: a multiword-token range, layered FEATS, populated enhanced
/// `DEPS`, and a `SpaceAfter=No` MISC.
const VENDORED_EWT: &[u8] = include_bytes!("vendored/ud-english-ewt/en_ewt-ud-dev-fragment.conllu");

/// The vendored fragment's provenance descriptor — the SAME `corpus.json` the license
/// CATEGORY is keyed off (SPDX + attribution + source URL + ring-fence flag).
const VENDORED_CORPUS_JSON: &str = include_str!("vendored/ud-english-ewt/corpus.json");

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
    // The drops are read back from the lift's loss store; an Exact row records none.
    assert!(
        lifted
            .loss
            .projection_drops_for(&lifted.ledger[0].target)
            .is_empty()
    );
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
    let form = conllu::to_forms(&doc.sentences[0]).expect("sentence projects to a form");
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
    assert!(
        s.comments
            .iter()
            .any(|c| c == "# text = Their cats cannot sleep.")
    );
    let sleep = s
        .tokens
        .iter()
        .find(|t| t.id == TokenId::Simple(5))
        .expect("token 5");
    assert_eq!(sleep.misc, "SpaceAfter=No");
}

/// Gate 2 — a REAL vendored UD treebank fragment round-trips byte-identically through the
/// SAME production [`ConlluBridge::round_trip`] the projection stage (`lang_projection`) and
/// the off-gate `maint_conllu_treebank_sweep` use: one round-trip codepath, one assertion
/// shape (`bridge.round_trip(FRAG) == FRAG`). The fragment is ring-fenced + fully attributed;
/// this test dogfoods that descriptor against the native license CATEGORY.
///
/// The shipped per-reading `generated/projections/lang/conllu/*.conllu` round-trip invariant is
/// the on-gate verdict of the same bridge in `conllu::emit_composed_form` (`round_trip_holds`),
/// exercised by the forward-projection tests; this test hardens the ingest side of that
/// production invariant against real upstream bytes.
#[test]
fn gate2_vendored_treebank_round_trip_is_byte_exact() {
    let out = ConlluBridge
        .round_trip(VENDORED_EWT)
        .expect("well-formed vendored UD fragment round-trips");
    assert_eq!(
        out, VENDORED_EWT,
        "serialize(parse(bytes)) must equal the vendored bytes byte-for-byte"
    );

    // The license CATEGORY, keyed off the corpus.json descriptor (NOT a path): a ring-fenced +
    // fully-attributed CC BY-SA 4.0 fragment clears vendoring as IMPORT_OK.
    let meta: serde_json::Value =
        serde_json::from_str(VENDORED_CORPUS_JSON).expect("corpus.json parses");
    let corpus = gmeow_license::VendoredCorpus {
        spdx_license: meta["spdx_license"].as_str().expect("spdx_license"),
        source_url: meta["source_url"].as_str().expect("source_url"),
        attribution: meta["attribution"].as_str().expect("attribution"),
        ring_fenced: meta["ring_fenced"].as_bool().expect("ring_fenced"),
    };
    assert_eq!(corpus.spdx_license, "CC-BY-SA-4.0");
    assert_eq!(
        gmeow_license::policy_for_vendored_corpus(&corpus),
        gmeow_license::LicensePolicy::ImportOk,
        "the ring-fenced + attributed CC BY-SA fragment is IMPORT_OK"
    );

    // The descriptor and the fragment agree: every declared sent_id is actually present.
    let text = std::str::from_utf8(VENDORED_EWT).expect("the fragment is UTF-8");
    for sid in meta["sent_ids"].as_array().expect("sent_ids array") {
        let sid = sid.as_str().expect("sent_id is a string");
        assert!(
            text.contains(&format!("# sent_id = {sid}")),
            "the vendored fragment must contain the declared sent_id {sid}"
        );
    }

    // The fragment genuinely exercises the real UD structure the descriptor claims.
    assert!(text.contains("1-2\t"), "a multiword-token range is present");
    assert!(
        text.contains("SpaceAfter=No"),
        "a SpaceAfter=No MISC is present"
    );
    assert!(
        text.contains("3:nsubj"),
        "a populated enhanced DEPS column is present"
    );
    assert!(
        text.contains("Case=Nom|Number=Plur|Person=1|PronType=Prs"),
        "layered FEATS are present"
    );
}

/// Assemble a CoNLL-U document from token/comment lines, terminated by the structural blank
/// line the grammar requires.
fn conllu_doc(lines: &[&str]) -> Vec<u8> {
    let mut s = String::new();
    for line in lines {
        s.push_str(line);
        s.push('\n');
    }
    s.push('\n');
    s.into_bytes()
}

/// The well-formed grammar-edge mutants of a base sentence: permuted FEATS key order, injected
/// / removed `SpaceAfter=No` MISC, a multiword-token range `a-b`, an empty node `n.m`, and a
/// populated enhanced `DEPS`. Every one is in the declared normal form, so the section law
/// (`serialize∘parse = id`) must hold byte-exact for each.
fn well_formed_edge_mutants() -> Vec<(&'static str, Vec<u8>)> {
    // Base word lines (all valid: 10 columns, integer HEADs, well-formed FEATS).
    let c_sid = "# sent_id = edge-base";
    let c_text = "# text = The cats slept";
    let w1 = "1\tThe\tthe\tDET\tDT\tDefinite=Def|PronType=Art\t2\tdet\t2:det\t_";
    let w2 = "2\tcats\tcat\tNOUN\tNNS\tNumber=Plur\t3\tnsubj\t3:nsubj\t_";
    let w3 = "3\tslept\tsleep\tVERB\tVBD\tMood=Ind|Tense=Past|VerbForm=Fin\t0\troot\t0:root\t_";

    vec![
        ("base", conllu_doc(&[c_sid, c_text, w1, w2, w3])),
        // Permuted FEATS key order — the bridge must NOT reorder FEATS.
        (
            "feats-permuted",
            conllu_doc(&[
                c_sid,
                c_text,
                "1\tThe\tthe\tDET\tDT\tPronType=Art|Definite=Def\t2\tdet\t2:det\t_",
                w2,
                w3,
            ]),
        ),
        // Injected SpaceAfter=No MISC (base carried `_`).
        (
            "spaceafter-injected",
            conllu_doc(&[
                c_sid,
                c_text,
                w1,
                "2\tcats\tcat\tNOUN\tNNS\tNumber=Plur\t3\tnsubj\t3:nsubj\tSpaceAfter=No",
                w3,
            ]),
        ),
        // Removed SpaceAfter=No MISC (back to `_`) — the inverse edge.
        (
            "spaceafter-removed",
            conllu_doc(&[
                c_sid,
                c_text,
                w1,
                "2\tcats\tcat\tNOUN\tNNS\tNumber=Plur\t3\tnsubj\t3:nsubj\t_",
                w3,
            ]),
        ),
        // A multiword-token range a-b prepended over the words it covers.
        (
            "mwt-range",
            conllu_doc(&[
                c_sid,
                c_text,
                "1-2\tThecats\t_\t_\t_\t_\t_\t_\t_\t_",
                w1,
                w2,
                w3,
            ]),
        ),
        // An enhanced empty node n.m appended (enhanced-graph only; not a basic constituent).
        (
            "empty-node",
            conllu_doc(&[c_sid, c_text, w1, w2, w3, "3.1\t_\t_\t_\t_\t_\t_\t_\t_\t_"]),
        ),
        // A populated enhanced DEPS column (retained verbatim).
        (
            "enhanced-deps",
            conllu_doc(&[
                c_sid,
                c_text,
                w1,
                "2\tcats\tcat\tNOUN\tNNS\tNumber=Plur\t3\tnsubj\t3:nsubj|1:conj\t_",
                w3,
            ]),
        ),
    ]
}

/// The ILL-FORMED mutants: each violates the grammar and MUST hard-fail (`Err`), never a
/// silent repair.
fn ill_formed_edge_mutants() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        // Nine columns instead of ten.
        (
            "nine-columns",
            conllu_doc(&["1\tThe\tthe\tDET\tDT\t_\t0\troot\t_"]),
        ),
        // Multiword-token range with start >= end.
        (
            "mwt-start-ge-end",
            conllu_doc(&["2-1\tx\t_\t_\t_\t_\t_\t_\t_\t_"]),
        ),
        // Malformed FEATS: an empty value.
        (
            "bad-feats-empty-value",
            conllu_doc(&["1\tx\tx\tX\tX\tNumber=\t0\troot\t_\t_"]),
        ),
        // Missing the terminating blank line (built WITHOUT the structural separator).
        (
            "no-trailing-blank",
            b"1\tx\tx\tX\tX\t_\t0\troot\t_\t_\n".to_vec(),
        ),
    ]
}

/// Gate 2 — the section law grounded over grammar-edge CLASSES. For every well-formed mutant
/// `serialize(parse(bytes)) == bytes` byte-exact AND the carried `logic:Correspondence` is an
/// exact isomorphism with a discharged `SectionLaw` (`is_exact_correspondence`); for every
/// ill-formed mutant the bridge HARD-FAILS. This covers the grammar-edge classes regardless of
/// which columns any single vendored treebank happens to exercise.
#[test]
fn gate2_section_law_holds_over_grammar_edge_mutations() {
    for (label, bytes) in well_formed_edge_mutants() {
        let rt = ConlluBridge
            .round_trip(&bytes)
            .unwrap_or_else(|d| panic!("well-formed mutant '{label}' must parse: {d:?}"));
        assert_eq!(
            rt, bytes,
            "serialize∘parse must be the identity for mutant '{label}'"
        );
        // The carried correspondence discharges the SectionLaw the retraction rests on.
        let key = String::from_utf8(bytes.clone()).expect("mutant is UTF-8");
        assert!(
            is_exact_correspondence(&conllu::conllu_correspondence(&key)),
            "the retraction law (serialize∘parse = id) is discharged for mutant '{label}'"
        );
    }

    for (label, bytes) in ill_formed_edge_mutants() {
        assert!(
            ConlluBridge.round_trip(&bytes).is_err(),
            "ill-formed mutant '{label}' must hard-fail, never silently repair"
        );
    }
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
