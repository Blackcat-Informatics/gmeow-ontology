// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn gap_path_is_unsupported_with_nonempty_residue() {
    // A synthetic gap (empty msgstr) is typed Unsupported on the floor rung with the
    // English carrier carried as residue — untranslatability-as-data.
    let unit = build_unit(
        "https://blackcatinformatics.ca/gmeow/Foo|rdfs:label",
        "Foo",
        "",
        "fr",
        "latinScript",
    );
    assert!(!unit.present);
    let mut loss = LossLedger::new();
    let row = unit_ledger_row(&unit, &mut loss);
    assert_eq!(row.preservation, PreservationKind::Unsupported);
    assert!(
        loss.projection_drops_for(&row.target)
            .iter()
            .any(|d| d.contains("Foo"))
    );
}

#[test]
fn fuzzy_entry_is_not_a_live_crossing() {
    use gmeow_docs::i18n_compile::PoEntry;
    // A machine-seeded `#, fuzzy` entry carries a non-empty msgstr but is not yet a
    // reviewed translation. Routed through the SAME shared policy the corpus loop uses
    // (`live_translation_target`) into the SAME `build_unit`, it must render as a
    // not-yet-live (Unsupported, empty-target) crossing — never a live translation —
    // so unreviewed content cannot surface as reviewed in the gmeow.gts projection.
    let ctx = "https://blackcatinformatics.ca/gmeow/Foo|rdfs:label";
    let fuzzy = PoEntry {
        msgctxt: ctx.to_string(),
        msgid: "Foo".to_string(),
        msgstr: "Machine seed".to_string(),
        fuzzy: true,
    };
    let reviewed = PoEntry {
        fuzzy: false,
        ..fuzzy.clone()
    };
    let fuzzy_unit = build_unit(
        &fuzzy.msgctxt,
        &fuzzy.msgid,
        live_translation_target(&fuzzy),
        "fr",
        "latinScript",
    );
    let reviewed_unit = build_unit(
        &reviewed.msgctxt,
        &reviewed.msgid,
        live_translation_target(&reviewed),
        "fr",
        "latinScript",
    );
    assert!(
        !fuzzy_unit.present,
        "a #, fuzzy entry must not be a present (live) translation crossing"
    );
    assert!(
        reviewed_unit.present,
        "removing the #, fuzzy flag makes the same entry a present crossing"
    );
    // The fuzzy crossing is byte-identical to a genuinely untranslated (empty-msgstr)
    // one: the machine seed contributes NO target surface to the shipped bundle.
    let untranslated_unit = build_unit(ctx, "Foo", "", "fr", "latinScript");
    assert_eq!(
        emit_ntriples(&[fuzzy_unit]),
        emit_ntriples(&[untranslated_unit]),
        "a fuzzy seed must project identically to an untranslated entry (English fallback)"
    );
}

#[test]
fn weakest_dominates_is_the_order_independent_weakest_join() {
    use PreservationKind::{Exact, Unsupported, ValidationOnly};
    // Weakest = max under strongest-first Ord, regardless of iteration order.
    assert_eq!(
        weakest_dominates([ValidationOnly, Exact].into_iter()),
        ValidationOnly,
        "a weaker ValidationOnly must dominate a stronger Exact"
    );
    assert_eq!(
        weakest_dominates([Exact, ValidationOnly].into_iter()),
        ValidationOnly,
        "the join must be order-independent"
    );
    // Any Unsupported gap (the floor) dominates the whole document.
    assert_eq!(
        weakest_dominates([ValidationOnly, Unsupported, Exact].into_iter()),
        Unsupported
    );
    // A document of all-Exact units rolls up Exact; the vacuous case floors at ValidationOnly.
    assert_eq!(weakest_dominates([Exact, Exact].into_iter()), Exact);
    assert_eq!(weakest_dominates(std::iter::empty()), ValidationOnly);
}

#[test]
fn script_for_lang_maps_known_and_hard_fails_unknown() {
    assert_eq!(script_for_lang("en").unwrap(), "latinScript");
    assert_eq!(script_for_lang("fr").unwrap(), "latinScript");
    assert_eq!(script_for_lang("zh").unwrap(), "hanScript");
    assert_eq!(script_for_lang("zh-Hans").unwrap(), "hanScript");
    // An unmapped language is a HARD FAIL, never a silent default surface.
    let err = script_for_lang("qtz").expect_err("unknown language must hard-fail");
    assert!(format!("{err}").contains("no lang:Script mapping"));
}
