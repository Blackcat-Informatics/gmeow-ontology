// SPDX-License-Identifier: AGPL-3.0-only

//! Structural conformance for the embedding-projection slice.
//!
//! The four structural checks a boolean SHACL `ASK` cannot reach:
//! * **F1** (AC2 non-duplication): the module REUSES graphrag/kernel terms and never
//!   REDECLARES them (no `rdfs:isDefinedBy` the slice on a reused term), while its own
//!   new terms are slice-owned; plus the boundary axioms
//!   (`gmeow:SimilarityObservation logic:disjointWith gmeow:RetrievalEvent`, and
//!   `gmeow:EmbeddingProjection` is NOT `logic:subClassOf gmeow:Embedding`).
//! * **F2** (AC5 four-way separation): the four operational categories — the pack-level
//!   projection, the similarity/retrieval observations, the derived index, and the
//!   per-object source embedding — are pairwise non-conflated classes (no
//!   subclass/equivalent edge among them).
//! * **F3** (AC6 predicate purity): every `gmeow:`/`logic:`/`math:`/`lang:` predicate the
//!   worked example uses resolves to a shipped term declared in the merged ontology.
//! * **CT11** (RDF-1.2-only): the module and its example carry no banned legacy RDF-1.1
//!   container vocabulary (`rdf:Bag`/`rdf:Seq`/`rdf:Alt`/`rdf:li`/`rdf:_N`); the shipped,
//!   base-quad-foldable `rdf:Statement` reification is deliberately NOT flagged.

use crate::conformance_support::*;

use std::path::PathBuf;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";

// The boundary / separation axioms are authored in the canonical `logic:` grounding
// forms (Principle 17: `logic:` is canonical; OWL/RDFS are generated projections). The
// `GraphStore::ontology()` source store parses the authored modules directly, so it
// carries `logic:subClassOf` / `logic:disjointWith` / `logic:equivalentClass` — the OWL
// projection is materialized downstream by the pipeline, not present in this store.
const LOGIC_SUBCLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";
const LOGIC_EQUIVALENT_CLASS: &str = "https://blackcatinformatics.ca/logic/equivalentClass";
const LOGIC_DISJOINT_WITH: &str = "https://blackcatinformatics.ca/logic/disjointWith";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/embedding-projection";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn slice_module_path() -> PathBuf {
    repo_root().join("slices/extensions/embedding-projection/module.ttl")
}

fn example_path() -> PathBuf {
    repo_root().join("slices/extensions/embedding-projection/examples/purremb-bookshelf.ttl")
}

/// F1 — non-duplication (AC2) + the boundary axioms.
#[gmeow_test_batch_macros::batch_test]
fn f1_non_duplication_and_boundary_axioms() {
    let g = GraphStore::ontology();

    // Boundary axioms are present in the merged ontology.
    assert!(
        g.has(
            Some(&gm("SimilarityObservation")),
            Some(LOGIC_DISJOINT_WITH),
            Some(&gm("RetrievalEvent"))
        ),
        "F1: gmeow:SimilarityObservation must be logic:disjointWith gmeow:RetrievalEvent"
    );
    assert!(
        !g.has(
            Some(&gm("EmbeddingProjection")),
            Some(LOGIC_SUBCLASS_OF),
            Some(&gm("Embedding"))
        ),
        "F1: gmeow:EmbeddingProjection must NOT be logic:subClassOf gmeow:Embedding \
         (it AGGREGATES via gmeow:aggregatesEmbedding)"
    );
    assert!(
        g.has(Some(&gm("aggregatesEmbedding")), None, None),
        "F1: gmeow:aggregatesEmbedding (the non-duplication seam) must be declared"
    );

    // Non-duplication: the slice references reused graphrag/kernel/notation terms but
    // never redeclares them under the slice authority. Load the slice module ALONE:
    // a reused term must NOT carry rdfs:isDefinedBy <slice>, while a slice-owned NEW
    // term must.
    let module = GraphStore::parse_ttl_file(&slice_module_path());

    let reused = [
        "Embedding",
        "RetrievalEvent",
        "VectorIndex",
        "DistanceMetric",
        "InformationObject",
        "Observation",
        "SensitivityLevel",
        "contentDigest",
        "embeddingModel",
        "gtsHeadId",
        "hasSensitivity",
        "hasDisclosurePolicy",
        "indexAlgorithm",
        "indexParameters",
        "wasDerivedFrom",
        "wasGeneratedBy",
    ];
    for t in reused {
        assert!(
            !module.has(Some(&gm(t)), Some(RDFS_IS_DEFINED_BY), Some(SLICE_IRI)),
            "F1: reused term gmeow:{t} must NOT be redeclared \
             (rdfs:isDefinedBy the embedding-projection slice)"
        );
    }

    let owned = [
        "EmbeddingProjection",
        "VectorSpaceContract",
        "EmbeddingFamily",
        "SimilarityObservation",
        "DerivedVectorIndex",
        "VectorTarget",
        "TargetSet",
        "ProfileSurface",
        "DeclassificationAct",
        "ExternalBinding",
    ];
    for t in owned {
        assert!(
            module.has(Some(&gm(t)), Some(RDFS_IS_DEFINED_BY), Some(SLICE_IRI)),
            "F1: slice-owned term gmeow:{t} must carry rdfs:isDefinedBy the slice"
        );
        // A slice-owned term is not merely bound to the slice authority: it MUST also
        // carry human-readable annotations — an rdfs:label and a skos:definition — so the
        // vocabulary is documented, not just declared.
        assert!(
            module.has(Some(&gm(t)), Some(RDFS_LABEL), None),
            "F1: slice-owned term gmeow:{t} must carry an rdfs:label"
        );
        assert!(
            module.has(Some(&gm(t)), Some(SKOS_DEFINITION), None),
            "F1: slice-owned term gmeow:{t} must carry a skos:definition"
        );
    }
}

