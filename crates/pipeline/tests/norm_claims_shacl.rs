// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Falsifiable SHACL acceptance test over the SHIPPED bundle's `graph/norm-claims`.
//!
//! `gmeow-dev validate --gts` (`make validate-gts`) SHACL-validates the SHIPPED bundle
//! through `ValidationRun::run(gts_bytes: Some(..))` →
//! `gmeow_validate::store::dataset_from_gts` → `purrdf::gts::flattened_dataset_from_bytes`,
//! which re-homes EVERY named graph (including `graph/norm-claims`) onto the default
//! graph before the merged-SHACL phase runs `store::shacl_validate_dataset` over that
//! same flattened dataset (`crates/validate/src/validate_all.rs`, phase "merged-shacl").
//! So any content the shipped bundle's `graph/norm-claims` carries IS already part of the
//! dataset `make validate-gts` SHACL-checks — not merely present in the bundle but
//! structurally invisible to the validator.
//!
//! Advice fires only on a DATA MATCH (see `norm_claims_bundle.rs`'s module docs), and the
//! shipped bundle's base graph is deliberately TBox-only, so `graph/norm-claims` carries no
//! advisory-harvested `gmeow:ComplianceAssessment` here — the reified advice wing is
//! honestly EMPTY in the shipped bundle. This test proves two things instead of asserting a
//! specific harvested code:
//!   1. Whatever `graph/norm-claims` DOES carry (if anything) SHACL-conforms against the
//!      merged shape union — the re-homed fragment is well-formed as shipped.
//!   2. No `gmeow:ComplianceAssessment` subject in it embeds an `advice.`-family code.
//! Both hold vacuously true when the graph is empty or absent, which is the expected
//! TBox-only state; the assertions below handle that case without panicking.
//!
//! The falsifiable non-vacuity proof that the advisory machinery — including its derived
//! SHACL shape — genuinely fires on real data lives in `advice_wing_fixture.rs`, which
//! supplies its own TEST-ONLY anti-pattern individual and drives the whole
//! compile → validate → split → project pipeline over it end to end.
//!
//! Like `norm_claims_bundle.rs`, this test `.expect()`s the committed bundle AND the
//! post-sync `generated/shapes/*.ttl` shape union (`purrdf::shapes::shape_union::shape_files`
//! fails closed when `generated/shapes/` is empty) — it runs green only after `make sync`.

use std::path::{Path, PathBuf};

use purrdf::gts::model::Graph;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// The `advice.` family code prefix (`crates/validate/src/codes.rs::ADVICE_FAMILY`) — the
/// string this test proves is ABSENT from any `gmeow:ComplianceAssessment` subject IRI in the
/// shipped bundle's `graph/norm-claims`.
const ADVICE_FAMILY: &str = "advice.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Fold the committed `generated/dist/gmeow.gts` into the native GTS `Graph` (term-id
/// space), the same reader path `norm_claims_bundle.rs` uses.
fn read_committed_gts_graph() -> Graph {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    purrdf::gts::read_graph(&bytes, true).expect("read_graph")
}

/// A term-id's display value (IRI/literal lexical form), matching `norm_claims_bundle.rs`'s
/// `term` closure.
fn term_value(g: &Graph, id: usize) -> String {
    g.terms
        .get(id)
        .and_then(|t| t.value.clone())
        .unwrap_or_else(|| format!("<term {id}>"))
}

/// `(subject, predicate, object)` display triples of every quad in `g`.
fn graph_triples(g: &Graph) -> Vec<(String, String, String)> {
    g.quads
        .iter()
        .map(|&(s, p, o, _)| (term_value(g, s), term_value(g, p), term_value(g, o)))
        .collect()
}

/// Build a native GTS [`Graph`] carrying ONLY the `graph/norm-claims` quads of the shipped
/// bundle, re-homed onto the default graph (mirroring exactly what `dataset_from_gts` /
/// `flattened_dataset_from_bytes` does on the real `make validate-gts` path — see the module
/// docs above). Returns `None` when the named graph is entirely absent from the bundle (no
/// such graph-name term interned) or carries zero quads — an absent graph and an empty graph
/// are both honest "no norm-claims content" states, never a panic.
fn norm_claims_only_graph() -> Option<Graph> {
    let g = read_committed_gts_graph();
    let Some(graph_id) = g
        .terms
        .iter()
        .position(|t| t.value.as_deref() == Some(GRAPH_NORM_CLAIMS))
    else {
        return None;
    };

    let quads: Vec<_> = g
        .quads
        .iter()
        .filter(|&&(_, _, _, gname)| gname == Some(graph_id))
        .map(|&(s, p, o, _)| (s, p, o, None))
        .collect();
    if quads.is_empty() {
        return None;
    }

    Some(Graph {
        terms: g.terms,
        quads,
        ..Graph::default()
    })
}

