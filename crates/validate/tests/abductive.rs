// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end proof of the D5 abductive advice producer over real authored schemas.
//!
//! Each fixture merges the canonical logic module (the `logic:AbductiveSchema`
//! vocabulary + the four completeness formulas) and the kernel module (the sortal
//! disjointness the sortal warrant relies on) with a tiny fixture A-Box, then drives
//! [`gmeow_validate::abductive::abductive_advisories`]. The producer is ENGINE-FREE: a
//! conjunctive/relatum candidate is warranted BY CONSTRUCTION (a fresh witness for a missing
//! relatum is a consistent addition), and a sortal candidate is warranted by a sound
//! class-disjointness lookup — only a discriminating model (at least one offered sortal ruled
//! out by a disjoint type) warrants sortal advice for the corroborated remainder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Severity;
use gmeow_validate::abductive::{AbductiveSuggestion, abductive_advisories};
use purrdf::{RdfDataset, RdfDatasetBuilder};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Merge kernel + logic modules with a fixture A-Box into one dataset — the `reasoned`
/// input the producer reads (schemas + formula trees + A-Box; the conjecture engine
/// closes each scenario itself).
fn reasoned(abox_ttl: &str) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for module in [
        "slices/core/kernel/module.ttl",
        "slices/grounding/logic/module.ttl",
    ] {
        let text = std::fs::read_to_string(repo_root().join(module)).expect("read module");
        let dataset =
            purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).expect("module parses");
        builder.push_dataset(dataset.as_ref());
    }
    let abox =
        purrdf::parse_dataset(abox_ttl.as_bytes(), "text/turtle", None).expect("abox parses");
    builder.push_dataset(abox.as_ref());
    builder.freeze().expect("merge")
}

/// Every suggestion whose advisory subject is `subject`.
fn for_subject<'a>(
    suggestions: &'a [AbductiveSuggestion],
    subject: &str,
) -> Vec<&'a AbductiveSuggestion> {
    suggestions
        .iter()
        .filter(|s| s.advisory.subject_iri.as_deref() == Some(subject))
        .collect()
}

fn quad_count(ds: &RdfDataset) -> usize {
    use purrdf::{DatasetView, GraphMatch};
    ds.quads_for_pattern(None, None, None, GraphMatch::Any)
        .count()
}

// ── Case 1: relator-mediation (StrategyRelatumCompletion) ────────────────────────────

#[test]
fn mediation_missing_beneficiary_yields_one_suggestion() {
    // c1 fills committedAgent + intentionGoal, MISSING commitmentBeneficiary.
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         <urn:c1> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:agentA> a gmeow:Agent .\n"
    ));
    let outcome = abductive_advisories(ds.as_ref());
    let mine = for_subject(&outcome, "urn:c1");

    assert_eq!(mine.len(), 1, "exactly one mediation suggestion: {mine:?}");
    let sug = mine[0];
    assert_eq!(sug.advisory.severity, Severity::Note);
    assert!(
        sug.advisory.suggestions[0].contains("gmeow:commitmentBeneficiary"),
        "suggestion must name the specific missing relatum: {:?}",
        sug.advisory.suggestions[0]
    );
    assert!(
        sug.advisory.tags.iter().any(|t| t == "abductive"),
        "carries the abductive tag: {:?}",
        sug.advisory.tags
    );
    assert!(
        sug.advisory
            .tags
            .iter()
            .any(|t| *t == format!("formalizes:{GMEOW}Commitment")),
        "carries the formalizes provenance tag: {:?}",
        sug.advisory.tags
    );
    assert!(
        sug.advisory.tags.iter().any(|t| t.starts_with("warrant:")),
        "carries the warrant tag: {:?}",
        sug.advisory.tags
    );
    // A paired warrant Diag exists and is a Note.
    assert_eq!(sug.warrant.grade().severity, Severity::Note);
    assert!(
        sug.warrant.message().contains("by construction"),
        "the relatum warrant narrates the by-construction argument, not an engine call: {}",
        sug.warrant.message()
    );
    assert!(
        !sug.warrant
            .message()
            .contains("conjecture engine corroborated"),
        "the relatum warrant makes NO engine-corroboration claim: {}",
        sug.warrant.message()
    );
}

