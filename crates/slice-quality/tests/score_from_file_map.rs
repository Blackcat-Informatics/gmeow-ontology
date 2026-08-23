// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Score a slice that never touched a filesystem.
//!
//! The scoring kernel reads a slice from an in-memory `BTreeMap<String, Vec<u8>>`
//! keyed by slice-relative forward-slash path. These tests exercise that contract
//! from both ends: a hand-built map (bytes that exist only in this process — no temp
//! directory, no fixture tree) scores like a real slice, and a map missing the ONE
//! file that carries slice identity hard-fails with a message naming it.
//!
//! They also pin the seam the whole refactor rests on: the map-derived Turtle
//! document list is the same file set, in the same order, as the on-disk path list
//! the pipeline still uses for cache keys. If those two ever diverge, the in-memory
//! scorer and the disk scorer would union the same slice's documents differently.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_slice_quality::ScoringEnv;
use gmeow_slice_quality::report::{
    score_slice_files_with_standard, slice_files_from_dir, slice_iri_of_files, slice_ttl_documents,
    slice_ttl_paths,
};

const SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/inmemory";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

/// The floor-free measurement standard, loaded from the canonical rubric module.
fn standard() -> gmeow_slice_quality::MeasurementStandard {
    let module = repo_root().join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module]).expect("rubric module parses");
    gmeow_slice_quality::rubric::load_rubric(&ds)
        .expect("rubric loads")
        .standard
}

/// A complete little slice built entirely in memory: a manifest carrying the slice's
/// identity, a module authoring two owned classes with a full annotation coat, a
/// worked example, a competency test cell, and a narrative `docs.md`. Nothing here
/// exists on disk at any point.
fn in_memory_slice() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    files.insert(
        "manifest.ttl".to_owned(),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <{SLICE_IRI}> a gmeow:Slice .\n"
        )
        .into_bytes(),
    );
    files.insert(
        "module.ttl".to_owned(),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             gmeow:Widget a owl:Class , logic:Kind ;\n\
                rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                rdfs:label \"Widget\"@en ;\n\
                skos:definition \"A fabricated artefact, never a mere aggregate of parts.\"@en .\n\
             gmeow:Gadget a owl:Class , logic:Kind ;\n\
                rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                rdfs:label \"Gadget\"@en ;\n\
                skos:definition \"A contrivance, as opposed to a naturally occurring object.\"@en .\n"
        )
        .into_bytes(),
    );
    files.insert(
        "examples/widget.ttl".to_owned(),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix ex: <https://example.test/inmemory/> .\n\
         ex:w1 a gmeow:Widget .\n"
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tests/competency.rq".to_owned(),
        "ASK { ex:w1 a gmeow:Widget . ex:g1 a gmeow:Gadget . }\n"
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "docs.md".to_owned(),
        format!(
            "# In-memory slice\n\n{}\n",
            "This slice exists only as bytes in a map. It is scored by the very same \
             kernel a checked-out slice is scored by, which is the whole point: the \
             scorer's input is a file map, and a directory is merely one way to produce \
             one. The prose here is long enough to clear the narrative-thesis bar the \
             documentation axis measures."
        )
        .into_bytes(),
    );
    files
}

/// A slice held only in memory scores end-to-end: identity resolves out of the map's
/// `manifest.ttl`, every rubric axis runs, and the file-shaped axes see the map's
/// entries (the `docs.md` thesis and the `tests/` cell are both credited).
#[test]
fn a_hand_built_file_map_scores_end_to_end() {
    let files = in_memory_slice();
    let report = score_slice_files_with_standard(
        &files,
        &standard(),
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    )
    .expect("an in-memory slice scores");

    assert_eq!(
        report.assessment.slice, SLICE_IRI,
        "identity is resolved from the map's manifest.ttl"
    );
    assert_eq!(
        report.assessment.grades.len(),
        standard().axes.len(),
        "every rubric axis is graded from the map alone"
    );

    // The file-shaped axes genuinely READ the map rather than defaulting: the
    // narrative docs.md earns the documentation axis its full mark, and the tests/
    // cell names both owned terms so optimal testing is complete. Both would be
    // strictly below 1.0 if the map entries were invisible to the axes.
    let grade = |local: &str| {
        report
            .assessment
            .grades
            .iter()
            .find(|g| g.axis_iri.ends_with(local))
            .unwrap_or_else(|| panic!("the rubric declares an axis ending {local}"))
    };
    assert_eq!(
        grade("axisDocumentation").score,
        1.0,
        "the map's docs.md is read by the documentation axis"
    );
    assert_eq!(
        grade("axisOptimalTesting").score,
        1.0,
        "the map's tests/competency.rq is read by the testing axis"
    );
}