/// The merged SHACL shape union, exactly the file set `gmeow-dev validate` uses
/// (`crates/gmeow-dev-cli/src/dev_validate.rs::merged_shapes`,
/// `purrdf::shapes::shape_union::shape_files`) — `shapes/*.ttl` (minus the DSL/manifest
/// exclusions) + `generated/shapes/*.ttl` (fail-closed if empty; present post-sync) +
/// `slices/*/*/shapes.ttl`.
fn merged_shapes_ttl(root: &Path) -> String {
    let files = purrdf::shapes::shape_union::shape_files(root).unwrap_or_else(|e| {
        panic!("cannot list the SHACL shape union (requires post-sync generated/shapes/*.ttl): {e}")
    });
    let mut out = String::new();
    for file in files {
        out.push_str(
            &std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("read {}: {e}", file.display())),
        );
        out.push('\n');
    }
    out
}

/// The honest invariant: whatever the shipped `graph/norm-claims` carries (if anything)
/// SHACL-conforms against the merged shape union, and none of it is an advisory-harvested
/// `gmeow:ComplianceAssessment` (no subject IRI embeds an `advice.`-family code). Both hold
/// vacuously when the graph is absent/empty, which is the expected state for the TBox-only
/// shipped bundle (see the module docs).
#[test]
fn shipped_norm_claims_conforms_and_carries_no_advisory_compliance_assessment() {
    let root = repo_root();

    let Some(graph) = norm_claims_only_graph() else {
        // Absent or empty graph/norm-claims: the honest invariant holds vacuously — there is
        // no content, so certainly no advisory-harvested ComplianceAssessment among it.
        return;
    };

    let triples = graph_triples(&graph);

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let advisory_assessment_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &assessment_class)
        .map(|(s, _, _)| s.as_str())
        .filter(|s| s.contains(ADVICE_FAMILY))
        .collect();
    assert!(
        advisory_assessment_subjects.is_empty(),
        "the TBox-only shipped bundle's graph/norm-claims must carry NO \
         gmeow:ComplianceAssessment whose IRI embeds an `{ADVICE_FAMILY}` code; found: \
         {advisory_assessment_subjects:?}"
    );

    // Whatever non-advisory content graph/norm-claims DOES carry must still SHACL-conform
    // against the merged shape union, re-homed exactly as `make validate-gts` re-homes it.
    let shapes_ttl = merged_shapes_ttl(&root);
    let shapes =
        purrdf::shapes::engine::parse_shapes(&shapes_ttl).expect("parse the merged shape union");
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph)
        .expect("build an RdfDataset from the re-homed graph/norm-claims quads");
    let report = purrdf::shapes::engine::validate_dataset(&dataset, &shapes)
        .expect("run SHACL over the re-homed graph/norm-claims dataset");
    assert!(
        report.conforms,
        "the shipped graph/norm-claims dataset must SHACL-conform as emitted; violations: {:#?}",
        report.results
    );
}

// WHOLE-BUNDLE (cross-graph-typing) SHACL conformance of the shipped norm-claims subjects is the
// remit of `make validate-gts` (`gmeow-dev validate --gts` → `ValidationRun::run` over
// `flattened_dataset_from_bytes`), NOT of a unit test here. That is a deliberate boundary, not a
// gap:
//   * The Observation family imposes value-TYPE shapes on `gmeow:vantage`
//     (`Observation-shape`: `sh:path gmeow:vantage ; sh:class gmeow:Entity`). Deciding whether a
//     `gmeow:vantage` value (a `gmeow:Standpoint`) satisfies `sh:class gmeow:Entity` requires the
//     COMPLETE `rdfs:subClassOf` TBox closure (`Standpoint ⊑ … ⊑ Entity`) — a whole-bundle fact. A
//     raw `validate_dataset` over the full 23M-triple flattened bundle is O(all shapes × all
//     nodes) and does not terminate within any test budget; a dependency-closure subset is WORSE
//     THAN USELESS — it makes a node a `sh:class`-checked `Observation` target without its full
//     superclass chain, so it FALSE-POSITIVES. There is no cheap, correct middle: `validate_dataset`
//     has no focus-node scoping, so a correct whole-bundle-semantics check IS the whole-bundle
//     validation, which is exactly what `make validate-gts` already runs.
//   * The isolated-fragment test above fully covers the D4 constraints that are actually authored
//     (`ComplianceAssessment{Vantage,Verdict,AssessedEvent,AssessedNorm}Constraint` — all
//     `logic:directType`-guarded property-PRESENCE `sh:minCount` shapes over the emitter's
//     self-typed A-box; no `sh:class` among them).
