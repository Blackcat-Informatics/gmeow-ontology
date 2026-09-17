// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn cmp(case: &str, world: &str, native: &str, published: &str) -> ExternalComparison {
    ExternalComparison {
        case: case.to_owned(),
        world: world.to_owned(),
        native: native.to_owned(),
        published: published.to_owned(),
    }
}

/// Assert every emitted line lands in the conformance graph.
fn all_lines_in_conformance_graph(nq: &str) {
    for line in nq.lines() {
        assert!(
            line.ends_with(&format!("<{CONFORMANCE_GRAPH}> .")),
            "line not in the conformance graph: {line}"
        );
    }
}

#[test]
fn capability_gap_emitter_is_deterministic_and_names_the_ontology_individual() {
    let (iri_a, block_a) = emit_capability_gap_nq(
        "entailment-mini-divergence",
        "multi-triple-conclusion",
        gmeow_logic::entail::CapabilityGapShape::VendoringMultiGoal,
    );
    let (iri_b, block_b) = emit_capability_gap_nq(
        "entailment-mini-divergence",
        "multi-triple-conclusion",
        gmeow_logic::entail::CapabilityGapShape::VendoringMultiGoal,
    );
    assert_eq!(iri_a, iri_b, "the content-addressed IRI must be stable");
    assert_eq!(block_a, block_b, "repeated calls must be byte-identical");
    all_lines_in_conformance_graph(&block_a);
    assert!(
        block_a.contains(&format!("<{GMEOW}CapabilityGap>")),
        "must type the individual as gmeow:CapabilityGap: {block_a}"
    );
    assert!(
        block_a.contains(&format!("<{GMEOW}GapShapeVendoringMultiGoal>")),
        "must point gmeow:gapShape at the correct ontology individual: {block_a}"
    );
}

#[test]
fn all_agree_emits_comparisons_and_tally() {
    // An all-agree corpus is no longer dropped: it folds a NON-blocking
    // corroboration finding AND a reified comparison individual, plus (via the
    // sibling tally emitter) a CorpusAgreementTally — all in the conformance graph.
    let comparisons = [cmp("consistency/open", "w", "consistent", "consistent")];
    let nq = emit_divergence_nq("w3c-owl2-el", &comparisons);
    assert!(
        !nq.is_empty(),
        "an all-agree run now emits corroboration + comparison quads"
    );
    all_lines_in_conformance_graph(&nq);

    // The agreement folds as a logic:FindingCorroboration finding…
    assert!(
        nq.contains("reason.divergence.agreement"),
        "the agreement folds as a corroboration finding: {nq}"
    );
    assert!(
        nq.contains(&format!("<{LOGIC}FindingCorroboration>")),
        "the corroboration finding carries the logic:FindingCorroboration category: {nq}"
    );
    // …and the comparison is reified with its equivalent lattice relation.
    assert!(nq.contains(&format!("<{GMEOW}ConformanceComparison>")));
    assert!(
        nq.contains(&format!("<{GMEOW}VerdictEquivalent>")),
        "an agreement's lattice relation is VerdictEquivalent: {nq}"
    );

    // The aggregate tally rides the same graph.
    let tally_nq = emit_agreement_tally_nq(&agreement_tally("w3c-owl2-el", &comparisons));
    all_lines_in_conformance_graph(&tally_nq);
    assert!(tally_nq.contains(&format!("<{GMEOW}CorpusAgreementTally>")));
    assert!(tally_nq.contains(&format!("<{GMEOW}tallyAgree> \"1\"")));

    // Deterministic.
    assert_eq!(nq, emit_divergence_nq("w3c-owl2-el", &comparisons));
}

#[test]
fn derived_lattice_relation_matches_divergence_kind() {
    // Equivalent ⟺ Agree, Weaker ⟺ dl-gap (native incomplete), Incomparable ⟺ corpus-only.
    assert_eq!(
        lattice_relation_local("consistent", "consistent"),
        "VerdictEquivalent"
    );
    assert_eq!(
        lattice_relation_local("incomplete", "consistent"),
        "VerdictWeaker"
    );
    assert_eq!(
        lattice_relation_local("consistent", "inconsistent"),
        "VerdictIncomparable"
    );
    // An OntoUML foundation-discipline comparison (non-verdict tokens) still derives
    // a relation by equality: a differing fired discipline set is Incomparable.
    assert_eq!(
        lattice_relation_local("FreeRole", "RelComp"),
        "VerdictIncomparable"
    );
    // native `incomplete` is a coverage GAP even when the tokens coincide (matching
    // compare_external_corpus, which classifies incomplete as DlGap, never Agree).
    assert_eq!(
        lattice_relation_local("incomplete", "incomplete"),
        "VerdictWeaker"
    );

    // The reified individual carries the derived relation matching the emitted finding.
    let agree = emit_divergence_nq("c", &[cmp("k", "w", "consistent", "consistent")]);
    assert!(agree.contains(&format!("<{GMEOW}VerdictEquivalent>")));
    let gap = emit_divergence_nq("c", &[cmp("k", "w", "incomplete", "consistent")]);
    assert!(gap.contains(&format!("<{GMEOW}VerdictWeaker>")));
    assert!(gap.contains("reason.divergence.dl-gap"));
    let disagree = emit_divergence_nq("c", &[cmp("k", "w", "consistent", "inconsistent")]);
    assert!(disagree.contains(&format!("<{GMEOW}VerdictIncomparable>")));
    assert!(disagree.contains("reason.divergence.corpus-only"));
}

