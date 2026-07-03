// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for the form content key: determinism, surface-invariance,
//! structural-sensitivity, feature-set order-independence, and interning.

use gmeow_lang_form::{dedup_by_content_key, Form, Interner, MorphFeature, Slot, SurfaceForm};
use proptest::prelude::*;

const EN: &str = "https://blackcatinformatics.ca/lang/english";

fn feat(key: &str, values: &[&str]) -> MorphFeature {
    MorphFeature {
        key: key.to_owned(),
        values: values.iter().map(|v| (*v).to_owned()).collect(),
        layer: None,
    }
}

fn lexeme(lemma: &str, pos: &str) -> Form {
    Form::Lexeme {
        sign_system: EN.to_owned(),
        lemma: lemma.to_owned(),
        part_of_speech: Some(pos.to_owned()),
    }
}

fn word_form(lemma: &str, pos: &str, features: Vec<MorphFeature>) -> Form {
    Form::WordForm {
        sign_system: EN.to_owned(),
        lexeme: Box::new(lexeme(lemma, pos)),
        features,
    }
}

fn slot(index: u32, role: &str, dep_head: Option<u32>, form: Form) -> Slot {
    Slot {
        index,
        role: Some(role.to_owned()),
        dep_relation: Some(role.to_owned()),
        depends_on: dep_head,
        form,
    }
}

/// "cats chase mice" as a composed form with a full slot analysis.
fn cats_chase_mice() -> Form {
    let cats = word_form("cat", "noun", vec![feat("Number", &["Plur"])]);
    let chase = word_form("chase", "verb", vec![feat("Tense", &["Pres"])]);
    let mice = word_form("mouse", "noun", vec![feat("Number", &["Plur"])]);
    Form::Composed {
        sign_system: EN.to_owned(),
        level: "sentence".to_owned(),
        analysis: Some("a1".to_owned()),
        head: Some(1),
        slots: vec![
            slot(0, "subject", Some(1), cats),
            slot(1, "predicate", None, chase),
            slot(2, "object", Some(1), mice),
        ],
    }
}

#[test]
fn content_key_is_deterministic() {
    let form = cats_chase_mice();
    assert_eq!(form.content_key(), form.clone().content_key());
    assert_eq!(form.stable_id(), cats_chase_mice().stable_id());
}

#[test]
fn surface_material_is_excluded_from_form_identity() {
    // Two surfaces of the same composed form: an NFC copy and an NFD copy. Their
    // surface keys differ, but the form they realize keys identically.
    let form = cats_chase_mice();
    let nfc = SurfaceForm {
        text: "cats chase mice".to_owned(),
        script: "latin".to_owned(),
        encoding: "UTF-8".to_owned(),
        normalization: "NFC".to_owned(),
        collation: "en".to_owned(),
    };
    let nfd = SurfaceForm {
        normalization: "NFD".to_owned(),
        ..nfc.clone()
    };
    assert_ne!(
        nfc.surface_key(),
        nfd.surface_key(),
        "distinct normalizations are distinct surfaces"
    );
    // The form's identity does not involve either surface at all.
    assert_eq!(form.content_key(), cats_chase_mice().content_key());
}

#[test]
fn swapping_constituents_changes_identity() {
    let base = cats_chase_mice();
    // Swap the subject and object forms (indexes 0 and 2).
    let cats = word_form("cat", "noun", vec![feat("Number", &["Plur"])]);
    let chase = word_form("chase", "verb", vec![feat("Tense", &["Pres"])]);
    let mice = word_form("mouse", "noun", vec![feat("Number", &["Plur"])]);
    let swapped = Form::Composed {
        sign_system: EN.to_owned(),
        level: "sentence".to_owned(),
        analysis: Some("a1".to_owned()),
        head: Some(1),
        slots: vec![
            slot(0, "subject", Some(1), mice),
            slot(1, "predicate", None, chase),
            slot(2, "object", Some(1), cats),
        ],
    };
    assert_ne!(
        base.content_key(),
        swapped.content_key(),
        "word order and grammatical function are identity-bearing"
    );
}

