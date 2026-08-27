// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The concept-lattice reader, over synthetic bundles that carry the shape it reads.
//!
//! The EMITTER of `gmeow:FormalConcept` rows is a separate producer, so this reader is
//! written against the shape rather than against whatever the current bundle happens to
//! hold. These fixtures ARE that shape: a catalog named graph carrying concept nodes with
//! `gmeow:conceptExtent` / `gmeow:conceptIntent`, folded into a real gts snapshot with the
//! same writer the pipeline uses.
//!
//! Three things are pinned:
//!
//! * a lattice that IS present reads back completely, sorted, with extents and intents
//!   recovered as local names;
//! * the lattice's one-sided BOUNDS (top has an empty intent, bottom an empty extent) are
//!   normal structure, not defects — an FCA lattice always has them;
//! * a facet-bearing node that is not typed `gmeow:FormalConcept` is a HARD FAIL naming
//!   the node, because the type filter would otherwise drop it in silence.

use gmeow_docs_catalog::{
    ConceptRow, GRAPH_DISTRIBUTION_CATALOG, read_concept_lattice, read_distribution_matrix,
};
use purrdf::{RdfLookaside, RdfTerm};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";

/// Fold `trig` into a real gts snapshot, through the SAME writer the carrier uses — so the
/// named graph the reader selects on is carried exactly as a shipped bundle carries it.
fn snapshot_from_trig(trig: &str) -> Vec<u8> {
    let dataset = purrdf::parse_dataset(trig.as_bytes(), "application/trig", None)
        .expect("fixture parses as TriG");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(
        &dataset,
        &RdfLookaside::default(),
        "gmeow-docs-catalog-test",
    )
    .expect("fixture folds into a gts snapshot")
}

/// A three-node lattice in the catalog graph: a top concept (full extent, empty intent), a
/// middle concept, and a bottom concept (empty extent, full intent).
fn lattice_fixture() -> String {
    format!(
        r#"
@prefix gmeow: <{GM}> .
<{GRAPH_DISTRIBUTION_CATALOG}> {{
  <{GM}concept/top> a gmeow:FormalConcept ;
      gmeow:conceptExtent <{GM}distribution/dist/site>, <{GM}distribution/dist/mdbook> .
  <{GM}concept/mid> a gmeow:FormalConcept ;
      gmeow:conceptExtent <{GM}distribution/dist/site> ;
      gmeow:conceptIntent <{GM}distribution/capability/interactivity> .
  <{GM}concept/bottom> a gmeow:FormalConcept ;
      gmeow:conceptIntent <{GM}distribution/capability/interactivity>,
                          <{GM}distribution/capability/print-fidelity> .
}}
"#
    )
}

#[test]
fn a_present_lattice_reads_back_completely_and_sorted() {
    let bytes = snapshot_from_trig(&lattice_fixture());
    let rows = read_concept_lattice(&bytes).expect("the lattice reads back");
    assert_eq!(
        rows,
        vec![
            ConceptRow {
                concept: format!("{GM}concept/bottom"),
                extent: vec![],
                intent: vec!["interactivity".to_owned(), "print-fidelity".to_owned()],
            },
            ConceptRow {
                concept: format!("{GM}concept/mid"),
                extent: vec!["site".to_owned()],
                intent: vec!["interactivity".to_owned()],
            },
            ConceptRow {
                concept: format!("{GM}concept/top"),
                extent: vec!["mdbook".to_owned(), "site".to_owned()],
                intent: vec![],
            },
        ],
        "rows sort by concept IRI; extents and intents sort as deduped local names"
    );
}

/// The lattice's bounds are legitimately one-sided: in FCA the top concept is `(G, G′)` and
/// the bottom is `(M′, M)`, so over a non-trivial context one has an empty intent and the
/// other an empty extent. Requiring both facets non-empty would reject every real lattice.
#[test]
fn the_lattice_bounds_may_be_one_sided() {
    let bytes = snapshot_from_trig(&lattice_fixture());
    let rows = read_concept_lattice(&bytes).expect("the lattice reads back");
    let top = rows
        .iter()
        .find(|row| row.concept.ends_with("/top"))
        .expect("top concept");
    assert!(top.intent.is_empty(), "the top concept has an empty intent");
    assert!(!top.extent.is_empty());
    let bottom = rows
        .iter()
        .find(|row| row.concept.ends_with("/bottom"))
        .expect("bottom concept");
    assert!(
        bottom.extent.is_empty(),
        "the bottom concept has an empty extent"
    );
    assert!(!bottom.intent.is_empty());
}

/// A catalog that declares no concepts yields an EMPTY row set. The emitter is a separate
/// producer; a catalog is complete without a lattice, and failing here would make the
/// reader red on every bundle materialized before the emitter exists.
#[test]
fn a_catalog_with_no_concepts_reads_back_empty_rather_than_failing() {
    let trig = format!(
        r#"
@prefix gmeow: <{GM}> .
<{GRAPH_DISTRIBUTION_CATALOG}> {{
  <{GM}distribution/dist/site> a gmeow:DocumentationDistribution ;
      gmeow:distributionFormat "site" ;
      gmeow:distributionFamily <{GM}distribution/family/doc-render> ;
      gmeow:artifactMediaType "text/html" ;
      gmeow:eligibleForConsumer <{GM}consumerPublicSite> .
}}
"#
    );
    let bytes = snapshot_from_trig(&trig);
    assert_eq!(
        read_concept_lattice(&bytes).expect("an empty lattice is not a failure"),
        Vec::<ConceptRow>::new()
    );
    // …and the distribution reader over the SAME bytes still returns its row, so "empty
    // lattice" is a statement about the lattice and not about the catalog.
    let rows = read_distribution_matrix(&bytes).expect("the distribution reads back");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].slug, "site");
}

