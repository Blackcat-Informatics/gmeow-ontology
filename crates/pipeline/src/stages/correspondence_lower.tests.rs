// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// The EmotionML worked-envelope pin: the emitter PROJECTS the SHIPPED schadenfreude worked
/// instance carried on the in-memory ontology dataset, so its intensity + per-dimension
/// values are COMPUTED — never fabricated and never re-read from disk. This drives the REAL
/// `compute_worked_envelope` over the REAL committed affect `module.ttl` (the carrier the
/// pipeline folds) and asserts the metric-tensor outputs: intensity √(79/100) = 0.888819,
/// valence 0.7 → 0.85, arousal 0.4 → 0.7. Retiring the shipped observation or perturbing its
/// appraisal vector reds this test — the drift gate the invariant demands.
#[test]
fn worked_envelope_projects_the_schadenfreude_example() {
    let root = repo_root();
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &root,
        "stage-mappings",
        WORKED_ENVELOPE_CHANNEL,
    )
    .expect("producer-selected original-source affect observation");
    let observed: WorkedEnvelopeObservation =
        serde_json::from_slice(&bytes).expect("native worked envelope");

    // The shipped observation IRI must resolve as a base-graph subject (retirement is a
    // hard fail): it is authored in module.ttl, never an excluded example overlay.
    assert!(
        observed.observation_present,
        "shipped schadenfreude intensity observation missing from the carrier: {SF_INTENSITY_IRI}"
    );

    let worked = observed.envelope;

    assert_eq!(
        worked.intensity, "0.888819",
        "overall intensity is the computed metric-tensor norm √(79/100)"
    );

    let valence = "https://blackcatinformatics.ca/gmeow/dimensionValence";
    let arousal = "https://blackcatinformatics.ca/gmeow/dimensionArousal";
    assert_eq!(
        worked.dimensions,
        vec![
            (valence.to_owned(), "0.85".to_owned()),
            (arousal.to_owned(), "0.7".to_owned()),
        ],
        "per-dimension unit-clamp values are computed from the schadenfreude vector"
    );
}
