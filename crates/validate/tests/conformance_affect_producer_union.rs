// SPDX-License-Identifier: AGPL-3.0-only
//! Whole-ontology-union SHACL conformance twins for the affect producer — the
//! OFF-GATE cost-class binary.
//!
//! These 8 twins each union a hand-authored fixture with the WHOLE merged ontology
//! (every `slices/*/*/module.ttl`) and validate the entire graph against the full
//! shape corpus (`.with_ontology()` -> `whole_shapes`, `.shape_union()` -> the live
//! production shape union). That whole-graph SHACL scan is the same irreducible
//! H8 `sh:sparql` cost the sibling
//! `conformance_{finance,agentic,ai_claims,music_analysis,math_producers}` binaries
//! carry: a deterministic cost-partition bench measured the per-twin scan at
//! ~35 GB / 54M allocations of churn -- ~63x the one-time corpus setup and
//! ~25,000x the fixture-only path -- so it is genuinely irreducible per-twin work.
//! Folding raises the critical path to setup + 8x the scan, and a cache can only
//! amortise the ~2% setup, never the per-twin scan. This exhaustive binary is
//! therefore separated from the default lane (`default-filter` in
//! `.config/nextest.toml`) and runs on `maint-heavy`, exactly like its cost-class
//! siblings; the cheap fixture-only + pure-Rust producer tests stay on the
//! per-commit gate in `conformance_affect_producer.rs`.

mod conformance_support;
use conformance_support::*;

// ── SHACL validation-surface twins (the exclusivity invariants a hand-authored
//    or tampered graph must still fail, independent of the producer) ───────────

/// Prefix header shared by the hand-authored twin graphs.
const TWIN_PREFIXES: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
    "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
    "@prefix ex: <https://example.org/affect/> .\n",
);

#[test]
fn twin_affect_decision_over_multilabel_set_fires() {
    // A decision whose decided label belongs to the MULTI-label GoEmotions set —
    // an argmax decision over a non-argmax set is a hard fail.
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:d a gmeow:AffectDecision ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t ; \
         gmeow:decidedLabel gmeow-goemotions:joy ; gmeow:decisionCrossedThreshold true ; \
         gmeow:derivedByFunction gmeow:fnArgmax .\n"
    );
    Case::inline(graph)
        .with_ontology()
        .fails()
        .violations(&["may only be recorded over a single-label"])
        .run();
}

#[test]
fn twin_softmax_over_multilabel_set_fires() {
    // A softmax output over a GoEmotions (multi-label) label — the score semantics
    // implies argmax but the set declares independent-threshold.
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:o a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:vantage ex:run ; \
         gmeow:classifiedTarget ex:t ; gmeow:emittedLabel gmeow-goemotions:joy ; \
         gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreSoftmax ; \
         gmeow:thresholdApplied 0.5 .\n"
    );
    Case::inline(graph)
        .with_ontology()
        .fails()
        .violations(&["implies a label-set decision rule"])
        .run();
}

/// The `gmeow:decidedLabel` property IRI — the path the projected cardinality shape
/// (`generated/shapes/validation-shapes.ttl`) constrains.
const GMEOW_DECIDED_LABEL: &str = "https://blackcatinformatics.ca/gmeow/decidedLabel";

#[test]
fn twin_affect_decision_two_decided_labels_fires() {
    // The categorical winner-take-all bound: a gmeow:AffectDecision decides EXACTLY ONE
    // gmeow:decidedLabel. Two decided labels over an argmax set is not a single-label
    // decision — a hard fail on the LIVE production shape union (the bound is authored in
    // the reasoning layer as gmeow:AffectDecision owl:cardinality 1 and PROJECTED to
    // generated/shapes/validation-shapes.ttl as sh:maxCount 1, enforced via
    // `shape_union::load_shapes`). The projected shape carries NO sh:message, so assert on
    // the constraint component + path, not a message substring.
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:d a gmeow:AffectDecision ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t ; \
         gmeow:decidedLabel gmeow-hf:ekmanJoy , gmeow-hf:ekmanAnger ; \
         gmeow:decisionCrossedThreshold false ; gmeow:derivedByFunction gmeow:fnArgmax .\n\
         ex:run a gmeow:Entity .\n"
    );
    Case::inline(graph)
        .shape_union()
        .fails()
        .fails_on_path(GMEOW_DECIDED_LABEL, "MaxCountConstraintComponent")
        .run();
}

#[test]
fn twin_affect_decision_missing_decided_label_fires() {
    // The mandatory half of the same bound: a gmeow:AffectDecision with no decided label
    // is meaningless — a hard fail on the live production shape union (sh:minCount 1,
    // projected from owl:cardinality 1). Assert on the min-count component + path.
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:d a gmeow:AffectDecision ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t ; \
         gmeow:decisionCrossedThreshold false ; gmeow:derivedByFunction gmeow:fnArgmax .\n\
         ex:run a gmeow:Entity .\n"
    );
    Case::inline(graph)
        .shape_union()
        .fails()
        .fails_on_path(GMEOW_DECIDED_LABEL, "MinCountConstraintComponent")
        .run();
}

