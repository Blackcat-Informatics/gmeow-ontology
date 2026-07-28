// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Corpus parity oracle for the FnO correspondence lowering.
//!
//! Proves the oxigraph-free FnO lowering reproduces the committed catalog exactly:
//! natively merge the DSL sources (for functions/cells) and the ontology sources (for
//! `rdfs:range` typing + the language-tag map) into two `DslView`s, lower via
//! `gmeow_logic_compile::projections::fno::lower_fno`, and compare the lowered
//! N-Triples to the committed `generated/projections/functions.fno.ttl` (re-parsed to a
//! normalized N-Triples line multiset). The committed artifact is the source of truth.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::fno::lower_fno;
use purrdf::slice::{ArtifactRole, SliceCatalog};
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, SerializeGraph, dataset_diff, parse_dataset,
    serialize_dataset,
};

const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

fn collect_ttl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect_ttl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
}

fn parse_turtle(bytes: &[u8]) -> Arc<RdfDataset> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None).expect("parse turtle")
}

/// The DSL source set (functions + cells): the sorted `dsl/mappings/**/*.ttl` tree,
/// then the sorted slice `Mapping` artifacts.
fn merge_dsl(root: &Path) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let mut dsl_files = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut dsl_files);
    dsl_files.sort();
    for path in &dsl_files {
        b.push_dataset(&parse_turtle(&std::fs::read(path).expect("read dsl")));
    }
    merge_slice_artifacts(root, ArtifactRole::Mapping, &mut b);
    b.freeze().expect("freeze dsl")
}

/// The ontology source set (`rdfs:range` typing): `ontology/gmeow.ttl`, then the
/// sorted slice `Module` artifacts.
fn merge_ontology(root: &Path) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.is_file() {
        b.push_dataset(&parse_turtle(&std::fs::read(&onto).expect("read ontology")));
    }
    merge_slice_artifacts(root, ArtifactRole::Module, &mut b);
    b.freeze().expect("freeze ontology")
}

fn merge_slice_artifacts(root: &Path, role: ArtifactRole, b: &mut RdfDatasetBuilder) {
    let slices_dir = root.join("slices");
    if !slices_dir.is_dir() {
        return;
    }
    let catalog = SliceCatalog::discover(&slices_dir, gmeow_ns::gmeow_slice_vocab())
        .expect("discover slices");
    let mut artifacts: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == role {
                artifacts.push((
                    record.slice_dir.join(&artifact.logical_path),
                    artifact.content.clone(),
                ));
            }
        }
    }
    artifacts.sort_by(|a, c| a.0.cmp(&c.0));
    for (_, bytes) in &artifacts {
        b.push_dataset(&parse_turtle(bytes));
    }
}

/// Re-serialize an RDF text source (Turtle or N-Triples) to canonical full-IRI
/// N-Triples so the committed Turtle catalog and the lowered N-Triples compare on the
/// same surface. Returns a SORTED `Vec` (a multiset, NOT a set): a dropped or
/// duplicated triple cannot hide.
fn ntriples_multiset(bytes: &[u8], media_type: &str) -> Vec<String> {
    let ds = parse_dataset(bytes, media_type, None).expect("parse rdf source");
    let nt = serialize_dataset(
        &ds,
        NativeRdfFormat::NTriples.media_type(),
        SerializeGraph::DefaultGraph,
    )
    .expect("serialize to N-Triples");
    let text = String::from_utf8(nt).expect("utf8 N-Triples");
    normalize_expects_index(&text)
}

/// Drop the only admitted non-semantic divergence from the parity comparison:
/// `rdfs:seeAlso` points at one implementation query for a function. The native lowering
/// chooses deterministically; the historical committed emitter chose the first cell from
/// store hash order. Everything else must compare as an RDF graph, blank nodes included.
fn without_see_also(ds: &RdfDataset) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for quad in ds.owned_quads() {
        if quad.predicate != RDFS_SEE_ALSO {
            b.push_owned_quad(&quad);
        }
    }
    for reifier in ds.owned_reifiers() {
        b.push_owned_reifier(&reifier);
    }
    for annotation in ds.owned_annotations() {
        b.push_owned_annotation(&annotation);
    }
    b.freeze().expect("filtered FnO dataset freezes")
}

