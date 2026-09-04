// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ORPHAN-ZERO vocabulary-ownership gate for the self-hosting documentation
//! projection.
//!
//! `crates/docs/src/rdf.rs::to_gmeow_rdf` projects the doc model into the
//! `gmeow:graph/documentation` named graph as generated A-Box. Every `gmeow:`
//! class it types with `rdf:type`, every `gmeow:` predicate it emits, and every
//! `gmeow:DocEvidenceKind` individual it references MUST have a TBox declaration
//! in SOME slice `.ttl` — otherwise the documentation graph names an ownerless
//! orphan the reader cannot resolve (the doc-layer analogue of an unowned term).
//!
//! This test DERIVES the emitted term set from the projection output itself — it
//! runs `to_gmeow_rdf` over both the live production model and a synthetic model
//! that fires every evidence-kind code path, re-parses the emitted N-Quads, and
//! collects the `gmeow:` predicate IRIs, `rdf:type` object IRIs, and
//! `gmeow:docEvidenceKind` object IRIs. It then walks every slice `module.ttl`
//! (plus the root ontology) for `gmeow:` subjects carrying an `rdf:type`
//! declaration, and HARD-FAILS listing any emitted term with no declaration. It
//! is NOT a hardcoded list: a new emitted `gmeow:` term added to the projector
//! without a matching TBox declaration reds this gate. This mirrors the
//! authored-claim-verified-against-code discipline of
//! `crates/validate/src/constitution.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_docs::{
    DiagnosticsDigest, DocCompetency, DocDiagFinding, DocFixture, DocFixtureKind, DocFlowEdge,
    DocPipeline, DocStage, DocTerm, DocTermCategory, DocsModel, TermLossDigest, TermLossRow,
    to_gmeow_rdf,
};
use purrdf::slice::rdf_query::{Dataset, GraphSel, Object, Subject};

mod common;

/// The GMEOW namespace IRI prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DOC_EVIDENCE_KIND: &str = "https://blackcatinformatics.ca/gmeow/docEvidenceKind";

/// The `gmeow:`-local name of an IRI IFF it names a vocabulary TERM — i.e. the
/// IRI is under the GMEOW namespace and its local part carries no further `/`
/// (so slice IRIs `.../slices/…`, the doc-graph instance subjects
/// `.../documentation/term/…`, and the named graph itself are excluded; only
/// bare-local vocabulary terms like `DocumentedTerm` / `documents` survive).
fn gmeow_term_local(iri: &str) -> Option<&str> {
    iri.strip_prefix(GMEOW).filter(|local| !local.contains('/'))
}

/// The set of `gmeow:` vocabulary terms the projection EMITS into
/// `graph/documentation`: every predicate IRI, every `rdf:type` object IRI, and
/// every `gmeow:docEvidenceKind` object IRI in the GMEOW namespace. Derived by
/// re-parsing the emitted N-Quads — never a hardcoded list.
fn emitted_terms(nq: &str) -> BTreeSet<String> {
    let ds = Dataset::parse(nq.as_bytes(), "application/n-quads", None, "to_gmeow_rdf output")
        .expect("to_gmeow_rdf must emit valid, round-trippable N-Quads");
    let mut out = BTreeSet::new();
    // The projection lives entirely in the gmeow:graph/documentation NAMED graph,
    // so span every graph (the default for_each_quad sees the default graph only).
    ds.graph(GraphSel::Any).for_each_quad(|_s, p, o, _g| {
        // Every predicate the projection emits.
        if let Some(local) = gmeow_term_local(p) {
            out.insert(local.to_string());
        }
        // rdf:type object classes, and docEvidenceKind object individuals: both
        // are `gmeow:` resources the emitter names and this TBox must own.
        if (p == RDF_TYPE || p == DOC_EVIDENCE_KIND)
            && let Object::Named(iri) = &o
            && let Some(local) = gmeow_term_local(iri)
        {
            out.insert(local.to_string());
        }
    });
    out
}

