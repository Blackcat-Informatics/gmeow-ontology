// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use crate::display::RDF_LANG_STRING;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// A lossy presentation must not alias distinct native datatype terms.
#[test]
fn interner_preserves_native_identity_despite_display_aliases() {
    let mut interner = TermInterner::new();

    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: RDF_LANG_STRING.to_owned(),
        language: None,
        direction: None,
    };

    let id_plain = interner.intern(&plain);
    let id_langless = interner.intern(&langless);
    assert_ne!(id_plain, id_langless);
    assert_eq!(interner.lookup(&langless), Some(id_langless));
    assert_eq!(interner.len(), 2);
    assert!(
        interner
            .displays
            .iter()
            .all(|display| display.get().is_none())
    );
    assert_eq!(
        interner.display_of(id_plain),
        interner.display_of(id_langless)
    );

    match interner.resolve(id_plain) {
        TermValue::Literal { datatype, .. } => assert_eq!(datatype, XSD_STRING),
        other => panic!("expected Literal, got {other:?}"),
    }

    let tagged = TermValue::lang_literal("a", "en");
    let id_tagged = interner.intern(&tagged);
    assert_ne!(id_plain, id_tagged);
    assert_eq!(interner.len(), 3);
}

/// The language tag's CASE is significant: `term_display` never lowercases it.
#[test]
fn interner_lang_tag_case_is_significant() {
    let raw = |lang: &str| TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: RDF_LANG_STRING.to_owned(),
        language: Some(lang.to_owned()),
        direction: None,
    };
    let mut interner = TermInterner::new();
    let id_upper = interner.intern(&raw("EN"));
    let id_lower = interner.intern(&raw("en"));
    assert_ne!(id_upper, id_lower, "lang tag case must stay significant");
    assert_eq!(interner.len(), 2);
    assert_eq!(interner.display_of(id_upper), "\"a\"@EN");
    assert_eq!(interner.display_of(id_lower), "\"a\"@en");
}

/// `lookup` is a pure probe — it never inserts.
#[test]
fn interner_lookup_never_inserts() {
    let mut interner = TermInterner::new();
    let a = TermValue::iri("http://ex/a");
    assert_eq!(interner.lookup(&a), None);
    assert!(interner.is_empty(), "lookup must not insert");
    let id = interner.intern(&a);
    assert_eq!(interner.lookup(&a), Some(id));
    assert_eq!(interner.lookup_iri("http://ex/a"), Some(id));
    assert_eq!(interner.lookup_iri("http://ex/absent"), None);
    assert!(interner.displays[id.index()].get().is_none());
    assert_eq!(interner.len(), 1);
}
#[test]
fn rehash_and_clone_preserve_native_handles_without_rendering() {
    let mut dictionary = TermInterner::new();
    let terms: Vec<_> = (0..1024)
        .map(|index| TermValue::iri(format!("urn:arena:{index}")))
        .collect();
    let handles: Vec<_> = terms.iter().map(|term| dictionary.intern(term)).collect();
    let mut cloned = dictionary.clone();
    for (term, handle) in terms.iter().zip(handles) {
        assert_eq!(dictionary.lookup(term), Some(handle));
        assert_eq!(cloned.intern(term), handle);
        assert_eq!(cloned.resolve(handle), term);
    }
    assert!(
        dictionary
            .displays
            .iter()
            .all(|display| display.get().is_none())
    );
    assert!(
        cloned
            .displays
            .iter()
            .all(|display| display.get().is_none())
    );
}
