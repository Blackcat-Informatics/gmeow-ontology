// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit native↔external-corpus reasoning divergences as a GTS-foldable
//! `gmeow:Finding` graph.
//!
//! When the native reasoner is graded against a published external corpus (the
//! W3C OWL 2 suite / ORE), every native verdict that disagrees with — or cannot
//! reach — the corpus's published expected result is retained as a coverage signal
//! rather than collapsed away. The divergence ledger classifies each comparison
//! ([`gmeow_logic::reason::compare_external_corpus`]) and projects the non-agreeing
//! rows into restricted [`gmeow_errors::Finding`]s
//! ([`gmeow_logic::reason::divergence_findings`]) — the diagnostics doctrine holds
//! that divergence-ledger entries ARE `gmeow:Finding`s, so this reuses that model
//! rather than minting a parallel vocabulary.
//!
//! The findings are emitted as N-Quads in a dedicated named graph
//! ([`CONFORMANCE_GRAPH`]) so they can be folded into a `gmeow.gts` evidence bundle
//! alongside, but distinct from, the `graph/diagnostics` validation findings. The
//! emitter dogfoods the single diagnostics projection
//! ([`gmeow_errors::render::to_gmeow_rdf_in_graph`]): content-addressed finding
//! IRIs, no blank nodes, deterministic order — fold-stable through GTS.

use gmeow_errors::Report;
use gmeow_errors::render::to_gmeow_rdf_in_graph;
use gmeow_logic::reason::{
    ExternalComparison, build_ledger, compare_external_corpus, divergence_findings,
};

/// The named graph the conformance divergence findings ride in — a sibling of
/// `graph/diagnostics`, separating reasoner-correctness evidence (the native-superset
/// coverage-gate signal) from validation/lint findings.
pub const CONFORMANCE_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/conformance";

/// The `logic:` namespace IRI prefix — home of the `logic:Conf*` verdict individuals and
/// the `logic:rawStatusToken` provenance property the reified comparisons point at.
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
/// The `gmeow:` namespace IRI prefix — home of the `gmeow:ConformanceComparison` /
/// `gmeow:CorpusAgreementTally` classes, the `gmeow:comparison*` / `gmeow:tally*`
/// properties, and the `gmeow:Verdict*` lattice-relation individuals the reified
/// comparisons carry (defined by the diagnostics slice).
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The content-addressed instance-IRI base for a reified `gmeow:ConformanceComparison`.
const COMPARISON_BASE: &str = "https://blackcatinformatics.ca/gmeow/conformance-comparison/";
/// The content-addressed instance-IRI base for a reified `gmeow:CorpusAgreementTally`.
const TALLY_BASE: &str = "https://blackcatinformatics.ca/gmeow/corpus-agreement-tally/";
/// The content-addressed instance-IRI base for a reified `gmeow:CapabilityGap`.
const CAPABILITY_GAP_BASE: &str = "https://blackcatinformatics.ca/gmeow/capability-gap/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// The ASCII unit separator joining a content-address key's fields — it cannot occur
/// in a corpus/case/world name or a verdict token, so the joined key is unambiguous.
const KEY_SEP: &str = "\u{1f}";

/// Escape a string literal for N-Quads (mirrors the diagnostics render escaping).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The `logic:Conf*` individual local name a lowercase native/published verdict token
/// projects onto, or `None` when the token is not one of the three recognized
/// conformance-verdict tokens (e.g. an OntoUML foundation-discipline label). A
/// comparison over an unrecognized token still carries its coordinates, raw token, and
/// derived lattice relation, but names no `logic:ConformanceVerdict` individual.
fn verdict_iri_local(token: &str) -> Option<&'static str> {
    match token.trim() {
        "consistent" => Some("ConfConsistent"),
        "inconsistent" => Some("ConfInconsistent"),
        "incomplete" => Some("ConfIncomplete"),
        _ => None,
    }
}

/// The `gmeow:Verdict*` lattice-relation local name DERIVED from a comparison's native
/// and published tokens — the reified image of its [`gmeow_logic::reason::DivergenceKind`]:
/// native `incomplete` → `VerdictWeaker` (DlGap), coincident → `VerdictEquivalent` (Agree),
/// otherwise → `VerdictIncomparable` (CorpusOnly). The `incomplete` check comes FIRST so
/// the derivation matches [`compare_external_corpus`]'s classification exactly (an
/// undecidable case is a coverage gap, never an agreement, even when the tokens coincide).
fn lattice_relation_local(native: &str, published: &str) -> &'static str {
    let native = native.trim();
    let published = published.trim();
    if native == "incomplete" {
        "VerdictWeaker"
    } else if native == published {
        "VerdictEquivalent"
    } else {
        "VerdictIncomparable"
    }
}

