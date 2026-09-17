// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic token-cost contracts for the qualifier-slot policy.

use gmeow_lang_bridge::gmn_glyph_token_cost;

/// The declared factored qualifier-slot aliases: `(alias, full canonical IRI)` pairs
/// for every marker admitted under razor half (a) — measured cost reduction
/// (`design/LANG-GMN.md`, "The measured token-cost razor"). None is admitted under half (b)
/// (the ambiguity-class discharge implemented by the GMN-1 codec/gate):
/// every marker here pays its way on the measured half alone, so no marker needs a
/// fires-without/absent-with fixture pair.
const QUALIFIER_MARKER_ALIASES: &[(&str, &str)] = &[
    // `m` (modality) — standpoint slice's gmeow:ModalForce.
    (
        "nec",
        "https://blackcatinformatics.ca/gmeow/modalForceNecessary",
    ),
    (
        "act",
        "https://blackcatinformatics.ca/gmeow/modalForceActual",
    ),
    (
        "poss",
        "https://blackcatinformatics.ca/gmeow/modalForcePossible",
    ),
    (
        "cf",
        "https://blackcatinformatics.ca/gmeow/modalForceCounterfactual",
    ),
    // `ek` (evidentiality-kind) — observations slice's gmeow:ObservationMethod.
    (
        "dir",
        "https://blackcatinformatics.ca/gmeow/methodDirectObservation",
    ),
    (
        "inst",
        "https://blackcatinformatics.ca/gmeow/methodInstrumentalReading",
    ),
    (
        "rmt",
        "https://blackcatinformatics.ca/gmeow/methodRemoteSensing",
    ),
    (
        "cmp",
        "https://blackcatinformatics.ca/gmeow/methodComputationalModel",
    ),
    (
        "exj",
        "https://blackcatinformatics.ca/gmeow/methodExpertJudgement",
    ),
    ("srv", "https://blackcatinformatics.ca/gmeow/methodSurvey"),
    (
        "strm",
        "https://blackcatinformatics.ca/gmeow/methodStreaming",
    ),
    // `bd` (boundary, `@p` records only) — logic slice's logic:OccurrentBoundary.
    ("open", "https://blackcatinformatics.ca/logic/Open"),
    ("closed", "https://blackcatinformatics.ca/logic/Closed"),
];

/// The razor's half-(a) discharge (`design/LANG-GMN.md`, "The measured token-cost razor"):
/// every declared qualifier-slot alias must cost strictly fewer `cl100k_base` tokens
/// than the full canonical IRI it dealiases — the cost the alternative of inlining or
/// separately asserting the full term would pay, and the reason the dictionary bijection
/// exists at all. A marker that failed this inequality would not be paying its way and would
/// have to be justified under half (b) instead (an executable fires-without/absent-with
/// fixture pair tied to a named `lang:Gmn*` failure class, run through the shipped codec/gate)
/// or dropped.
#[test]
fn qualifier_marker_aliases_cost_less_than_full_iri() {
    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        let alias_cost = gmn_glyph_token_cost(alias);
        let iri_cost = gmn_glyph_token_cost(full_iri);
        assert!(
            alias_cost < iri_cost,
            "qualifier-slot alias {alias:?} measures {alias_cost} tokens, which must be \
             strictly cheaper than its full canonical IRI {full_iri:?} ({iri_cost} tokens) — \
             the razor's half-(a) measured-cost-reduction discharge"
        );
    }
}