// ── Case 2: WEMI chain (StrategyChainCompletion) ─────────────────────────────────────

#[test]
fn item_missing_exemplifies_yields_one_suggestion() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:i1> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let mine_owned = abductive_advisories(ds.as_ref());
    let mine = for_subject(&mine_owned, "urn:i1");
    assert_eq!(mine.len(), 1, "exactly one WEMI suggestion: {mine:?}");
    assert_eq!(mine[0].advisory.severity, Severity::Note);
    assert!(
        mine[0].advisory.suggestions[0].contains("gmeow:exemplifies")
            && mine[0].advisory.suggestions[0].contains("gmeow:Manifestation"),
        "suggestion names gmeow:exemplifies + a Manifestation: {:?}",
        mine[0].advisory.suggestions[0]
    );
    assert!(mine[0].warrant.message().contains("by construction"));
}

// ── Case 3: reference frame (StrategyFrameDeclaration) ───────────────────────────────

#[test]
fn expression_missing_frame_yields_one_suggestion() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:x1> a gmeow:Expression ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:x1");
    assert_eq!(mine.len(), 1, "exactly one frame suggestion: {mine:?}");
    assert_eq!(mine[0].advisory.severity, Severity::Note);
    assert!(
        mine[0].advisory.suggestions[0].contains("gmeow:hasReferenceFrame"),
        "suggestion names gmeow:hasReferenceFrame: {:?}",
        mine[0].advisory.suggestions[0]
    );
}

// ── Case 3b: measurement frame (StrategyFrameDeclaration, PROPERTY guard) ────────────

