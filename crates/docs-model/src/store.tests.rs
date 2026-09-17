// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn first_literal_returns_lowest_lexical_form() {
    let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:a ex:label \"zebra\" ;\n\
                       ex:label \"apple\" .\n";
    let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
    // min() semantics: the lexically-lowest literal, NOT dataset order.
    assert_eq!(
        store.first_literal("https://example.org/a", "https://example.org/label"),
        Some("apple".to_owned())
    );
}

#[test]
fn subjects_of_type_finds_named_subjects_sorted() {
    let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:b a ex:Thing .\n\
                   ex:a a ex:Thing .\n";
    let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
    assert_eq!(
        store.subjects_of_type("https://example.org/Thing"),
        vec![
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned()
        ]
    );
}

#[test]
fn blank_subject_literal_read_works() {
    let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:a ex:has [ ex:version \"1.2\" ] .\n";
    let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
    let blanks = store.blank_objects("https://example.org/a", "https://example.org/has");
    assert_eq!(blanks.len(), 1);
    let node = Node::Blank(blanks[0].clone());
    assert_eq!(
        store.first_literal_of(&node, "https://example.org/version"),
        Some("1.2".to_owned())
    );
}
