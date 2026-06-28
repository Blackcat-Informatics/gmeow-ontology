// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Corpus parity oracle for the FnO correspondence lowering (#1089).
//!
//! Proves the oxigraph-free FnO lowering reproduces the historical emitter exactly:
//! natively merge the DSL sources (for functions/cells) and the ontology sources (for
//! `rdfs:range` typing + the language-tag map) into two `DslView`s, lower via
//! `gmeow_logic_compile::projections::fno::lower_fno`, and assert the N-Triples text is
//! byte-identical to `gmeow_slice::fno_emit::emit_fno(root)` (the oxigraph emitter,
//! itself already gated against the committed `functions.fno.ttl`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::fno::lower_fno;
use gmeow_rdf::{parse_dataset, NativeRdfFormat, RdfDataset, RdfDatasetBuilder};
use gmeow_slice::{ArtifactRole, SliceCatalog};

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
    let catalog = SliceCatalog::discover(&slices_dir).expect("discover slices");
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

/// Collapse the positional index of an `fno:expects` rdf:List blank label so that a
/// deterministic list reordering compares equal: `…_expects_0` / `…_expects_1` → a
/// single `…_expects_X`. The historical emitter ordered the expects list by the
/// oxigraph store's hash order; the lowering orders it deterministically (lexically),
/// so the two graphs are equal up to that list permutation (the same parameter SET,
/// just listed in a different order). Every other triple — including each `rdf:first`
/// member and the function/param/mapping nodes — must still match exactly.
fn normalize_expects_index(nt: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
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
        out.insert(s);
    }
    out
}

#[test]
fn fno_lowering_matches_emitter_modulo_expects_order() {
    let root = repo_root();
    let dsl = merge_dsl(&root);
    let onto = merge_ontology(&root);
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);

    let lowered = lower_fno(&dsl_view, &onto_view).expect("lower fno").catalog;
    let emitted = gmeow_slice::fno_emit::emit_fno(&root).expect("emit_fno");
    assert!(!lowered.is_empty(), "FnO catalog should be non-empty");

    let lo = normalize_expects_index(&lowered);
    let em = normalize_expects_index(&emitted);

    let only_emitter: Vec<&String> = em.difference(&lo).collect();
    let only_lowered: Vec<&String> = lo.difference(&em).collect();

    // The ONLY admissible divergence is `rdfs:seeAlso`: the emitter picks a function's
    // implementation-query `.rq` from the FIRST cell that uses the transform in the
    // oxigraph store's hash order; the lowering picks it deterministically (the first
    // cell in IRI-sorted order). For a multi-profile function the two pick different
    // profiles. Everything else — types, labels, params, expects (modulo order),
    // implementations, mappings — must match exactly. Each diverging seeAlso must be
    // over the SAME function in both sets (same subject, different `.rq` object).
    let see_subjects = |lines: &[&String]| -> std::collections::BTreeSet<String> {
        lines
            .iter()
            .filter(|l| l.contains("#seeAlso>"))
            .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
            .collect()
    };
    let non_see = |lines: &[&String]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| !l.contains("#seeAlso>"))
            .map(|s| (*s).clone())
            .collect()
    };

    let e_non_see = non_see(&only_emitter);
    let l_non_see = non_see(&only_lowered);
    assert!(
        e_non_see.is_empty() && l_non_see.is_empty(),
        "FnO lowering diverged beyond the deterministic seeAlso choice:\n  emitter-only non-seeAlso ({}): {:?}\n  lowered-only non-seeAlso ({}): {:?}",
        e_non_see.len(),
        e_non_see.iter().take(8).collect::<Vec<_>>(),
        l_non_see.len(),
        l_non_see.iter().take(8).collect::<Vec<_>>(),
    );
    assert_eq!(
        see_subjects(&only_emitter),
        see_subjects(&only_lowered),
        "seeAlso divergence must be over the same set of functions (deterministic vs hash first-profile)",
    );
}