#[test]
fn unit_bearing_value_missing_frame_yields_one_reference_frame_suggestion() {
    // m1 carries a logic:unit (the IRI-valued witness a framed value exists) but NO
    // logic:referenceFrame — the logic:MeasurementFrameMissing gap. The measurement-frame
    // schema's guard is a PROPERTY-presence atom (logic:unit(this, ?u)), not a class type,
    // so the subject is a gap subject purely by carrying logic:unit. Exactly one "declare a
    // reference frame" advisory, naming logic:referenceFrame.
    let ds = reasoned(&format!(
        "@prefix logic: <{LOGIC}> .\n@prefix gmeow: <{GMEOW}> .\n\
         <urn:m1> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:m1");
    assert_eq!(
        mine.len(),
        1,
        "exactly one measurement-frame suggestion for a unit-bearing value with no frame: {mine:?}"
    );
    assert_eq!(mine[0].advisory.severity, Severity::Note);
    assert!(
        mine[0].advisory.suggestions[0].contains("logic:referenceFrame"),
        "the suggestion must name the missing logic:referenceFrame relatum: {:?}",
        mine[0].advisory.suggestions[0]
    );
    assert!(
        mine[0]
            .advisory
            .tags
            .iter()
            .any(|t| *t == format!("formalizes:{LOGIC}referenceFrame")),
        "carries the logic:referenceFrame provenance tag: {:?}",
        mine[0].advisory.tags
    );
    assert!(mine[0].warrant.message().contains("by construction"));
}

#[test]
fn unit_bearing_value_with_frame_yields_no_measurement_suggestion() {
    // m2 carries BOTH logic:unit and logic:referenceFrame — already complete, so the
    // measurement-frame schema emits nothing (honest absence).
    let ds = reasoned(&format!(
        "@prefix logic: <{LOGIC}> .\n@prefix gmeow: <{GMEOW}> .\n\
         <urn:m2> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox ; logic:referenceFrame <urn:celsiusFrame> .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:m2");
    assert!(
        mine.is_empty(),
        "a unit-bearing value that already declares its logic:referenceFrame yields zero \
         measurement-frame suggestions (honest absence): {mine:?}"
    );
}

// ── Case 4: bare-entity sortal (StrategySortalSpecialization) ────────────────────────

#[test]
fn bare_entity_yields_no_suggestion_a_nondiscriminating_menu_is_suppressed() {
    // A genuinely bare gmeow:Entity — only its guard type, nothing else asserted — has FOUR
    // consistent top-sortal specializations: nothing refutes any of them, so the completeness
    // disjunction does not discriminate at all. Advising a 4-way "specialize to X" menu here
    // would be noise (F1: SUPPRESS non-discriminating sortal advice), so ZERO suggestions.
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:e1> a gmeow:Entity ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:e1");
    assert!(
        mine.is_empty(),
        "a bare entity with nothing refuted yields zero sortal suggestions (honest absence, \
         not a non-discriminating menu): {mine:?}"
    );
}

#[test]
fn entity_refuting_one_sortal_yields_suggestions_for_the_corroborated_remainder() {
    // e2 is typed gmeow:Entity plus a fixture-only class disjoint with gmeow:Agent — NOT
    // itself one of the four top sortals, so e2 is still "bare" w.r.t. the offered
    // specializations (no already-specialized suppression), but adding gmeow:Agent now
    // CLASHES with e2's own assertions: the disjunction genuinely discriminates, so advice
    // is emitted for the corroborated remainder (InformationObject / PhysicalObject /
    // SocialObject) while gmeow:Agent — the refuted disjunct — is excluded.
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         <urn:notAnAgent> a owl:Class ; owl:disjointWith gmeow:Agent .\n\
         <urn:e2> a gmeow:Entity , <urn:notAnAgent> ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:e2");

    assert!(
        !mine
            .iter()
            .any(|s| s.advisory.suggestions[0].contains("gmeow:Agent")),
        "the refuted sortal gmeow:Agent is NOT among the suggestions: {:?}",
        mine.iter()
            .map(|s| &s.advisory.suggestions[0])
            .collect::<Vec<_>>()
    );
    for sortal in ["InformationObject", "PhysicalObject", "SocialObject"] {
        assert!(
            mine.iter()
                .any(|s| s.advisory.suggestions[0].contains(&format!("gmeow:{sortal}"))),
            "the corroborated sortal gmeow:{sortal} IS among the suggestions: {:?}",
            mine.iter()
                .map(|s| &s.advisory.suggestions[0])
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        mine.len(),
        3,
        "exactly the three corroborated disjuncts, the refuted one excluded: {mine:?}"
    );
    for s in &mine {
        assert_eq!(s.advisory.severity, Severity::Note);
        assert!(
            s.advisory
                .tags
                .iter()
                .any(|t| *t == format!("formalizes:{GMEOW}Entity"))
        );
    }
    // Codes are injective (no duplicate advisory code → the D4 claim emitter never clashes).
    let mut codes: Vec<&str> = mine.iter().map(|s| s.advisory.code.as_str()).collect();
    codes.sort_unstable();
    let n = codes.len();
    codes.dedup();
    assert_eq!(codes.len(), n, "advisory codes are injective per candidate");
}

// ── Determinism ──────────────────────────────────────────────────────────────────────

#[test]
fn producer_is_deterministic() {
    let abox = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         <urn:c1> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:e1> a gmeow:Entity ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:x1> a gmeow:Expression ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:i1> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox .\n"
    );
    let ds = reasoned(&abox);
    let run = |ds: &RdfDataset| -> Vec<(String, Option<String>, Vec<String>)> {
        abductive_advisories(ds)
            .into_iter()
            .map(|s| {
                (
                    s.advisory.code,
                    s.advisory.subject_iri,
                    s.advisory.suggestions,
                )
            })
            .collect()
    };
    let a = run(ds.as_ref());
    let b = run(ds.as_ref());
    assert_eq!(a, b, "same input ⇒ identical output");
    assert!(!a.is_empty(), "the mixed fixture yields suggestions");
    // Sorted by (code, subject).
    let mut sorted = a.clone();
    sorted.sort_by(|x, y| x.0.cmp(&y.0).then_with(|| x.1.cmp(&y.1)));
    assert_eq!(a, sorted, "output is sorted by (code, subject_iri)");
}

// ── Honest absence & non-mutation ────────────────────────────────────────────────────

#[test]
fn already_complete_subjects_yield_zero_suggestions() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         <urn:cFull> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:aA> ; \
             gmeow:commitmentBeneficiary <urn:bB> ; gmeow:intentionGoal <urn:gG> .\n\
         <urn:eAgent> a gmeow:Entity , gmeow:Agent ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:iFull> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:exemplifies <urn:manif> .\n\
         <urn:xFull> a gmeow:Expression ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:hasReferenceFrame <urn:frame> .\n\
         <urn:manif> a gmeow:Manifestation .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    for subject in ["urn:cFull", "urn:eAgent", "urn:iFull", "urn:xFull"] {
        assert!(
            for_subject(&all, subject).is_empty(),
            "an already-complete {subject} yields zero suggestions: {:?}",
            for_subject(&all, subject)
        );
    }
}

#[test]
fn two_missing_relata_each_yield_their_own_corroborated_suggestion() {
    // c3 fills ONLY committedAgent — missing beneficiary AND goal. Per-conjunct
    // completeness (G4): each missing relatum is its own independently-warranted
    // candidate, so this yields TWO suggestions — one for gmeow:commitmentBeneficiary
    // and one for gmeow:intentionGoal — and does NOT re-suggest the already-present
    // gmeow:committedAgent. This is the discipline's OWN canonical example: an
    // under-mediated relator with only one party present must still produce advice.
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:c3> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:aA> .\n"
    ));
    let all = abductive_advisories(ds.as_ref());
    let mine = for_subject(&all, "urn:c3");

    assert_eq!(
        mine.len(),
        2,
        "a Commitment missing two relata yields exactly one suggestion per missing relatum: {mine:?}"
    );

    let suggestion_text: Vec<&str> = mine
        .iter()
        .map(|s| s.advisory.suggestions[0].as_str())
        .collect();
    assert!(
        suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:commitmentBeneficiary")),
        "one suggestion must name the missing gmeow:commitmentBeneficiary relatum: {suggestion_text:?}"
    );
    assert!(
        suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:intentionGoal")),
        "one suggestion must name the missing gmeow:intentionGoal relatum: {suggestion_text:?}"
    );
    assert!(
        !suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:committedAgent")),
        "the already-present gmeow:committedAgent relatum must NOT be re-suggested: {suggestion_text:?}"
    );

    for s in &mine {
        assert_eq!(s.advisory.severity, Severity::Note);
        assert!(s.warrant.message().contains("by construction"));
    }
    // Codes are injective — the two candidates never collide onto one advisory code.
    assert_ne!(
        mine[0].advisory.code, mine[1].advisory.code,
        "the two missing-relatum candidates carry distinct advisory codes"
    );
}

