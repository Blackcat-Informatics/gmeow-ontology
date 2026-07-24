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
use gmeow_validate::abductive::{AbductiveSuggestion, abductive_advisories};
use purrdf::{RdfDataset, RdfDatasetBuilder};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

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
    Budget {
        max_answers: None,
        max_steps: Some(5_000_000),
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
    let suggestions = abductive_advisories(ds.as_ref(), &budget());
    let mine = for_subject(&suggestions, "urn:c1");

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
    let mine = for_subject(&mine_owned, "urn:i1");
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
    let mine = for_subject(&all, "urn:x1");
    assert_eq!(mine.len(), 1, "exactly one frame suggestion: {mine:?}");
    assert_eq!(mine[0].advisory.severity, Severity::Note);
    assert!(
        mine[0].advisory.suggestions[0].contains("gmeow:hasReferenceFrame"),
        "suggestion names gmeow:hasReferenceFrame: {:?}",
        mine[0].advisory.suggestions[0]
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
         <urn:e2> a gmeow:Entity , <urn:notAnAgent> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
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
         <urn:c1> a gmeow:Commitment ; gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:e1> a gmeow:Entity .\n\
         <urn:x1> a gmeow:Expression .\n\
         <urn:i1> a gmeow:Item .\n"
    );
    let ds = reasoned(&abox);
    let run = |ds: &RdfDataset| -> Vec<(String, Option<String>, Vec<String>)> {
        abductive_advisories(ds, &budget())
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
        "@prefix gmeow: <{GMEOW}> .\n<urn:c3> a gmeow:Commitment ; gmeow:committedAgent <urn:aA> .\n"
    ));
    let all = abductive_advisories(ds.as_ref(), &budget());
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
    let suggestions = abductive_advisories(ds.as_ref(), &budget());
    assert!(!suggestions.is_empty());
    // The witness / scenario-world IRIs the producer mints live only in the borrowed
    // scenario EDB, never in the base graph — the byte count is unchanged.
    assert_eq!(
        before,
        quad_count(ds.as_ref()),
        "the base graph is never mutated by the producer"
    );
}