#[test]
fn twin_affect_decision_single_decided_label_conforms() {
    // The well-formed shape of the bound: exactly one decided label over an argmax set
    // conforms — the projected cardinality must not over-flag the producer's own output.
    // Validated against the live production shape union (includes validation-shapes.ttl).
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:d a gmeow:AffectDecision ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t ; \
         gmeow:decidedLabel gmeow-hf:ekmanJoy ; gmeow:decisionCrossedThreshold false ; \
         gmeow:derivedByFunction gmeow:fnArgmax .\n\
         ex:run a gmeow:Entity .\n"
    );
    Case::inline(graph).shape_union().run();
}

#[test]
fn twin_affect_decision_two_vantages_conforms() {
    // The projected vantage bound is a plain sh:minCount 1 (from owl:minQualifiedCardinality 1
    // + owl:onClass owl:Thing, which the compiler degrades to an UNTYPED minCount — the value
    // TYPE stays owned by gmeow:vantage rdfs:range gmeow:Entity, not this per-class bound). A
    // LOWER bound, NOT capped at max-1: a decision with TWO distinct vantage runs must still
    // conform against the live production shape union — vantage count is uncapped. (The runs
    // are typed gmeow:Entity here because this ontology-merged harness also enforces the
    // vantage range shape; the plain-minCount, no-over-flag behaviour on the user-data-only
    // `gmeow validate` surface is exercised by the CLI enforcement demonstration.)
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:d a gmeow:AffectDecision ; gmeow:vantage ex:runA , ex:runB ; \
         gmeow:observedFeature ex:t ; gmeow:decidedLabel gmeow-hf:ekmanJoy ; \
         gmeow:decisionCrossedThreshold false ; gmeow:derivedByFunction gmeow:fnArgmax .\n\
         ex:runA a gmeow:Entity .\n\
         ex:runB a gmeow:Entity .\n"
    );
    Case::inline(graph).shape_union().run();
}

#[test]
fn twin_two_exclusive_claims_over_one_target_fires() {
    // Two mutually-exclusive claims routed over one target from an EXCLUSIVE
    // (Ekman7) run — the validation-surface guard for the mutually-exclusive-claims
    // invariant, independent of the producer (which would itself hard-fail before
    // emitting this).
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:o1 a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:vantage ex:run ; \
         gmeow:classifiedTarget ex:t ; gmeow:emittedLabel gmeow-hf:ekmanJoy ; \
         gmeow:classifierScore 0.6 ; gmeow:scoreSemantics gmeow:scoreSoftmax ; \
         gmeow:thresholdApplied 0.5 ; gmeow:supportsAffectiveClaim ex:c1 .\n\
         ex:c1 a gmeow:AffectiveClaim ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t .\n\
         ex:o2 a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:vantage ex:run ; \
         gmeow:classifiedTarget ex:t ; gmeow:emittedLabel gmeow-hf:ekmanAnger ; \
         gmeow:classifierScore 0.55 ; gmeow:scoreSemantics gmeow:scoreSoftmax ; \
         gmeow:thresholdApplied 0.5 ; gmeow:supportsAffectiveClaim ex:c2 .\n\
         ex:c2 a gmeow:AffectiveClaim ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t .\n"
    );
    Case::inline(graph)
        .with_ontology()
        .fails()
        .violations(&["At most one gmeow:AffectiveClaim"])
        .run();
}

#[test]
fn twin_two_exclusive_claims_across_two_runs_does_not_fire() {
    // Two INDEPENDENT argmax runs over the SAME target, each legitimately emitting
    // exactly ONE claim over the exclusive (Ekman7) set. The exclusivity invariant
    // is scoped to a single classifier run, so a shared-target/shared-set join alone
    // must NOT trip it — only >1 claim WITHIN ONE run is a hard fail.
    let graph = format!(
        "{TWIN_PREFIXES}\
         ex:o1 a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:runA ; gmeow:vantage ex:runA ; \
         gmeow:classifiedTarget ex:t ; gmeow:emittedLabel gmeow-hf:ekmanJoy ; \
         gmeow:classifierScore 0.6 ; gmeow:scoreSemantics gmeow:scoreSoftmax ; \
         gmeow:thresholdApplied 0.5 ; gmeow:supportsAffectiveClaim ex:c1 .\n\
         ex:c1 a gmeow:AffectiveClaim ; gmeow:vantage ex:runA ; gmeow:observedFeature ex:t .\n\
         ex:o2 a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:runB ; gmeow:vantage ex:runB ; \
         gmeow:classifiedTarget ex:t ; gmeow:emittedLabel gmeow-hf:ekmanAnger ; \
         gmeow:classifierScore 0.55 ; gmeow:scoreSemantics gmeow:scoreSoftmax ; \
         gmeow:thresholdApplied 0.5 ; gmeow:supportsAffectiveClaim ex:c2 .\n\
         ex:c2 a gmeow:AffectiveClaim ; gmeow:vantage ex:runB ; gmeow:observedFeature ex:t .\n"
    );
    Case::inline(graph).with_ontology().run();
}
