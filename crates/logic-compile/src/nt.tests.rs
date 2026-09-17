// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// These cover the escaper DIRECTLY. The property used to be reached only through the
// `xcl`/`clif`/`cgif` round-trip tests, which carried the control characters inside an
// IRI — and an IRI may not contain them (RFC 3987), so the RDF IR now refuses to intern
// one and those tests can no longer reach this code at all. The escaper is still the
// thing that must be right, so it is tested on its own terms rather than through a
// carrier that is no longer constructible.

#[test]
fn iri_escape_covers_the_full_control_range_not_just_c0_low() {
    // DEL (0x7F) and the C1 range (0x80–0x9F) are above 0x20 and were the easy ones to
    // miss; `is_control()` covers C0, DEL and C1 alike.
    let escaped = nt_escape_iri("a\u{7F}b\u{85}c\u{01}d");
    assert_eq!(escaped, "a\\u007Fb\\u0085c\\u0001d");
}

#[test]
fn iri_escape_covers_every_irirefs_forbidden_char() {
    // The IRIREF grammar forbids these raw; each must ride as a UCHAR or the re-parse
    // hard-fails. Space is legal-LOOKING, which is exactly why it is easy to omit.
    assert_eq!(
        nt_escape_iri("<>\"{}|^`\\ "),
        "\\u003C\\u003E\\u0022\\u007B\\u007D\\u007C\\u005E\\u0060\\u005C\\u0020"
    );
}

#[test]
fn iri_escape_leaves_ordinary_characters_alone() {
    let iri = "https://blackcatinformatics.ca/logic/Knows";
    assert_eq!(
        nt_escape_iri(iri),
        iri,
        "no escaping is applied gratuitously"
    );
}

#[test]
fn literal_escape_prefers_short_forms_then_uchar_for_the_rest() {
    // \t \n \r have short forms; every other control rides as a UCHAR so it never
    // reaches an XML text node raw.
    assert_eq!(
        nt_escape_literal("a\tb\nc\rd\u{7F}e\u{85}f\u{01}g"),
        "a\\tb\\nc\\rd\\u007Fe\\u0085f\\u0001g"
    );
    assert_eq!(
        nt_escape_literal(r#"back\slash "quoted""#),
        r#"back\\slash \"quoted\""#
    );
}
