// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from
//! `slices/extensions/music/tests/test_music_competency.py`.
//!
//! The competency guard for the music stress-corpus fixtures: the merged ontology
//! closed with every `slices/extensions/music/fixtures/*.ttl`, over which the
//! `queries/competency/music.rq` bundle (a 15-way UNION with per-branch `BIND`)
//! returns exactly the expected `(question, work, evidence)` rows for the 15
//! competency questions.
//!
//! The `expected` set is LIFTED VERBATIM from the Python original — each row's
//! three terms transcribed faithfully — NOT re-derived from the ontology and NOT
//! blessed from engine output. `?question` is a plain `xsd:string` literal (from
//! the branch `BIND("…" AS ?question)`); `?work`/`?evidence` are IRIs.

use crate::conformance_support::*;
use purrdf::TermValue;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// One expected `(question, work, evidence)` row.
fn row(question: &str, work: &str, evidence: &str) -> Vec<TermValue> {
    vec![lit(question), iri(&gm(work)), iri(&gm(evidence))]
}

/// Several rows sharing a `(question, work)` over a list of evidence individuals.
fn rows_over(question: &str, work: &str, evidence: &[&str]) -> Vec<Vec<TermValue>> {
    evidence.iter().map(|ev| row(question, work, ev)).collect()
}

/// Twin of `test_music_competency_query`: the 15-way competency bundle returns
/// exactly the expected work/evidence rows. Registered as
/// `music-competency/query-bundle` (`Feature::Union` + `Feature::Bind`).
#[gmeow_test_batch_macros::batch_test]
fn music_competency_query() {
    let mut expected: Vec<Vec<TermValue>> = Vec::new();
    expected.push(row(
        "Q1: nested rational tuplets",
        "fixtureFerneyhoughWork",
        "fixtureFerneyhoughTuplet54",
    ));
    expected.push(row(
        "Q2: irrational tempo canon",
        "fixtureNancarrowTempoCanonWork",
        "fixtureNancarrowSqrt2Mapping",
    ));
    expected.extend(rows_over(
        "Q3: complete DegreeOfFreedom profile",
        "fixtureFourThirtyThreeWork",
        &[
            "dofFourThirtyThreeDuration",
            "dofFourThirtyThreeDynamics",
            "dofFourThirtyThreeInstrumentation",
            "dofFourThirtyThreeLocation",
            "dofFourThirtyThreeOrder",
            "dofFourThirtyThreePerformerCount",
            "dofFourThirtyThreePitch",
            "dofFourThirtyThreeSoundContent",
            "dofFourThirtyThreeTacet",
            "dofFourThirtyThreeTempo",
        ],
    ));
    expected.push(row(
        "Q4: fragment graph + TraversalConstraint + PerformanceDecisions",
        "fixtureStockhausenKlavierstuckXIWork",
        "fixtureStockhausenTraversalConstraint",
    ));
    expected.extend(rows_over(
        "Q5: 43-tone just intonation with integer-pair ratios",
        "fixturePartch43Work",
        &[
            "fixturePartchRatio1_1",
            "fixturePartchRatio2_1",
            "fixturePartchRatio3_2",
            "fixturePartchRatio4_3",
            "fixturePartchRatio5_3",
            "fixturePartchRatio5_4",
            "fixturePartchRatio9_8",
            "fixturePartchRatio11_8",
        ],
    ));
    expected.push(row(
        "Q6: stochastic glissando field with graphic notation",
        "fixtureXenakisGlissandoWork",
        "fixtureXenakisGlissandoProcess",
    ));
    expected.push(row(
        "Q7: spectrum-derived PitchCollection with CMN projection loss",
        "fixtureGriseyPartielsWork",
        "fixtureGriseyPartielsPitches",
    ));
    expected.extend(rows_over(
        "Q8: graphic score with standpointed symbolic interpretations",
        "fixtureCardewTreatiseWork",
        &["fixtureCardewTranscriptionA", "fixtureCardewTranscriptionB"],
    ));
    expected.extend(rows_over(
        "Q9: mensural notation with unequal talea and color cycles",
        "fixtureArsSubtiliorWork",
        &[
            "fixtureArsSubtiliorTaleaSegment",
            "fixtureArsSubtiliorColorSegment",
        ],
    ));
    expected.push(row(
        "Q10: added-value MetricGroups + non-retrogradable identity \
         + mode of limited transposition",
        "fixtureMessiaenExcerptWork",
        "fixtureMessiaenModeClaim",
    ));
    expected.extend(rows_over(
        "Q11: unsynchronized ad-lib spans bounded by cue anchors",
        "fixtureLutoslawskiAdLibWork",
        &["fixtureLutoslawskiMappingA", "fixtureLutoslawskiMappingB"],
    ));
    expected.push(row(
        "Q12: score-less oral tradition with ornament profile \
         and transmission lineage",
        "fixtureOralRagaYamanWork",
        "fixtureRagaYamanAlapOrnamentProfile",
    ));
    expected.extend(rows_over(
        "Q13: additive aksak MetricGroups with changing meters",
        "fixtureAksakFolkTuneWork",
        &[
            "fixtureAksakMeter5",
            "fixtureAksakMeter7",
            "fixtureAksakMeter9",
        ],
    ));
    expected.extend(rows_over(
        "Q14: polymeter + contested meter + riff transformations \
         + drop-D + refuted genre",
        "fixtureMathRockTrackWork",
        &[
            "fixtureMathRockBar17SixEightClaim",
            "fixtureMathRockBar17TwelveEightClaim",
        ],
    ));
    expected.push(row(
        "Q15: phasing generative process with realizations",
        "fixtureReichPhasingWork",
        "fixtureReichPhasingProcess",
    ));

    QueryCase::new(
        "music-competency/query-bundle",
        &[Feature::Union, Feature::Bind],
    )
    .over_ontology_plus_dir("slices/extensions/music/fixtures")
    .query_file("queries/competency/music.rq")
    .select_distinct_set(expected)
    .run();
}
