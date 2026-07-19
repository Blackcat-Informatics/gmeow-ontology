// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deliverable D4 (#763 Task 6, Part 1) — Completion-Adversary F4 falsifiable check.
//!
//! `gmeow-dev validate --gts` (`make validate-gts`) SHACL-validates the SHIPPED bundle
//! through `ValidationRun::run(gts_bytes: Some(..))` →
//! `gmeow_validate::store::dataset_from_gts` → `purrdf::gts::flattened_dataset_from_bytes`,
//! which re-homes EVERY named graph (including `graph/norm-claims`) onto the default
//! graph before the merged-SHACL phase runs `store::shacl_validate_dataset` over that
//! same flattened dataset (`crates/validate/src/validate_all.rs`, phase "merged-shacl").
//! So the emitted `gmeow:ComplianceAssessment` nodes in `graph/norm-claims` ARE already
//! part of the dataset `make validate-gts` SHACL-checks — not merely present in the
//! bundle but structurally invisible to the validator.
//!
//! This test does not merely assert that fact; it PROVES the Task-1 mandatory-`vantage`
//! restriction on `gmeow:ComplianceAssessment` (`slices/extensions/norms/module.ttl`,
//! `owl:Restriction` on `gmeow:vantage` with `owl:minQualifiedCardinality 1`) actually
//! FIRES as a SHACL violation on the shipped assessment: it validates the well-formed
//! `graph/norm-claims` subgraph (must CONFORM) and a mutant with the assessment's single
//! `gmeow:vantage` triple dropped (must NOT conform). If the Task-1 restriction were ever
//! deleted, no derived shape would fire, the mutant would incorrectly conform, and this
//! test would fail — the non-vacuity proof Completion-Adversary F4 requires.
//!
//! Like `norm_claims_bundle.rs`, this test `.expect()`s the committed bundle AND the
//! post-sync `generated/shapes/*.ttl` shape union (`purrdf::shapes::shape_union::shape_files`
//! fails closed when `generated/shapes/` is empty) — it runs green only after `make sync`.

use std::path::{Path, PathBuf};

use purrdf::gts::model::Graph;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// The demonstrator advisory code both advice wings project (`crates/validate/src/advisory.rs`
/// `Advisory::demo`), embedded in the `graph/norm-claims` claim's content-addressed IRIs
/// (`NORM_CLAIMS_BASE_IRI`).
const ADVICE_CODE: &str = "advice.tier.active";

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

/// `(subject, predicate, object)` display triples of every quad in `g` (already filtered to
/// one graph and re-homed to the default graph by [`norm_claims_only_graph`]).
fn graph_triples(g: &Graph) -> Vec<(String, String, String)> {
    g.quads
        .iter()
        .map(|&(s, p, o, _)| (term_value(g, s), term_value(g, p), term_value(g, o)))
        .collect()
}