/// A synthetic model whose single documented term genuinely carries EVERY
/// evidence kind (competency, diagnostics, fixture, loss, provenance), so the
/// projection exercises every per-kind predicate the live model may not
/// currently populate (the real repo's diagnostics-to-term join is empty today).
/// Mirrors `crates/docs/tests/doc_evidence.rs::evidence_rich_model`.
fn evidence_rich_model() -> DocsModel {
    let cat = format!("{GMEOW}Cat");

    let term = DocTerm {
        iri: cat.clone(),
        curie: "gmeow:Cat".to_string(),
        label: Some("Cat".to_string()),
        definition: Some("A small domesticated felid.".to_string()),
        category: DocTermCategory::Class,
        owner_slice: format!("{GMEOW}slice/zoo"),
        ..Default::default()
    };

    let fixture = DocFixture {
        slice: format!("{GMEOW}slice/zoo"),
        logical_path: "tests/conformance-fixtures/cat-ok.ttl".to_string(),
        title: "A conforming cat".to_string(),
        text: "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n".to_string(),
        kind: DocFixtureKind::Wellformed,
        terms_referenced: vec!["gmeow:Cat".to_string()],
        expected_outcome: Some("conforms".to_string()),
        violation_code: None,
        rationale: None,
        catalog_slug: None,
    };

    let competency = DocCompetency {
        iri: format!("{GMEOW}cq/cats-are-animals"),
        rationale: Some("Every cat must classify as an animal.".to_string()),
        query_file: None,
        query_text: Some("SELECT ?c WHERE { ?c a gmeow:Cat }".to_string()),
        exact_rows: None,
        expected_row_count: None,
        expected_rows: Vec::new(),
        exercises: vec![cat.clone()],
        owner_slice: format!("{GMEOW}slice/zoo"),
    };

    let mut diag_by_term = std::collections::BTreeMap::new();
    diag_by_term.insert(
        cat.clone(),
        vec![DocDiagFinding {
            code: "shacl.MinCountConstraintComponent".to_string(),
            severity: "error".to_string(),
            category: "shacl".to_string(),
            message: "cat is missing a required owner".to_string(),
            slice_iri: Some(format!("{GMEOW}slice/zoo")),
            help_uri: None,
        }],
    );
    let diagnostics = DiagnosticsDigest {
        by_term: diag_by_term,
        by_slice: std::collections::BTreeMap::new(),
        total: 1,
    };

    let mut loss_by_term = std::collections::BTreeMap::new();
    loss_by_term.insert(
        cat.clone(),
        vec![TermLossRow {
            target: format!("property-path:{GMEOW}hasOwner"),
            preservation_kind: "SoundUnderApproximation".to_string(),
            complexity_class: "PTIME".to_string(),
            lossy_drops: vec!["owl:qualifiedCardinality".to_string()],
        }],
    );
    let term_loss = TermLossDigest {
        by_term: loss_by_term,
        total_property_path_rows: 1,
    };

    let pipeline = DocPipeline {
        stages: vec![
            DocStage {
                iri: format!("{GMEOW}stage-source-load"),
                consumes: Vec::new(),
                ..Default::default()
            },
            DocStage {
                iri: format!("{GMEOW}stage-docs-render"),
                consumes: vec![format!("{GMEOW}stage-source-load")],
                ..Default::default()
            },
        ],
        edges: vec![DocFlowEdge {
            from: format!("{GMEOW}stage-source-load"),
            to: format!("{GMEOW}stage-docs-render"),
            flow_entities: Vec::new(),
        }],
        goal: None,
        success_mode: None,
    };

    DocsModel {
        title: "Evidence-rich model".to_string(),
        version: "2".to_string(),
        terms: vec![term],
        fixtures: vec![fixture],
        competencies: vec![competency],
        diagnostics: Some(diagnostics),
        term_loss: Some(term_loss),
        pipeline: Some(pipeline),
        available_languages: vec!["english".to_string()],
        ..Default::default()
    }
}

/// Every `slices/**/module.ttl` under the repo, plus the root ontology.
fn declaration_files(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("ontology/gmeow.ttl")];
    let mut stack = vec![root.join("slices")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "module.ttl") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The set of `gmeow:` vocabulary terms DECLARED across the slice modules and
/// the root ontology: every `gmeow:` subject that carries an `rdf:type`
/// assertion (a TBox declaration — `owl:Class` / `owl:*Property` / a
/// `gmeow:DocEvidenceKind` individual / …). A file that fails to parse cannot
/// carry a declaration this gate needs (the doc, kernel, logic, and versions
/// modules parse), so it is skipped rather than aborting the whole scan.
fn declared_terms(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in declaration_files(root) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(ds) = Dataset::parse_turtle(&bytes, None, &path.display().to_string()) else {
            continue;
        };
        ds.graph(GraphSel::Any).for_each_quad(|s, p, _o, _g| {
            if p != RDF_TYPE {
                return;
            }
            if let Subject::Named(iri) = &s
                && let Some(local) = gmeow_term_local(iri)
            {
                out.insert(local.to_string());
            }
        });
    }
    out
}

/// ORPHAN-ZERO: every `gmeow:` term the documentation projection emits into
/// `graph/documentation` has a TBox declaration in some slice module. A newly
/// emitted, undeclared `gmeow:` term reds this gate.
#[test]
fn every_emitted_documentation_term_is_declared() {
    let root = common::repo_root();

    // Union the emitted surface of the LIVE production model (which carries
    // slices, concerns, and mapping sets → the Documented{Slice,Concern,MappingSet}
    // record classes) with the synthetic evidence-rich model (which fires every
    // gmeow:DocEvidence per-kind predicate, including the diagnostics/loss paths
    // the real repo does not populate today) — so the emitted set is the fullest
    // faithful projection surface, not a subset.
    let mut emitted = emitted_terms(&to_gmeow_rdf(
        &common::cached_model(),
        &std::collections::BTreeMap::new(),
    ));
    emitted.extend(emitted_terms(&to_gmeow_rdf(
        &evidence_rich_model(),
        &std::collections::BTreeMap::new(),
    )));

    // Non-vacuity: the projection must genuinely emit doc vocabulary, else the
    // subset check below would be trivially satisfiable and the gate meaningless.
    assert!(
        emitted.len() >= 10,
        "the documentation projection emitted only {} gmeow: terms — expected the \
         full doc vocabulary; the emitter surface may have collapsed",
        emitted.len()
    );
    for anchor in [
        "DocumentedTerm",
        "DocEvidence",
        "documents",
        "docGroundedBy",
    ] {
        assert!(
            emitted.contains(anchor),
            "the projection did not emit the anchor term gmeow:{anchor} — the \
             emitter surface changed; re-derive the expected set"
        );
    }

    let declared = declared_terms(&root);
    assert!(
        !declared.is_empty(),
        "no gmeow: declarations discovered under slices/ — the module scan is vacuous"
    );

    let orphans: Vec<&String> = emitted.difference(&declared).collect();
    assert!(
        orphans.is_empty(),
        "ORPHAN-ZERO VIOLATION: the documentation projection emits {} gmeow: term(s) \
         into graph/documentation with NO TBox declaration in any slice module:\n  {}\n\
         Every emitted gmeow: term must be declared (owner: slices/core/documentation \
         for the gmeow:doc* vocabulary, or the reusing slice for a borrowed term).",
        orphans.len(),
        orphans
            .iter()
            .map(|o| format!("gmeow:{o}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}