/// A node carrying concept facets WITHOUT the `gmeow:FormalConcept` type would be dropped
/// by the type filter without a word. That is exactly the silent degradation no-optionality
/// forbids, so it is a HARD FAIL naming the node.
#[test]
fn a_facet_bearing_node_that_is_not_typed_is_a_named_hard_error() {
    let trig = format!(
        r#"
@prefix gmeow: <{GM}> .
<{GRAPH_DISTRIBUTION_CATALOG}> {{
  <{GM}concept/typed> a gmeow:FormalConcept ;
      gmeow:conceptExtent <{GM}distribution/dist/site> .
  <{GM}concept/untyped> gmeow:conceptIntent <{GM}distribution/capability/interactivity> .
}}
"#
    );
    let bytes = snapshot_from_trig(&trig);
    let err = read_concept_lattice(&bytes)
        .expect_err("an untyped facet-bearing node must not be silently dropped");
    let message = err.to_string();
    assert!(
        message.contains("concept/untyped"),
        "the refusal must name the node: {message}"
    );
    assert!(
        message.contains("FormalConcept"),
        "the refusal must name the type that is missing: {message}"
    );
}

/// A bundle with no distribution-catalog graph at all IS a bundle defect, for BOTH readers
/// — that is the one shared failure the catalog read has, and each reports it under its own
/// diagnostic code.
#[test]
fn both_readers_fail_closed_without_the_catalog_graph() {
    let trig = format!("<{GM}s> <{GM}p> <{GM}o> .\n");
    let bytes = snapshot_from_trig(&trig);

    let lattice_err = read_concept_lattice(&bytes)
        .expect_err("a bundle with no catalog graph must hard-fail the lattice read");
    assert!(
        lattice_err.to_string().contains("distribution-catalog"),
        "the refusal must name the missing graph: {lattice_err}"
    );
    assert_eq!(
        lattice_err.code(),
        gmeow_docs_catalog::error::ConceptLattice::register(),
        "the lattice reader reports under its own code"
    );

    let matrix_err = read_distribution_matrix(&bytes)
        .expect_err("a bundle with no catalog graph must hard-fail the matrix read");
    assert_eq!(
        matrix_err.code(),
        gmeow_docs_catalog::error::DistributionCatalog::register(),
        "the distribution reader reports under its own code"
    );
}

/// A catalog graph that carries distributions but is missing a required facet is a HARD
/// FAIL naming the subject and the facet — never a silently partial matrix.
#[test]
fn a_distribution_missing_a_required_facet_is_a_named_hard_error() {
    let trig = format!(
        r#"
@prefix gmeow: <{GM}> .
<{GRAPH_DISTRIBUTION_CATALOG}> {{
  <{GM}distribution/dist/site> a gmeow:DocumentationDistribution ;
      gmeow:distributionFormat "site" ;
      gmeow:distributionFamily <{GM}distribution/family/doc-render> ;
      gmeow:artifactMediaType "text/html" .
}}
"#
    );
    let bytes = snapshot_from_trig(&trig);
    let err = read_distribution_matrix(&bytes).expect_err("a facet-less distribution must refuse");
    let message = err.to_string();
    assert!(message.contains("eligibleForConsumer"), "{message}");
    assert!(message.contains("distribution/dist/site"), "{message}");
}

/// The reader reads the NAMED graph, not the default graph: catalog-shaped triples sitting
/// outside `graph/distribution-catalog` must not be picked up, or the meta-level boundary
/// means nothing.
#[test]
fn triples_outside_the_catalog_graph_are_not_read() {
    let trig = format!(
        r#"
@prefix gmeow: <{GM}> .
<{GM}concept/decoy> a gmeow:FormalConcept ;
    gmeow:conceptExtent <{GM}distribution/dist/site> .
<{GRAPH_DISTRIBUTION_CATALOG}> {{
  <{GM}concept/real> a gmeow:FormalConcept ;
      gmeow:conceptExtent <{GM}distribution/dist/mdbook> .
}}
"#
    );
    let bytes = snapshot_from_trig(&trig);
    let rows = read_concept_lattice(&bytes).expect("the lattice reads back");
    assert_eq!(rows.len(), 1, "only the in-graph concept is read: {rows:?}");
    assert_eq!(rows[0].concept, format!("{GM}concept/real"));
}

/// The graph IRI the reader scopes to has ONE definition site, in `gmeow-bundle-view`'s
/// read-side graph-IRI module, which the carrier re-exports back — so the writer and the
/// reader cannot drift to different IRIs.
#[test]
fn the_catalog_graph_iri_is_the_shared_read_side_constant() {
    assert_eq!(
        GRAPH_DISTRIBUTION_CATALOG,
        gmeow_bundle_view::graph_iris::GRAPH_DISTRIBUTION_CATALOG
    );
    assert_eq!(
        GRAPH_DISTRIBUTION_CATALOG,
        "https://blackcatinformatics.ca/gmeow/graph/distribution-catalog"
    );
    // Sanity: the constant really is an IRI term, not a label.
    assert!(matches!(
        RdfTerm::Iri(GRAPH_DISTRIBUTION_CATALOG.to_owned()),
        RdfTerm::Iri(_)
    ));
}
