// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn glyph_observations_preserve_raw_membership_and_conflicting_coordinates() {
    let source = purrdf::parse_dataset(
        br#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .
ex:den a lang:Denotation ; g:gmnDenotationGrapheme ex:plus, ex:missing .
ex:untyped g:gmnDenotationGrapheme ex:unselected .
ex:plus g:gmnCodepoints "U+002B", "U+002D" ; g:gmnSigilScope ex:math, ex:logic .
ex:unselected g:gmnCodepoints "U+0041" .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let observed = glyph_sources(&source);
    assert_eq!(observed.len(), 2);
    let plus = &observed["https://example.test/plus"];
    assert_eq!(
        plus.codepoints,
        BTreeSet::from(["U+002B".into(), "U+002D".into()])
    );
    assert_eq!(plus.scopes.len(), 2);
    assert!(
        observed["https://example.test/missing"]
            .codepoints
            .is_empty()
    );
    assert!(!observed.contains_key("https://example.test/unselected"));
}