/// F2 — four-way separation (AC5): the four categories are pairwise distinct classes,
/// none a subclass or equivalent of another.
#[gmeow_test_batch_macros::batch_test]
fn f2_four_way_separation() {
    let g = GraphStore::ontology();

    // The pack-level projection, the two observation kinds, and the derived index.
    let operational = [
        "EmbeddingProjection",
        "SimilarityObservation",
        "RetrievalEvent",
        "DerivedVectorIndex",
    ];
    for a in operational {
        for b in operational {
            if a == b {
                continue;
            }
            assert!(
                !g.has(Some(&gm(a)), Some(LOGIC_SUBCLASS_OF), Some(&gm(b))),
                "F2: gmeow:{a} must not be logic:subClassOf gmeow:{b}"
            );
            assert!(
                !g.has(Some(&gm(a)), Some(LOGIC_EQUIVALENT_CLASS), Some(&gm(b))),
                "F2: gmeow:{a} must not be logic:equivalentClass gmeow:{b}"
            );
        }
    }

    // The source-artifact category (the per-object source vector gmeow:Embedding) is a
    // fourth distinct category: neither it nor the projection subsumes or equals the
    // other.
    for (a, b) in [
        ("EmbeddingProjection", "Embedding"),
        ("Embedding", "EmbeddingProjection"),
    ] {
        assert!(
            !g.has(Some(&gm(a)), Some(LOGIC_SUBCLASS_OF), Some(&gm(b))),
            "F2: gmeow:{a} must not be logic:subClassOf gmeow:{b}"
        );
        assert!(
            !g.has(Some(&gm(a)), Some(LOGIC_EQUIVALENT_CLASS), Some(&gm(b))),
            "F2: gmeow:{a} must not be logic:equivalentClass gmeow:{b}"
        );
    }
}

/// F3 — predicate purity (AC6): every grounding-namespace predicate the worked example
/// uses is a declared, shipped term in the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn f3_predicate_purity() {
    let g = GraphStore::ontology();
    let example = GraphStore::parse_ttl_file(&example_path());
    let (_vars, rows) = example.select(&[], "SELECT DISTINCT ?p WHERE { ?s ?p ?o }");

    let mut checked = 0usize;
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(p) = term.as_iri() else {
            continue;
        };
        // Only the gmeow:/logic:/math:/lang: predicates are subject to the declaration
        // check; rdf/rdfs/owl/xsd/skos are standard external vocabularies.
        if p.starts_with(GMEOW)
            || p.starts_with(LOGIC)
            || p.starts_with(MATH)
            || p.starts_with(LANG)
        {
            assert!(
                g.has(Some(p), None, None),
                "F3: predicate <{p}> used in the example does not resolve to a shipped \
                 term declared in the merged ontology"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 20,
        "F3: expected the example to exercise many grounding-namespace predicates, \
         only {checked} were checked (the sweep is not reaching the example)"
    );
}

/// CT11 — RDF-1.2 only: the slice module and its example carry no banned legacy RDF-1.1
/// container vocabulary. rdf:Statement reification is the shipped foldable form and is
/// not flagged.
#[gmeow_test_batch_macros::batch_test]
fn ct11_rdf12_only() {
    const ASK: &str = r#"
        PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
        ASK {
          { ?s a rdf:Bag } UNION { ?s a rdf:Seq } UNION { ?s a rdf:Alt }
          UNION { ?s rdf:li ?o }
          UNION { ?s ?pm ?o . FILTER(STRSTARTS(STR(?pm), "http://www.w3.org/1999/02/22-rdf-syntax-ns#_")) }
        }"#;

    for path in [slice_module_path(), example_path()] {
        let g = GraphStore::parse_ttl_file(&path);
        assert!(
            !g.ask(&[], ASK),
            "CT11: {} must contain no banned legacy RDF-1.1 container constructs \
             (rdf:Bag/rdf:Seq/rdf:Alt/rdf:li/rdf:_N)",
            path.display()
        );
    }
}
