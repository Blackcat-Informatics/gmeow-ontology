// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native recreation of the retired `tests/test_vocabulary_surface.py`
//! `test_root_imports_are_exactly_the_core_profile` invariant (Principle 4 —
//! one canonical source): the root IRI *is* the core profile.
//!
//! The root ontology's `owl:imports` set MUST equal EXACTLY the set of tierCore
//! slice IRIs. An extension slice leaking into the root, or a core slice missing
//! from it, is a gated failure — never silent drift. This is the native twin of
//! the deleted Python assertion, using the same slice-manifest tier truth the
//! `profiles` pipeline stage reads (`gmeow:sliceTier gmeow:tierCore`) and the
//! native purrdf Turtle parser for both the root ontology and every manifest.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use purrdf::slice::rdf_query::Dataset;

const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";
const SLICE_TIER: &str = "https://blackcatinformatics.ca/gmeow/sliceTier";
const TIER_CORE: &str = "https://blackcatinformatics.ca/gmeow/tierCore";

/// The committed root ontology document.
const ONTOLOGY_FILE: &str = "ontology/gmeow.ttl";

/// The repository root — the `gmeow-validate` crate lives at `crates/validate`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the validate crate should live under crates/")
        .to_path_buf()
}

fn parse(path: &Path) -> Dataset {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    Dataset::parse_turtle(&bytes, &path.display().to_string())
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

/// The root ontology's `owl:imports` IRI set (named-node objects only, exactly as
/// the retired Python `root.objects(predicate=OWL.imports)` URIRef filter).
fn root_imports(root: &Path) -> BTreeSet<String> {
    let ds = parse(&root.join(ONTOLOGY_FILE));
    let mut out = BTreeSet::new();
    for subject in ds
        .subjects_of_type(OWL_ONTOLOGY)
        .expect("query owl:Ontology subjects")
    {
        for iri in ds
            .object_iris(&subject, OWL_IMPORTS)
            .expect("query owl:imports objects")
        {
            out.insert(iri);
        }
    }
    out
}

/// Every `manifest.ttl` that has a sibling `module.ttl` — the minting slices the
/// `profiles` stage's `discover_slices` walks (a pure-selection profile slice
/// mints no `module.ttl` and so contributes no core member).
fn slice_manifests(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("slices")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "manifest.ttl")
                && path.with_file_name("module.ttl").is_file()
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The set of tierCore slice IRIs — the canonical core profile membership,
/// read from the same `gmeow:sliceTier gmeow:tierCore` manifest fact the
/// `profiles` pipeline stage classifies on.
fn core_slice_iris(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for manifest in slice_manifests(root) {
        let ds = parse(&manifest);
        for iri in ds.subjects_of_type(SLICE_CLASS).expect("query gmeow:Slice") {
            let is_core = ds
                .object_iris(&iri, SLICE_TIER)
                .expect("query gmeow:sliceTier")
                .iter()
                .any(|t| t == TIER_CORE);
            if is_core {
                out.insert(iri);
            }
        }
    }
    out
}

/// The root IRI IS the core profile: `owl:imports` == tierCore slice set, exactly.
#[gmeow_test_batch_macros::batch_test]
fn root_imports_are_exactly_the_core_profile() {
    let root = repo_root();
    let imports = root_imports(&root);
    let core = core_slice_iris(&root);

    // Non-vacuity: both the root imports and the core set must be genuinely
    // populated — a parse that silently yielded empty sets would make the
    // equality assertion below trivially true and the gate meaningless.
    assert!(
        !imports.is_empty(),
        "root ontology {ONTOLOGY_FILE} has no owl:imports — the query is vacuous"
    );
    assert!(
        !core.is_empty(),
        "no tierCore slices discovered — the manifest scan is vacuous"
    );

    let extra: Vec<&String> = imports.difference(&core).collect();
    let missing: Vec<&String> = core.difference(&imports).collect();
    assert!(
        imports == core,
        "root/core drift — root owl:imports must equal the tierCore slice set exactly.\n\
         extra (imported but not tierCore): {extra:?}\n\
         missing (tierCore but not imported): {missing:?}"
    );
}
