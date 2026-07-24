// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end proof of the D5 abductive advice producer over real authored schemas.
//!
//! Each fixture merges the canonical logic module (the `logic:AbductiveSchema`
//! vocabulary + the four completeness formulas) and the kernel module (the sortal
//! disjointness the sortal warrant relies on) with a tiny fixture A-Box, then drives
//! [`gmeow_validate::abductive::abductive_advisories`]. The producer reasons each
//! candidate through the native conjecture engine; only an engine corroboration warrants
//! a suggestion.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Severity;
use gmeow_logic::query_ir::Budget;
use gmeow_validate::abductive::{
    ABDUCTIVE_MAX_STEPS, AbductiveSuggestion, abductive_advisories, abductive_budget,
};
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

fn budget() -> Budget {
    abductive_budget()
}

/// A budget too tiny to let the native engine reach a conclusive verdict on the sortal
/// disjointness check below (empirically: `entity_refuting_one_sortal_…`'s fixture needs
/// somewhere between 50 and 100 committed derivations to fully resolve every disjunct's
/// `owl:disjointWith` consistency check; `10` sits deep inside the always-exhausted zone,
/// with wide margin on both sides) — used by the exhaustion tests below. Deliberately NOT
/// `0`: a `Corroborated` ground-Horn candidate can need literally zero new derivations (it
/// is redundant with the EDB as-is), so `0` alone would not distinguish "genuinely
/// exhausted" from "trivially already decided" for every fixture; `10` forces the engine to
/// actually attempt (and fail to complete) real DL consistency-checking work.
fn tiny_budget() -> Budget {
    Budget {
        max_answers: None,
        max_steps: Some(10),
    }
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
         <urn:c1> a gmeow:Commitment ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:agentA> a gmeow:Agent .\n"
    ));
    let outcome = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&outcome.suggestions, "urn:c1");

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
        sug.warrant.message().contains("corroborated"),
        "warrant narrates the corroboration: {}",
        sug.warrant.message()
    );
}

// ── Case 2: WEMI chain (StrategyChainCompletion) ─────────────────────────────────────

#[test]
fn item_missing_exemplifies_yields_one_suggestion() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:i1> a gmeow:Item .\n"
    ));
    let mine_owned = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&mine_owned.suggestions, "urn:i1");
    assert_eq!(mine.len(), 1, "exactly one WEMI suggestion: {mine:?}");
    assert_eq!(mine[0].advisory.severity, Severity::Note);
    assert!(
        mine[0].advisory.suggestions[0].contains("gmeow:exemplifies")
            && mine[0].advisory.suggestions[0].contains("gmeow:Manifestation"),
        "suggestion names gmeow:exemplifies + a Manifestation: {:?}",
        mine[0].advisory.suggestions[0]
    );
    assert!(mine[0].warrant.message().contains("corroborated"));
}

// ── Case 3: reference frame (StrategyFrameDeclaration) ───────────────────────────────

#[test]
fn expression_missing_frame_yields_one_suggestion() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n<urn:x1> a gmeow:Expression .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:x1");
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
        "@prefix logic: <{LOGIC}> .\n<urn:m1> logic:unit <urn:degreeCelsius> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:m1");
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
    assert!(mine[0].warrant.message().contains("corroborated"));
}

#[test]
fn unit_bearing_value_with_frame_yields_no_measurement_suggestion() {
    // m2 carries BOTH logic:unit and logic:referenceFrame — already complete, so the
    // measurement-frame schema emits nothing (honest absence).
    let ds = reasoned(&format!(
        "@prefix logic: <{LOGIC}> .\n\
         <urn:m2> logic:unit <urn:degreeCelsius> ; logic:referenceFrame <urn:celsiusFrame> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:m2");
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
        "@prefix gmeow: <{GMEOW}> .\n<urn:e1> a gmeow:Entity .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:e1");
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
         <urn:e2> a gmeow:Entity , <urn:notAnAgent> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:e2");

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
         <urn:c1> a gmeow:Commitment ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:e1> a gmeow:Entity .\n\
         <urn:x1> a gmeow:Expression .\n\
         <urn:i1> a gmeow:Item .\n"
    );
    let ds = reasoned(&abox);
    let run = |ds: &RdfDataset| -> Vec<(String, Option<String>, Vec<String>)> {
        abductive_advisories(ds, &budget())
            .suggestions
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
         <urn:cFull> a gmeow:Commitment ; gmeow:committedAgent <urn:aA> ; \
             gmeow:commitmentBeneficiary <urn:bB> ; gmeow:intentionGoal <urn:gG> .\n\
         <urn:eAgent> a gmeow:Entity , gmeow:Agent .\n\
         <urn:iFull> a gmeow:Item ; gmeow:exemplifies <urn:manif> .\n\
         <urn:xFull> a gmeow:Expression ; gmeow:hasReferenceFrame <urn:frame> .\n\
         <urn:manif> a gmeow:Manifestation .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    for subject in ["urn:cFull", "urn:eAgent", "urn:iFull", "urn:xFull"] {
        assert!(
            for_subject(&all.suggestions, subject).is_empty(),
            "an already-complete {subject} yields zero suggestions: {:?}",
            for_subject(&all.suggestions, subject)
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
        "@prefix gmeow: <{GMEOW}> .\n<urn:c3> a gmeow:Commitment ; gmeow:committedAgent <urn:aA> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&all.suggestions, "urn:c3");

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
        assert!(s.warrant.message().contains("corroborated"));
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
         <urn:c1> a gmeow:Commitment ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:e1> a gmeow:Entity .\n"
    ));
    let before = quad_count(ds.as_ref());
    let outcome = abductive_advisories(ds.as_ref(), &budget());
    assert!(!outcome.suggestions.is_empty());
    // The witness / scenario-world IRIs the producer mints live only in the borrowed
    // scenario EDB, never in the base graph — the byte count is unchanged.
    assert_eq!(
        before,
        quad_count(ds.as_ref()),
        "the base graph is never mutated by the producer"
    );
}