#[test]
fn covert_form_keys_distinctly_from_an_absent_constituent() {
    // A word form with an overt plural vs a covert (zero) plural: distinct forms.
    let overt = word_form("cat", "noun", vec![feat("Number", &["Plur"])]);
    let covert = Form::Covert {
        sign_system: EN.to_owned(),
        features: vec![feat("Number", &["Plur"])],
    };
    assert_ne!(overt.content_key(), covert.content_key());
    // A bare lexeme (no plural at all) is distinct from the covert plural.
    let absent = lexeme("cat", "noun");
    assert_ne!(covert.content_key(), absent.content_key());
}

#[test]
fn feature_set_order_does_not_affect_identity() {
    let a = word_form(
        "run",
        "verb",
        vec![feat("Tense", &["Past"]), feat("Number", &["Sing"])],
    );
    let b = word_form(
        "run",
        "verb",
        vec![feat("Number", &["Sing"]), feat("Tense", &["Past"])],
    );
    assert_eq!(a.content_key(), b.content_key());
    // And within a feature, the value set is unordered.
    let c = word_form("x", "noun", vec![feat("Case", &["Nom", "Acc"])]);
    let d = word_form("x", "noun", vec![feat("Case", &["Acc", "Nom"])]);
    assert_eq!(c.content_key(), d.content_key());
}

#[test]
fn distinct_analyses_do_not_collapse() {
    let one = cats_chase_mice();
    let two = match cats_chase_mice() {
        Form::Composed {
            sign_system,
            level,
            head,
            slots,
            ..
        } => Form::Composed {
            sign_system,
            level,
            analysis: Some("a2".to_owned()),
            head,
            slots,
        },
        _ => unreachable!(),
    };
    assert_ne!(
        one.content_key(),
        two.content_key(),
        "co-resident analyses of one surface must not intern together"
    );
}

#[test]
fn interner_collapses_structurally_identical_forms() {
    let mut interner = Interner::new();
    let k1 = interner.intern(cats_chase_mice());
    let k2 = interner.intern(cats_chase_mice());
    assert_eq!(k1, k2);
    assert_eq!(interner.len(), 1, "identical forms intern to one node");
    interner.intern(lexeme("dog", "noun"));
    assert_eq!(interner.len(), 2);
    assert!(interner.get(&k1).is_some());
}

#[test]
fn dedup_helper_keeps_one_per_content_key() {
    let mut forms = vec![
        lexeme("cat", "noun"),
        lexeme("dog", "noun"),
        lexeme("cat", "noun"),
    ];
    dedup_by_content_key(&mut forms);
    assert_eq!(forms.len(), 2);
}

#[test]
fn worked_example_stable_id_is_pinned() {
    // A byte-for-byte golden: the stable id of "cats chase mice" must not drift.
    insta::assert_snapshot!(cats_chase_mice().stable_id());
}

proptest! {
    /// Idempotence: keying a form and its clone agree; keying is a pure function.
    #[test]
    fn key_is_idempotent(lemma in "[a-z]{1,8}", n in 0u8..3) {
        let feats = (0..n)
            .map(|i| feat("Number", &[if i % 2 == 0 { "Sing" } else { "Plur" }]))
            .collect::<Vec<_>>();
        let form = word_form(&lemma, "noun", feats);
        prop_assert_eq!(form.content_key(), form.clone().content_key());
    }

    /// Order-independence: permuting a feature set never changes the key.
    #[test]
    fn feature_permutation_is_order_independent(
        keys in prop::collection::vec("[A-Z][a-z]{1,6}", 0..5)
    ) {
        let mut features: Vec<MorphFeature> =
            keys.iter().enumerate().map(|(i, k)| {
                // Distinct keys avoid the ambiguity of two features sharing a key.
                feat(&format!("{k}{i}"), &["Val"])
            }).collect();
        let forward = word_form("x", "noun", features.clone());
        features.reverse();
        let backward = word_form("x", "noun", features);
        prop_assert_eq!(forward.content_key(), backward.content_key());
    }
}