#[test]
fn producer_does_not_mutate_the_base_graph() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         <urn:c1> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:e1> a gmeow:Entity ; gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let before = quad_count(ds.as_ref());
    let outcome = abductive_advisories(ds.as_ref());
    assert!(!outcome.is_empty());
    // The witness / scenario-world IRIs the producer mints live only in the borrowed
    // scenario EDB, never in the base graph — the byte count is unchanged.
    assert_eq!(
        before,
        quad_count(ds.as_ref()),
        "the base graph is never mutated by the producer"
    );
}

// ── Part 4: synthetic-scale proof — the bare-entity fan-out is gone ───────────────────

/// A SYNTHETIC large A-Box (500 bare `gmeow:Entity` boxABox individuals + a handful of
/// genuinely-incomplete relator / item / expression / unit-bearing subjects) proves the
/// abductive producer no longer fans a full per-candidate reasoning pass out over every bare
/// entity. This is the STRUCTURAL regression signal, not a brittle wall-clock assert (per the
/// no-calibration discipline): each of the 500 bare entities carries ONLY its guard type, so
/// the sortal class-disjointness lookup ([`sortal_suggestions_for_subject`]) refutes nothing
/// and SUPPRESSES it — an O(1) set lookup per candidate, no `conjecture_test`, no KB rehome.
/// The producer is engine-free. The proof is that this completes as an ordinary fast unit test
/// AND is correct at scale: ZERO sortal advice for any bare entity, and the expected advice
/// for every genuinely-incomplete subject.
///
/// Before the fix this same input would drive 500 × 4 = 2000 `conjecture_test` calls, each
/// re-homing the whole growing KB into a fresh scenario world — the O(individuals²) blow-up
/// that hung `stage-validate`; the test would then fail to complete promptly.
#[test]
fn scale_bare_entities_short_circuit_while_incomplete_subjects_still_advise() {
    const BARE: usize = 500;
    let mut abox = format!("@prefix gmeow: <{GMEOW}> .\n@prefix logic: <{LOGIC}> .\n");
    for n in 0..BARE {
        // A genuinely bare A-Box entity: only its guard type + the boxABox marker, nothing
        // that could refute any offered sortal ⇒ suppressed by the lookup (no engine call).
        abox.push_str(&format!(
            "<urn:bare{n}> a gmeow:Entity ; gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
    }
    // A handful of genuinely-incomplete subjects across the relatum/chain/frame/measurement
    // strategies — all warranted by construction, no engine anywhere.
    abox.push_str(
        "<urn:incCommitment> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; \
             gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:agentA> a gmeow:Agent .\n\
         <urn:incItem> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:incExpression> a gmeow:Expression ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:incUnit> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox .\n",
    );
    let ds = reasoned(&abox);
    let outcome = abductive_advisories(ds.as_ref());

    // (1) Suppression proof: NOT ONE of the 500 bare entities produces any sortal advice.
    let bare_advice: Vec<&str> = outcome
        .iter()
        .filter_map(|s| s.advisory.subject_iri.as_deref())
        .filter(|s| s.starts_with("urn:bare"))
        .collect();
    assert!(
        bare_advice.is_empty(),
        "every bare boxABox gmeow:Entity must be suppressed to ZERO sortal advice at scale (no \
         per-entity reasoning; nothing is refuted so the menu is non-discriminating): {bare_advice:?}"
    );

    // (2) Correctness at scale: every genuinely-incomplete subject still gets exactly its
    // expected advice — the fix scopes and short-circuits, it does not silence real gaps.
    let commitment = for_subject(&outcome, "urn:incCommitment");
    assert_eq!(
        commitment.len(),
        1,
        "the incomplete Commitment (missing only commitmentBeneficiary) still advises: {commitment:?}"
    );
    assert!(
        commitment[0].advisory.suggestions[0].contains("gmeow:commitmentBeneficiary"),
        "the Commitment advice names the missing relatum: {:?}",
        commitment[0].advisory.suggestions[0]
    );
    let item = for_subject(&outcome, "urn:incItem");
    assert_eq!(item.len(), 1, "the incomplete Item still advises: {item:?}");
    assert!(item[0].advisory.suggestions[0].contains("gmeow:exemplifies"));
    let expression = for_subject(&outcome, "urn:incExpression");
    assert_eq!(
        expression.len(),
        1,
        "the incomplete Expression still advises: {expression:?}"
    );
    assert!(expression[0].advisory.suggestions[0].contains("gmeow:hasReferenceFrame"));
    let unit = for_subject(&outcome, "urn:incUnit");
    assert_eq!(
        unit.len(),
        1,
        "the incomplete unit-bearing value still advises: {unit:?}"
    );
    assert!(unit[0].advisory.suggestions[0].contains("logic:referenceFrame"));
}

// ── Part 5: relatum-heavy scale proof — the conjunctive warrant does ZERO engine work ──

/// A SYNTHETIC large A-Box of GENUINELY-INCOMPLETE RELATUM subjects — hundreds of one-party
/// `gmeow:Commitment`s (each missing TWO relata), bare `gmeow:Item`s (each missing
/// `gmeow:exemplifies`), and unit-bearing values (each missing `logic:referenceFrame`) —
/// proves the conjunctive/relatum warrant path performs NO native `conjecture_test` work at
/// all: every one of these candidates is warranted BY CONSTRUCTION (a fresh witness for a
/// missing relatum is a consistent addition), so the producer emits their advice with zero
/// reasoning passes. This is the STRUCTURAL/functional regression signal (per the
/// no-calibration discipline), NOT a brittle wall-clock assert: before the fix EACH of these
/// ~1500 candidates drove a full `conjecture_test` over the whole reasoned KB (the residual
/// that kept `stage-validate` at 16+ minutes); now the entire relatum-heavy corpus is
/// warranted by construction and this runs as an ordinary fast unit test.
///
/// Asserts, at scale: EVERY incomplete relatum subject still gets exactly its expected
/// by-construction advice (the fix drops the tautological engine call, it does NOT silence
/// real gaps), and EVERY relatum warrant is worded as a by-construction argument — never a
/// (now-false) engine-corroboration claim.
#[test]
fn scale_relatum_heavy_input_is_warranted_entirely_by_construction() {
    const N: usize = 300;
    let mut abox = format!("@prefix gmeow: <{GMEOW}> .\n@prefix logic: <{LOGIC}> .\n");
    for n in 0..N {
        // One-party Commitment: only committedAgent present ⇒ MISSING beneficiary + goal ⇒
        // two by-construction candidates each.
        abox.push_str(&format!(
            "<urn:c{n}> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:a{n}> .\n"
        ));
        // Bare Item ⇒ MISSING gmeow:exemplifies ⇒ one by-construction candidate.
        abox.push_str(&format!(
            "<urn:it{n}> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        // Unit-bearing value ⇒ MISSING logic:referenceFrame ⇒ one by-construction candidate.
        abox.push_str(&format!(
            "<urn:u{n}> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
    }
    let ds = reasoned(&abox);
    let outcome = abductive_advisories(ds.as_ref());

    for n in 0..N {
        let commitment = for_subject(&outcome, &format!("urn:c{n}"));
        assert_eq!(
            commitment.len(),
            2,
            "each one-party Commitment yields exactly two by-construction suggestions: {commitment:?}"
        );
        let item = for_subject(&outcome, &format!("urn:it{n}"));
        assert_eq!(
            item.len(),
            1,
            "each bare Item yields one suggestion: {item:?}"
        );
        let unit = for_subject(&outcome, &format!("urn:u{n}"));
        assert_eq!(
            unit.len(),
            1,
            "each unit-bearing value yields one suggestion: {unit:?}"
        );
        // Every relatum warrant is a by-construction argument — never an engine claim.
        for s in commitment.iter().chain(item.iter()).chain(unit.iter()) {
            assert!(
                s.warrant.message().contains("by construction"),
                "every relatum warrant narrates the by-construction argument: {}",
                s.warrant.message()
            );
            assert!(
                !s.warrant
                    .message()
                    .contains("conjecture engine corroborated"),
                "no relatum warrant may claim an engine corroboration: {}",
                s.warrant.message()
            );
        }
    }
}

// ── Part 6: sortal-heavy scale proof — the O(1) class-disjointness lookup ──────────────

/// A SYNTHETIC large A-Box that stresses the SORTAL path specifically — ~700 bare
/// `gmeow:Entity` boxABox individuals + ~50 subjects each carrying a fixture class
/// `owl:disjointWith` one top sortal — proves the sortal wing is now an O(1) CLASS-DISJOINTNESS
/// LOOKUP, not a per-candidate reasoning pass. This is the structural proof the ~757-subject
/// real-corpus bottleneck (the SORTAL fan-out that hung `stage-validate` for 6.5+ minutes with
/// its ~1664 per-candidate KB-rehome + conjecture calls) is gone: it runs as an ordinary fast
/// unit test with NO engine and NO reasoning at all.
///
/// Correctness at scale: every bare entity is SUPPRESSED (nothing refuted ⇒ non-discriminating
/// menu ⇒ zero advice), and every disjoint subject emits EXACTLY the three non-refuted sortals
/// (the one its fixture class is `owl:disjointWith` is excluded, the others corroborated).
///
/// Before the fix this input drove (700 + 50) × 4 = 3000 `conjecture_test` calls, each rehoming
/// the whole growing KB into a fresh scenario world and running a full DL consistency check —
/// the exact SORTAL bottleneck measured on the real corpus.
#[test]
fn scale_sortal_lookup_is_o1_bare_suppressed_disjoint_emits_the_remainder() {
    const BARE: usize = 700;
    const DISJOINT: usize = 50;
    // The four top sortals in the completeness disjunction, each paired with a fixture class
    // declared disjoint with it. A subject typed gmeow:Entity + the fixture class refutes that
    // ONE sortal (a class-disjointness clash) and corroborates the other three.
    let sortals = [
        "Agent",
        "InformationObject",
        "PhysicalObject",
        "SocialObject",
    ];
    let mut abox =
        format!("@prefix gmeow: <{GMEOW}> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n");
    for sortal in sortals {
        abox.push_str(&format!(
            "<urn:notA{sortal}> a owl:Class ; owl:disjointWith gmeow:{sortal} .\n"
        ));
    }
    for n in 0..BARE {
        abox.push_str(&format!(
            "<urn:bare{n}> a gmeow:Entity ; gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
    }
    for n in 0..DISJOINT {
        let sortal = sortals[n % sortals.len()];
        abox.push_str(&format!(
            "<urn:disj{n}> a gmeow:Entity , <urn:notA{sortal}> ; gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
    }
    let ds = reasoned(&abox);
    let outcome = abductive_advisories(ds.as_ref());

    // (1) Every bare entity is suppressed to ZERO sortal advice (nothing refuted).
    let bare_advice = outcome
        .iter()
        .filter_map(|s| s.advisory.subject_iri.as_deref())
        .filter(|s| s.starts_with("urn:bare"))
        .count();
    assert_eq!(
        bare_advice, 0,
        "every bare gmeow:Entity must be suppressed (a non-discriminating menu) at scale: \
         {bare_advice} advised"
    );

    // (2) Every disjoint subject emits EXACTLY the three corroborated sortals, excluding the
    // one its fixture class is disjoint with.
    for n in 0..DISJOINT {
        let refuted = sortals[n % sortals.len()];
        let mine = for_subject(&outcome, &format!("urn:disj{n}"));
        assert_eq!(
            mine.len(),
            3,
            "disjoint subject urn:disj{n} must emit exactly the three corroborated sortals \
             (the refuted gmeow:{refuted} excluded): {mine:?}"
        );
        assert!(
            !mine
                .iter()
                .any(|s| s.advisory.suggestions[0].contains(&format!("gmeow:{refuted}"))),
            "the refuted sortal gmeow:{refuted} must be excluded for urn:disj{n}: {:?}",
            mine.iter()
                .map(|s| &s.advisory.suggestions[0])
                .collect::<Vec<_>>()
        );
        for other in sortals.iter().filter(|s| **s != refuted) {
            assert!(
                mine.iter()
                    .any(|s| s.advisory.suggestions[0].contains(&format!("gmeow:{other}"))),
                "the corroborated sortal gmeow:{other} must be present for urn:disj{n}: {:?}",
                mine.iter()
                    .map(|s| &s.advisory.suggestions[0])
                    .collect::<Vec<_>>()
            );
        }
        // Warranted by class disjointness, never an engine claim.
        for s in &mine {
            assert!(
                s.warrant.message().contains("class disjointness"),
                "the sortal warrant narrates the class-disjointness argument: {}",
                s.warrant.message()
            );
            assert!(
                !s.warrant
                    .message()
                    .contains("conjecture engine corroborated"),
                "no sortal warrant may claim an engine corroboration: {}",
                s.warrant.message()
            );
        }
    }
}