/// A map with no `manifest.ttl` is a HARD ERROR, and the message names the missing
/// file explicitly — identity is the precondition for scoring, never a degradable
/// axis, and a caller who forgot the manifest must be told which key is missing.
#[test]
fn a_map_without_a_manifest_hard_fails_naming_manifest_ttl() {
    let mut files = in_memory_slice();
    files.remove("manifest.ttl");

    let err = slice_iri_of_files(&files).expect_err("a manifest-less map must hard-fail");
    let message = err.to_string();
    assert!(
        message.contains("manifest.ttl"),
        "the hard error must name manifest.ttl explicitly, got: {message}"
    );

    // The same hard failure propagates through the whole scoring entry point — a
    // manifest-less map never scores as a clean, contentless slice.
    let scored = score_slice_files_with_standard(
        &files,
        &standard(),
        ScoringEnv::Bundle(std::sync::Arc::new(
            gmeow_lang_bridge::GmnDictionary::default(),
        )),
    );
    let Err(err) = scored else {
        panic!("scoring a manifest-less map must hard-fail, never produce a report");
    };
    assert!(
        err.to_string().contains("manifest.ttl"),
        "the scoring hard error must name manifest.ttl explicitly, got: {err}"
    );
}

/// The map-derived Turtle document list and the on-disk path list are the SAME file
/// set in the SAME order, over every real slice in the checkout.
///
/// This is the load-bearing equivalence of the whole file-map seam: the pipeline
/// still enumerates `slice_ttl_paths` for cache keys while the scorer unions
/// `slice_ttl_documents`, and a divergence in either membership or order would mean
/// the two see different slices (or intern the same slice's terms in a different
/// order, which is a determinism hazard, not merely a cosmetic one).
#[test]
fn map_document_order_matches_the_on_disk_path_order_for_every_slice() {
    let slices_root = repo_root().join("slices");
    let dirs = gmeow_slice_quality::discover_slice_dirs(&slices_root);
    assert!(
        !dirs.is_empty(),
        "the checkout must carry slices for this equivalence to mean anything"
    );

    for dir in &dirs {
        let files = slice_files_from_dir(dir).expect("slice files read");
        let from_map: Vec<&str> = slice_ttl_documents(&files)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        let from_disk: Vec<String> = slice_ttl_paths(dir)
            .iter()
            .map(|p| {
                p.strip_prefix(dir)
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            from_map,
            from_disk.iter().map(String::as_str).collect::<Vec<_>>(),
            "{}: the map's Turtle documents must match the on-disk path list exactly, \
             in the same order",
            dir.display()
        );
    }
}

/// Every slice-local path the axes ask for by name is present in the map a real slice
/// directory produces, whenever that file exists on disk. A map that silently dropped
/// one of these would score the corresponding axis against an absent file and report a
/// clean vacuous result instead of the real measurement.
#[test]
fn the_dir_derived_map_carries_every_named_axis_input() {
    let dirs = gmeow_slice_quality::discover_slice_dirs(&repo_root().join("slices"));
    for dir in &dirs {
        let files = slice_files_from_dir(dir).expect("slice files read");
        for named in ["manifest.ttl", "module.ttl", "shapes.ttl", "docs.md"] {
            assert_eq!(
                files.contains_key(named),
                dir.join(named).is_file(),
                "{}: the map must carry {named} exactly when the slice ships it",
                dir.display()
            );
        }
        for sub in ["examples", "tests", "queries", "i18n", "mappings"] {
            let on_disk = walk_rel(&dir.join(sub), &dir.join(sub));
            for rel in on_disk {
                let key = format!("{sub}/{rel}");
                assert!(
                    files.contains_key(&key),
                    "{}: the map must carry {key}",
                    dir.display()
                );
            }
        }
    }
}

/// Every regular file under `dir`, as `/`-joined paths relative to `root` (empty when
/// `dir` does not exist).
fn walk_rel(root: &Path, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rel(root, &path));
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    out
}