/// Collapse the positional index of an `fno:expects` rdf:List blank label so that a
/// deterministic list reordering compares equal: `…_expects_0` / `…_expects_1` → a
/// single `…_expects_X`. The two catalogs hold the same parameter SET, just listed in
/// a possibly-different order; every other triple — including each `rdf:first` member
/// and the function/param/mapping nodes — must still match exactly. Blank-node labels
/// are otherwise stable (the lowering and the committed serializer both mint
/// deterministic labels), so a sorted multiset comparison is exact up to this list
/// permutation. Returns a SORTED `Vec` (multiset), not a deduping set.
fn normalize_expects_index(nt: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in nt.lines() {
        let mut s = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(pos) = rest.find("_expects_") {
            s.push_str(&rest[..pos + "_expects_".len()]);
            rest = &rest[pos + "_expects_".len()..];
            // Drop the run of digits that follows.
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            s.push('X');
            rest = &rest[digits..];
        }
        s.push_str(rest);
        out.push(s);
    }
    out.sort();
    out
}

/// The per-subject `rdfs:seeAlso` triples in a multiset, keyed by subject. The emitter
/// (and so the committed catalog) picks a function's implementation-query `.rq` from
/// the FIRST cell using the transform in the oxigraph store's hash order; the lowering
/// picks it deterministically. For a multi-profile function the two pick different
/// `.rq` objects — the ONLY admissible divergence. This collapse keeps each seeAlso
/// per-subject so a dropped/extra seeAlso still surfaces (it is not folded away).
fn see_also_by_subject(lines: &[String]) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for line in lines {
        if line.contains("#seeAlso>")
            && let Some(subj) = line.split_whitespace().next()
        {
            out.entry(subj.to_owned()).or_default().push(line.clone());
        }
    }
    out
}

#[test]
fn fno_lowering_matches_committed_modulo_expects_and_seealso() {
    let root = repo_root();
    let dsl = merge_dsl(&root);
    let onto = merge_ontology(&root);
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);

    let lowered_nt = lower_fno(&dsl_view, &onto_view).expect("lower fno").catalog;
    assert!(!lowered_nt.is_empty(), "FnO catalog should be non-empty");

    let committed_path = root.join("generated/projections/functions.fno.ttl");
    let committed_bytes =
        std::fs::read(&committed_path).expect("committed functions.fno.ttl present");

    let lowered_ds = parse_dataset(
        lowered_nt.as_bytes(),
        NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .expect("lowered FnO parses as N-Triples");
    let committed_ds = parse_dataset(&committed_bytes, NativeRdfFormat::Turtle.media_type(), None)
        .expect("committed FnO parses as Turtle");
    let lowered_without_see = without_see_also(&lowered_ds);
    let committed_without_see = without_see_also(&committed_ds);
    let diff = dataset_diff(&lowered_without_see, &committed_without_see);
    assert!(
        diff.isomorphic,
        "FnO lowering diverged from the committed catalog beyond the deterministic seeAlso choice: {diff:?}",
    );

    let lo = normalize_expects_index(&lowered_nt);
    let co = ntriples_multiset(&committed_bytes, NativeRdfFormat::Turtle.media_type());

    // Each diverging seeAlso must be over the SAME subject in both (deterministic vs
    // hash first-profile picks a different `.rq`), with the SAME count per subject —
    // so a dropped or extra seeAlso triple is still caught.
    let lo_see = see_also_by_subject(&lo);
    let co_see = see_also_by_subject(&co);
    let lo_subjects: std::collections::BTreeSet<&String> = lo_see.keys().collect();
    let co_subjects: std::collections::BTreeSet<&String> = co_see.keys().collect();
    assert_eq!(
        lo_subjects, co_subjects,
        "seeAlso subjects diverged between lowering and committed catalog",
    );
    for (subj, lo_lines) in &lo_see {
        let co_lines = &co_see[subj];
        assert_eq!(
            lo_lines.len(),
            co_lines.len(),
            "seeAlso triple count for {subj} differs (a dropped/extra seeAlso)",
        );
    }
}
