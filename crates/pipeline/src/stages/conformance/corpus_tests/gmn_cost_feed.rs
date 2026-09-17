// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only cost-feed and alias assertions over producer-selected native observations.

fn costs() -> &'static super::super::gmn_dictionary::costs::Costs {
    &super::gmn_dictionary::observations().costs
}

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

#[test]
fn authored_glyph_cost_matches_measurement() {
    let observed = costs();
    assert!(
        observed.rows.len() >= 18,
        "required token-cost feed inventory"
    );
    for row in &observed.rows {
        assert_eq!(row.codepoints.len(), 1, "one codepoint spelling: {row:?}");
        assert_eq!(row.values.len(), 1, "one authored cost value: {row:?}");
        let authored: usize = row.values[0].parse().expect("authored integer cost");
        let measured = *row.measured.as_ref().expect("glyph cost measurement");
        assert_eq!(authored, measured, "authored cost diverges: {row:?}");
    }
}

#[test]
fn qualifier_marker_aliases_are_authored_dictionary_entries() {
    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        assert!(
            costs()
                .aliases
                .get(*full_iri)
                .is_some_and(|aliases| aliases.contains(*alias)),
            "no authored dictionary entry binds {full_iri} to {alias:?}"
        );
    }
}

#[test]
fn codec_emits_the_authored_qualifier_marker_aliases_verbatim() {
    for (alias, full_iri) in QUALIFIER_MARKER_ALIASES {
        let text = costs()
            .emitted
            .get(*full_iri)
            .expect("required alias probe")
            .as_ref()
            .expect("codec writes the alias probe");
        assert!(
            text.contains(&format!(": {alias}")),
            "the codec must emit {alias:?} verbatim for {full_iri}, got:\n{text}"
        );
    }
}

#[test]
fn denotation_brackets_are_not_glyphs() {
    let observed = costs();
    assert!(
        !observed.spellings.is_empty(),
        "required authored glyph inventory"
    );
    for spelling in &observed.spellings {
        assert!(
            !spelling
                .split(' ')
                .any(|group| group == "U+27E6" || group == "U+27E7"),
            "denotation brackets must use the den named key: {spelling:?}"
        );
    }
    assert!(
        observed.denotation_form_present,
        "the den named-key form must be authored"
    );
}