/// Build a native GTS [`Graph`] carrying ONLY the `graph/norm-claims` quads of the shipped
/// bundle, re-homed onto the default graph (`purrdf::shapes::engine::validate_dataset`
/// validates the dataset's quads regardless of graph slot, but re-homing to the default
/// graph mirrors exactly what `dataset_from_gts` / `flattened_dataset_from_bytes` does on
/// the real `make validate-gts` path — see the module docs above).
///
/// `omit`, when `Some((subject_iri, predicate_iri))`, drops the one quad matching that
/// exact `(subject, predicate)` pair — the mutation lever for the falsifiable
/// conforms/does-not-conform pair.
fn norm_claims_only_graph(omit: Option<(&str, &str)>) -> Graph {
    let g = read_committed_gts_graph();
    let graph_id = g
        .terms
        .iter()
        .position(|t| t.value.as_deref() == Some(GRAPH_NORM_CLAIMS))
        .expect("graph/norm-claims graph-name term must be interned in the shipped bundle");

    let quads: Vec<_> = g
        .quads
        .iter()
        .filter(|&&(s, p, _, gname)| {
            if gname != Some(graph_id) {
                return false;
            }
            match omit {
                Some((subject, predicate)) => {
                    !(term_value(&g, s) == subject && term_value(&g, p) == predicate)
                }
                None => true,
            }
        })
        .map(|&(s, p, o, _)| (s, p, o, None))
        .collect();

    assert!(
        !quads.is_empty(),
        "graph/norm-claims must carry a non-empty triple set in the shipped bundle"
    );

    Graph {
        terms: g.terms,
        quads,
        ..Graph::default()
    }
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

/// The falsifiable pair (Completion-Adversary F4): the shipped `graph/norm-claims`
/// dataset SHACL-conforms as-is, and stops conforming the instant the Task-1 mandatory
/// `gmeow:vantage` triple of the demonstrator `ComplianceAssessment` is dropped.
#[test]
fn shipped_norm_claims_shacl_conforms_and_fails_without_mandatory_vantage() {
    let root = repo_root();
    let shapes_ttl = merged_shapes_ttl(&root);
    let shapes =
        purrdf::shapes::engine::parse_shapes(&shapes_ttl).expect("parse the merged shape union");

    // Locate the demonstrator ComplianceAssessment subject and its gmeow:vantage predicate
    // from the well-formed graph, the same way `norm_claims_bundle.rs` does.
    let conforming_graph = norm_claims_only_graph(None);
    let triples = graph_triples(&conforming_graph);

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let assessment = triples
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &assessment_class)
        .map(|(s, _, _)| s.clone())
        .find(|s| s.contains(ADVICE_CODE))
        .unwrap_or_else(|| {
            panic!(
                "no gmeow:ComplianceAssessment subject embedding `{ADVICE_CODE}` in graph/norm-claims"
            )
        });
    let vantage_pred = format!("{GMEOW}vantage");
    let vantage_count = triples
        .iter()
        .filter(|(s, p, _)| s == &assessment && p == &vantage_pred)
        .count();
    assert_eq!(
        vantage_count, 1,
        "the {ADVICE_CODE} ComplianceAssessment must carry exactly one gmeow:vantage triple \
         before mutation, found {vantage_count}"
    );

    // Positive half: the well-formed graph/norm-claims dataset SHACL-conforms.
    let conforming_dataset = purrdf::gts::dataset_from_gts_graph(&conforming_graph)
        .expect("build an RdfDataset from the well-formed graph/norm-claims quads");
    let conforming_report = purrdf::shapes::engine::validate_dataset(&conforming_dataset, &shapes)
        .expect("run SHACL over the well-formed graph/norm-claims dataset");
    assert!(
        conforming_report.conforms,
        "the shipped graph/norm-claims dataset must SHACL-conform as emitted; violations: {:#?}",
        conforming_report.results
    );

    // Negative half: dropping the assessment's ONLY gmeow:vantage triple must produce a
    // SHACL violation — the non-vacuity proof that the Task-1 mandatory-vantage
    // restriction (`gmeow:ComplianceAssessment rdfs:subClassOf [ owl:onProperty
    // gmeow:vantage ; owl:minQualifiedCardinality 1 ; owl:onClass owl:Thing ]`,
    // `slices/extensions/norms/module.ttl`) is genuinely enforced by the derived shape.
    // If that restriction were ever deleted, no shape would fire here, this mutant would
    // incorrectly conform, and this assertion would fail.
    let mutated_graph = norm_claims_only_graph(Some((assessment.as_str(), vantage_pred.as_str())));
    let mutated_dataset = purrdf::gts::dataset_from_gts_graph(&mutated_graph)
        .expect("build an RdfDataset from the vantage-dropped graph/norm-claims quads");
    let mutated_report = purrdf::shapes::engine::validate_dataset(&mutated_dataset, &shapes)
        .expect("run SHACL over the vantage-dropped mutant");
    assert!(
        !mutated_report.conforms,
        "dropping gmeow:vantage from the {ADVICE_CODE} ComplianceAssessment must produce a \
         SHACL violation (Task-1's mandatory-vantage restriction on gmeow:ComplianceAssessment) \
         — instead the mutant conformed, which means the restriction is not actually enforced \
         by the derived shape (a vacuous non-vacuity proof)"
    );
}