#[test]
fn tally_counts_agree_corpus_only_and_dl_gap() {
    // The aggregate tally keeps ALL three kinds (unlike the findings emitter,
    // which drops agrees): one Agree, one CorpusOnly (decided-but-wrong), one
    // DlGap (undecidable). cases == agree + corpus_only + dl_gap.
    let tally = agreement_tally(
        "w3c-owl2-el",
        &[
            cmp("consistency/open", "w", "consistent", "consistent"), // Agree
            cmp("inconsistency/clash", "w", "consistent", "inconsistent"), // CorpusOnly
            cmp("beyond-el/cardinality", "w", "incomplete", "consistent"), // DlGap
        ],
    );
    assert_eq!(tally.corpus, "w3c-owl2-el");
    assert_eq!(tally.cases, 3);
    assert_eq!(tally.agree, 1);
    assert_eq!(tally.corpus_only, 1);
    assert_eq!(tally.dl_gap, 1);
}

#[test]
fn all_agree_corpus_still_yields_a_full_tally() {
    // An all-agree corpus emits no divergence graph but MUST still tally (agree ==
    // cases) — else the dashboard would silently drop a 100%-agreeing corpus.
    let tally = agreement_tally(
        "tptp-mini",
        &[
            cmp("theorem-a", "w", "inconsistent", "inconsistent"),
            cmp("theorem-b", "w", "consistent", "consistent"),
        ],
    );
    assert_eq!(tally.cases, 2);
    assert_eq!(tally.agree, 2);
    assert_eq!(tally.corpus_only, 0);
    assert_eq!(tally.dl_gap, 0);
}

#[test]
fn agree_corpus_only_and_undecidable_emit_three_findings_and_comparisons() {
    let comparisons = [
        cmp("consistency/open", "w", "consistent", "consistent"), // Agree → corroboration
        cmp("inconsistency/clash", "w", "consistent", "inconsistent"), // CorpusOnly
        cmp("beyond-el/cardinality", "w", "incomplete", "consistent"), // DlGap
    ];
    let nq = emit_divergence_nq("w3c-owl2-el", &comparisons);

    // Every emitted quad lands in the conformance graph, never diagnostics.
    let lines: Vec<&str> = nq.lines().collect();
    assert!(!lines.is_empty(), "divergences must emit");
    all_lines_in_conformance_graph(&nq);

    // Three findings now: corroboration (Agree) + CorpusOnly + DlGap, typed gmeow:Finding.
    let finding_types = lines.iter().filter(|l| l.contains("/Finding>")).count();
    assert_eq!(
        finding_types, 3,
        "three findings (corroboration + CorpusOnly + DlGap)"
    );

    // One reified comparison individual PER comparison (three), all in-graph.
    let comparison_types = lines
        .iter()
        .filter(|l| l.contains(&format!("<{GMEOW}ConformanceComparison>")))
        .count();
    assert_eq!(
        comparison_types, 3,
        "one comparison individual per comparison"
    );

    // The structured divergence kinds the native⊇external coverage gate keys on are present.
    assert!(nq.contains("reason.divergence.agreement"));
    assert!(nq.contains("reason.divergence.corpus-only"));
    assert!(nq.contains("reason.divergence.dl-gap"));
    // The raw published expected verdict rides verbatim as provenance (both in the
    // finding message and on the reified comparison's logic:rawStatusToken).
    assert!(
        nq.contains("published expected is inconsistent"),
        "corpus-only finding carries the published expected: {nq}"
    );
    assert!(
        nq.contains(&format!("<{LOGIC}rawStatusToken> \"inconsistent\"")),
        "the reified comparison carries the raw published token: {nq}"
    );

    // Deterministic.
    assert_eq!(nq, emit_divergence_nq("w3c-owl2-el", &comparisons));
}