// ── G6 Part A: one named budget constant, shared by both call sites ───────────────────

/// `abductive_budget()` is built from the one named [`ABDUCTIVE_MAX_STEPS`] constant — the
/// SAME item `crates/pipeline/src/stages/validate.rs` and
/// `crates/validate/src/validate_all.rs` both call (proved at the source level: neither
/// call site carries the `5_000_000` literal any more — see the crate-level grep in the
/// G6 verification notes). This test proves the constructor's OWN value is exactly the
/// named constant, not a second, independently-drifting literal.
#[test]
fn abductive_budget_is_built_from_the_named_constant() {
    let b = abductive_budget();
    assert_eq!(
        b.max_steps,
        Some(ABDUCTIVE_MAX_STEPS),
        "abductive_budget() must carry exactly the named ABDUCTIVE_MAX_STEPS ceiling"
    );
    assert_eq!(
        b.max_answers, None,
        "the answer-count axis is not the abductive warrant's discriminator"
    );
}

// ── G6 Part B: budget exhaustion is OBSERVABLE, never a silent drop ───────────────────

/// A candidate warrant test cut short by a deliberately tiny budget surfaces an honest
/// "could-not-decide (budget exhausted)" diagnostic in [`AbductiveOutcome::exhausted`] —
/// NEVER a silent empty result indistinguishable from a genuine `Open`/non-corroboration,
/// and NEVER a false advisory.
///
/// Reuses `entity_refuting_one_sortal_yields_suggestions_for_the_corroborated_remainder`'s
/// OWN fixture: under the production budget, every one of the four offered sortal
/// disjuncts resolves conclusively (one `RefutedInStandpoint` via `owl:disjointWith`, three
/// `Corroborated`). Under [`tiny_budget`] every disjunct's `owl:disjointWith` consistency
/// check is cut short before it can conclude — a genuine, non-trivial DL derivation the
/// zero-derivation-redundant ground-Horn case (see `tiny_budget`'s doc) cannot exercise.
#[test]
fn sortal_candidate_exhausted_by_a_tiny_budget_surfaces_an_exhaustion_diagnostic() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         <urn:notAnAgent> a owl:Class ; owl:disjointWith gmeow:Agent .\n\
         <urn:e2> a gmeow:Entity , <urn:notAnAgent> .\n"
    ));
    let outcome = abductive_advisories(ds.as_ref(), &tiny_budget());

    // No false advisory: an exhausted subject must NOT yield a (dishonest) suggestion —
    // neither the eventually-refuted gmeow:Agent disjunct nor the eventually-corroborated
    // remainder, since NONE of the four disjuncts reached a conclusive verdict.
    assert!(
        for_subject(&outcome.suggestions, "urn:e2").is_empty(),
        "a budget-exhausted subject must never surface a false advisory: {:?}",
        for_subject(&outcome.suggestions, "urn:e2")
    );

    // Not a silent drop: an honest could-not-decide diagnostic is present per disjunct.
    assert_eq!(
        outcome.exhausted.len(),
        4,
        "all four offered sortal disjuncts must surface their own could-not-decide \
         diagnostic, not vanish silently: exhausted = {:?}",
        outcome.exhausted
    );
    for diag in &outcome.exhausted {
        assert_eq!(
            diag.grade().severity,
            Severity::Note,
            "the exhaustion diagnostic is a Note (mirrors the warrant Diag's own grade)"
        );
        assert!(
            diag.message().contains("exhausted") && diag.message().contains("could-not-decide"),
            "the exhaustion diagnostic must honestly name budget exhaustion, never a false \
             advisory: {}",
            diag.message()
        );
        assert!(
            diag.message().contains("urn:e2"),
            "the exhaustion diagnostic must name the dropped subject: {}",
            diag.message()
        );
    }
}

/// The exhaustion path is deterministic under repeated runs, exactly like the corroborated
/// path (`producer_is_deterministic`): the SAME input twice yields the SAME exhaustion
/// diagnostics, sorted by their content-addressed code.
#[test]
fn exhaustion_diagnostics_are_deterministic() {
    let ds = reasoned(&format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         <urn:notAnAgent> a owl:Class ; owl:disjointWith gmeow:Agent .\n\
         <urn:e2> a gmeow:Entity , <urn:notAnAgent> .\n"
    ));
    let run = |ds: &RdfDataset| -> Vec<String> {
        abductive_advisories(ds, &tiny_budget())
            .exhausted
            .into_iter()
            .map(|d| d.message().to_owned())
            .collect()
    };
    let a = run(ds.as_ref());
    let b = run(ds.as_ref());
    assert_eq!(a, b, "same input ⇒ identical exhaustion diagnostics");
    assert!(
        !a.is_empty(),
        "a tiny budget must exhaust at least one disjunct in the sortal fixture"
    );
}