/// The content-addressed, deterministic, blank-node-free instance IRI for one reified
/// comparison — a blake3 of `corpus|case|world|native|published` (the SAME hash scheme
/// the diagnostics finding IRIs are minted with), so the folded individual is fold-stable.
fn comparison_iri(corpus: &str, case: &str, world: &str, native: &str, published: &str) -> String {
    let key = [corpus, case, world, native, published].join(KEY_SEP);
    let hash = blake3::hash(key.as_bytes()).to_hex();
    format!("{COMPARISON_BASE}{hash}")
}

/// Emit one reified `gmeow:ConformanceComparison` individual as N-Quads in
/// [`CONFORMANCE_GRAPH`] — the coordinates, both graded verdicts (when the tokens name
/// declared `logic:ConformanceVerdict` individuals), the verbatim published token, and
/// the derived lattice relation. Properties are emitted in a fixed order; the caller
/// sorts blocks by IRI so the whole product is byte-stable.
fn comparison_block(corpus: &str, c: &ExternalComparison) -> (String, String) {
    let native = c.native.trim();
    let published = c.published.trim();
    let iri = comparison_iri(corpus, &c.case, &c.world, native, published);
    let subject = format!("<{iri}>");
    let graph = format!("<{CONFORMANCE_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();
    let mut triple = |p: String, o: String| lines.push(format!("{subject} {p} {o} {graph} ."));
    triple(
        format!("<{RDF_TYPE}>"),
        format!("<{GMEOW}ConformanceComparison>"),
    );
    triple(
        format!("<{GMEOW}comparisonCorpus>"),
        format!("\"{}\"", nq_escape(corpus)),
    );
    triple(
        format!("<{GMEOW}comparisonCase>"),
        format!("\"{}\"", nq_escape(&c.case)),
    );
    triple(
        format!("<{GMEOW}comparisonWorld>"),
        format!("\"{}\"", nq_escape(&c.world)),
    );
    if let Some(local) = verdict_iri_local(native) {
        triple(
            format!("<{GMEOW}comparisonNativeVerdict>"),
            format!("<{LOGIC}{local}>"),
        );
    }
    if let Some(local) = verdict_iri_local(published) {
        triple(
            format!("<{GMEOW}comparisonPublishedVerdict>"),
            format!("<{LOGIC}{local}>"),
        );
    }
    triple(
        format!("<{LOGIC}rawStatusToken>"),
        format!("\"{}\"", nq_escape(published)),
    );
    triple(
        format!("<{GMEOW}comparisonLatticeRelation>"),
        format!("<{GMEOW}{}>", lattice_relation_local(native, published)),
    );
    (iri, format!("{}\n", lines.join("\n")))
}

/// Grade native verdicts against an external corpus's published expected verdicts and
/// emit both the divergence `gmeow:Finding` graph AND one reified
/// `gmeow:ConformanceComparison` individual per comparison, all in [`CONFORMANCE_GRAPH`].
///
/// EVERY comparison folds: an agreement now becomes a NON-blocking
/// `logic:FindingCorroboration` finding (positive corroborating evidence) alongside its
/// reified comparison individual; a disagreement (`CorpusOnly`) or undecidable case
/// (`DlGap`) becomes a blocking/gap finding plus its comparison individual. The output
/// is deterministic (findings sorted + content-addressed, comparison blocks sorted by
/// IRI) and GTS-fold-stable, and non-empty whenever the corpus has any graded case.
pub fn emit_divergence_nq(corpus: &str, comparisons: &[ExternalComparison]) -> String {
    let rows = compare_external_corpus(corpus, comparisons);
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    let findings = divergence_findings(&ledger);

    let mut report = Report::new("conformance");
    for finding in findings {
        report.add_finding(finding);
    }
    let mut out = to_gmeow_rdf_in_graph(&report, CONFORMANCE_GRAPH);

    // Reify EVERY comparison as a content-addressed individual in the SAME graph,
    // sorted by IRI so the appended block is byte-stable regardless of input order.
    let mut blocks: Vec<(String, String)> = comparisons
        .iter()
        .map(|c| comparison_block(corpus, c))
        .collect();
    blocks.sort();
    for (_, block) in blocks {
        out.push_str(&block);
    }
    out
}

/// Emit one reified `gmeow:CorpusAgreementTally` individual as N-Quads in
/// [`CONFORMANCE_GRAPH`] — the aggregate native↔published tally for one corpus, keyed by
/// a content-addressed (blake3-of-corpus) IRI, carrying the five tally properties. This
/// preserves the per-corpus pass rate as first-class ontological data in the reasoned
/// bundle (the aggregate twin of the per-case comparison individuals). Deterministic.
pub fn emit_agreement_tally_nq(tally: &AgreementTally) -> String {
    let hash = blake3::hash(tally.corpus.as_bytes()).to_hex();
    let iri = format!("{TALLY_BASE}{hash}");
    let subject = format!("<{iri}>");
    let graph = format!("<{CONFORMANCE_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();
    let mut triple = |p: String, o: String| lines.push(format!("{subject} {p} {o} {graph} ."));
    let int = |v: usize| format!("\"{v}\"^^<{XSD_INTEGER}>");
    triple(
        format!("<{RDF_TYPE}>"),
        format!("<{GMEOW}CorpusAgreementTally>"),
    );
    triple(
        format!("<{GMEOW}tallyCorpus>"),
        format!("\"{}\"", nq_escape(&tally.corpus)),
    );
    triple(format!("<{GMEOW}tallyCases>"), int(tally.cases));
    triple(format!("<{GMEOW}tallyAgree>"), int(tally.agree));
    triple(format!("<{GMEOW}tallyCorpusOnly>"), int(tally.corpus_only));
    triple(format!("<{GMEOW}tallyDlGap>"), int(tally.dl_gap));
    format!("{}\n", lines.join("\n"))
}

/// Emit one reified `gmeow:CapabilityGap` individual as N-Quads in [`CONFORMANCE_GRAPH`]
/// — the ontology image of one committed divergence case's structured
/// [`gmeow_logic::entail::CapabilityGapShape`], the RDF twin of the agreement-matrix
/// "Capability gaps (by shape)" breakdown. Keyed by a content-addressed
/// (blake3-of-corpus|case|shape-token) IRI, so re-emitting the same case is byte-stable.
/// Returns `(iri, block)`; the caller sorts blocks by IRI so a multi-case fold is
/// byte-stable regardless of input order (mirrors [`comparison_block`]).
pub fn emit_capability_gap_nq(
    corpus: &str,
    case: &str,
    shape: gmeow_logic::entail::CapabilityGapShape,
) -> (String, String) {
    let key = [corpus, case, shape.as_token()].join(KEY_SEP);
    let hash = blake3::hash(key.as_bytes()).to_hex();
    let iri = format!("{CAPABILITY_GAP_BASE}{hash}");
    let subject = format!("<{iri}>");
    let graph = format!("<{CONFORMANCE_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();
    let mut triple = |p: String, o: String| lines.push(format!("{subject} {p} {o} {graph} ."));
    triple(format!("<{RDF_TYPE}>"), format!("<{GMEOW}CapabilityGap>"));
    triple(
        format!("<{GMEOW}capabilityGapCorpus>"),
        format!("\"{}\"", nq_escape(corpus)),
    );
    triple(
        format!("<{GMEOW}capabilityGapCase>"),
        format!("\"{}\"", nq_escape(case)),
    );
    triple(
        format!("<{GMEOW}gapShape>"),
        format!("<{GMEOW}{}>", shape.ontology_individual_local()),
    );
    (iri, format!("{}\n", lines.join("\n")))
}

/// One corpus's aggregate native↔published agreement tally: the per-kind counts the
/// divergence ledger records, plus the case total.
///
/// This is the aggregate sibling of [`emit_divergence_nq`]: where that emitter drops
/// agreements and keeps only the divergent rows as findings, a tally KEEPS the agree
/// count — an all-agree corpus still produces a full tally (agree == cases). It is
/// the raw input the benchmark dashboard projects into a per-corpus pass rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementTally {
    /// The corpus name (the `cases/external/<corpus>/` directory).
    pub corpus: String,
    /// Total graded native↔published comparisons in the corpus.
    pub cases: usize,
    /// Comparisons where the native verdict matched the published expected.
    pub agree: usize,
    /// Comparisons the native path DECIDED but disagreed with the published expected.
    pub corpus_only: usize,
    /// Comparisons the native path could not decide (an honest coverage gap).
    pub dl_gap: usize,
}

/// Classify one corpus's graded comparisons into an [`AgreementTally`].
///
/// A pure classification over the already-grouped comparisons (no disk walk): it runs
/// the same [`compare_external_corpus`] ledger the divergence emitter uses, so the two
/// projections agree by construction. Frozen external grading yields only Agree /
/// CorpusOnly / DlGap rows, so `agree + corpus_only + dl_gap == cases`.
pub fn agreement_tally(corpus: &str, comparisons: &[ExternalComparison]) -> AgreementTally {
    let rows = compare_external_corpus(corpus, comparisons);
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    // `cases` is the true attempted count. `compare_external_corpus` is a total map
    // (one row per comparison, always exactly one of Agree/CorpusOnly/DlGap) and
    // `build_ledger` counts every row without dedup or filtering, so the partition
    // `agree + corpus_only + dl_gap == cases` holds by construction. Enforce it here
    // rather than trust it: if a future edit ever makes the grading filter or dedup, a
    // deflated agree rate must surface loudly, never silently.
    debug_assert_eq!(
        ledger.agree + ledger.corpus_only + ledger.dl_gap,
        comparisons.len(),
        "agreement tally must partition every graded comparison into agree/corpus-only/dl-gap"
    );
    AgreementTally {
        corpus: corpus.to_string(),
        cases: comparisons.len(),
        agree: ledger.agree,
        corpus_only: ledger.corpus_only,
        dl_gap: ledger.dl_gap,
    }
}

#[cfg(test)]
mod tests {
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
}
