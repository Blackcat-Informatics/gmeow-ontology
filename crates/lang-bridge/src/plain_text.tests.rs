// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::{exact_round_trip_holds, is_exact_correspondence};
use gmeow_logic_compile::ir::LegPath;

#[test]
fn utf8_string_lifts_and_emits_identical_bytes() {
    let bytes = "The definition prose — with an em-dash and café.".as_bytes();
    let lifted = PlainTextBridge.lift(bytes).expect("valid UTF-8 lifts");
    assert_eq!(
        lifted.surfaces.len(),
        1,
        "one raw surface per plain-text lift"
    );
    assert!(
        lifted.forms.is_empty(),
        "raw prose invents no analyzed form"
    );
    // The surface round-trip is EXACT: emit re-emits the input bytes verbatim.
    assert_eq!(PlainTextBridge.emit(&lifted), bytes);
}

#[test]
fn carried_correspondence_is_exact() {
    let lifted = PlainTextBridge.lift(b"anything").expect("lifts");
    assert!(
        is_exact_correspondence(&lifted.correspondence),
        "the surface round-trip is an isomorphism with discharged laws"
    );
    assert_eq!(
        lifted.correspondence.morphism_class,
        MorphismClass::Isomorphism
    );
    assert_eq!(lifted.ledger.len(), 1);
    assert_eq!(lifted.ledger[0].preservation, PreservationKind::Exact);
}

#[test]
fn surface_identity_round_trip_holds_at_the_leg_level() {
    // The put leg of the identity surface map is the structural inverse of its get leg,
    // so the decidable round-trip check the correspondence gates reuse holds.
    let get = LegPath::Step("http://example.org/lang/surfaceText".to_owned());
    let put = get.invert();
    assert!(exact_round_trip_holds(&get, &put));
}

#[test]
fn non_utf8_hard_fails_never_silently_repaired() {
    // A lone 0xFF byte is not valid UTF-8.
    let diag = PlainTextBridge
        .lift(&[0x41, 0xff, 0x42])
        .expect_err("non-UTF-8 must hard-fail");
    assert_eq!(diag.failure_class, LangFailure::NonUtf8Surface);
    assert!(diag.construct.contains("non-UTF-8"));
}

#[test]
fn degenerate_surfaces_still_lift() {
    for degenerate in ["", "   ", "\u{0}\u{7}"] {
        let lifted = PlainTextBridge
            .lift(degenerate.as_bytes())
            .expect("a degenerate but valid-UTF-8 string still lifts");
        assert_eq!(PlainTextBridge.emit(&lifted), degenerate.as_bytes());
    }
}

#[test]
fn normalization_label_is_honest_for_nfc_and_nfd() {
    // "é" as one precomposed code point is NFC; as "e" + combining acute is NFD.
    assert_eq!(normalization_label("\u{e9}"), "NFC");
    assert_eq!(normalization_label("e\u{301}"), "NFD");
    // Pure ASCII is simultaneously NFC and NFD; NFC is the honest label.
    assert_eq!(normalization_label("plain ascii"), "NFC");
}
