// SPDX-License-Identifier: AGPL-3.0-only

//! Shape-census totality gate for the logic:-only shape-retirement contract.
//!
//! The projection-purity gate ([`gmeow_validate::repo_static::check_repo_static`])
//! proves a *local* property: no authored `sh:NodeShape`/`sh:PropertyShape` exists
//! without a `logic:formalizes` back-reference. That is necessary but NOT sufficient
//! for the equivalence-before-deletion contract: a `logic:formalizes` triple whose
//! OBJECT resolves to nothing is a *hollow backing* — it satisfies the presence check
//! yet names no real `logic:` source of truth, so the shape is a second source of
//! truth in disguise. The purity gate cannot see that (it only checks the triple
//! exists); this census does.
//!
//! This gate asserts the totality / disjointness of the retirement partition over the
//! whole authored surface (`slices` + `shapes` + `dsl` + `governance` — the same root
//! set the purity gates scan), using a REAL RDF parse rather than a line scan (Turtle
//! subjects stand alone on their own line, so a `grep`-style scan silently drops them
//! and reports confident-but-wrong danglers):
//!
//! 1. **Backing totality** — every authored `sh:NodeShape`/`sh:PropertyShape` node
//!    carries at least one `logic:formalizes` edge. (Re-derives the purity invariant
//!    from the parse, so the census stands alone as a fail-closed oracle.)
//! 2. **No hollow backing** — every `logic:formalizes` edge WHOSE SUBJECT IS A
//!    RETAINED AUTHORED SHAPE points *up* to a backing term that is a defined subject
//!    somewhere in the authored source. A retained hand shape whose backing target is
//!    defined nowhere is a hollow backing and a HARD failure.
//!
//!    Note the direction: `logic:formalizes` has two opposite uses. On a *retained
//!    hand shape* it names the backing `logic:Constraint`/axiom that MUST exist
//!    (checked here). On a `logic:Constraint` it names the *retired shape it replaced*,
//!    which is intentionally DELETED (defined nowhere by design — the migrated `math:`
//!    slice is full of these). The census scopes invariant 2 to authored-shape
//!    subjects precisely so a legitimate retirement record is never mistaken for a
//!    dangling backing.
//! 3. **Non-vacuity** — the authored-shape set is non-empty, so the gate cannot pass
//!    by finding nothing to check.
//!
//! Source-only and deterministic: it reads no generated artifact, so it is stable
//! across regenerations and cannot go vacuously green after the authored shapes are
//! deleted (deletion shrinks set 1 but the invariant still quantifies over what
//! remains). It runs in the default `cargo nextest` lane that `make check` invokes.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue, parse_dataset};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SH_NODE_SHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SH_PROPERTY_SHAPE: &str = "http://www.w3.org/ns/shacl#PropertyShape";
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";

/// The maximal authored root set — identical to the purity gates in
/// `crates/validate/src/repo_static.rs`, so authored SHACL cannot hide from the census
/// in a root the gates do not scan.
const AUTHORED_ROOTS: &[&str] = &["slices", "shapes", "dsl", "governance"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Every `*.ttl` under `dir` (recursive), sorted; a missing dir yields nothing. The
/// `generated/` tree is never under these roots, so no projection leaks into the scan.
fn ttl_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            ttl_files_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
}

fn authored_ttl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in AUTHORED_ROOTS {
        ttl_files_recursive(&root.join(sub), &mut out);
    }
    out
}

#[inline]
fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The parsed facts one authored shape file contributes to the census.
#[derive(Default)]
struct FileFacts {
    /// `sh:NodeShape`/`sh:PropertyShape` subject IRIs declared in this file.
    shape_iris: Vec<String>,
    /// `logic:formalizes` edges: (shape-or-constraint subject IRI, target IRI).
    formalizes: Vec<(String, String)>,
    /// Every IRI that is the SUBJECT of any triple in this file (the "defined here" set).
    defined_subjects: BTreeSet<String>,
}

fn scan_file(path: &Path) -> FileFacts {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let ds = parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));

    let mut facts = FileFacts::default();

    // Every subject IRI in the file — the "defined somewhere" evidence set. A blank
    // node cannot be a `logic:formalizes` target (targets are named terms), so only
    // IRI subjects matter here.
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            facts.defined_subjects.insert(s.to_string());
        }
    }

    // Shape declarations: subjects typed sh:NodeShape or sh:PropertyShape.
    if let Some(type_id) = iri_id(&ds, RDF_TYPE) {
        for shape_ty in [SH_NODE_SHAPE, SH_PROPERTY_SHAPE] {
            let Some(ty_id) = iri_id(&ds, shape_ty) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(type_id), Some(ty_id), GraphMatch::Any) {
                if let TermRef::Iri(s) = ds.resolve(q.s) {
                    facts.shape_iris.push(s.to_string());
                }
            }
        }
    }

    // logic:formalizes edges.
    if let Some(pred_id) = iri_id(&ds, LOGIC_FORMALIZES) {
        for q in ds.quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any) {
            if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                facts.formalizes.push((s.to_string(), o.to_string()));
            }
        }
    }

    facts
}

/// The whole-repo census: parse every authored `.ttl`, then prove the three totality
/// invariants over the union.
#[test]
fn census_partition_is_total_and_disjoint() {
    let root = repo_root();
    let files = authored_ttl_files(&root);
    assert!(
        !files.is_empty(),
        "census found no authored .ttl under {AUTHORED_ROOTS:?} — the scan roots are wrong \
         (a vacuous census is a broken census)"
    );

    // shape IRI -> file it was declared in (for a precise failure locus).
    let mut shape_file: BTreeMap<String, PathBuf> = BTreeMap::new();
    // subject IRI -> set of formalizes targets it declares.
    let mut formalizes_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // the union of every defined subject IRI across the whole authored source.
    let mut defined: BTreeSet<String> = BTreeSet::new();

    for file in &files {
        let facts = scan_file(file);
        for iri in facts.shape_iris {
            shape_file.entry(iri).or_insert_with(|| file.clone());
        }
        for (subject, target) in facts.formalizes {
            formalizes_of.entry(subject).or_default().insert(target);
        }
        defined.extend(facts.defined_subjects);
    }

    assert!(
        !shape_file.is_empty(),
        "census found no authored sh:NodeShape/sh:PropertyShape — non-vacuity violated"
    );

    // Invariant 1: backing totality — every authored shape carries logic:formalizes.
    let unbacked: Vec<String> = shape_file
        .iter()
        .filter(|(iri, _)| !formalizes_of.contains_key(*iri))
        .map(|(iri, file)| format!("  {iri}  ({})", rel(&root, file)))
        .collect();
    assert!(
        unbacked.is_empty(),
        "census invariant 1 (backing totality) FAILED: {} authored shape(s) carry no \
         logic:formalizes back-reference — a hand shape with no logic: source of truth:\n{}",
        unbacked.len(),
        unbacked.join("\n")
    );

    // Invariant 2: no hollow backing — every logic:formalizes edge FROM A RETAINED
    // AUTHORED SHAPE points up to a backing term that is a defined subject somewhere in
    // the authored source. Edges FROM a logic:Constraint (which point back to the
    // retired shape it replaced — legitimately deleted) are out of scope: a retirement
    // record is not a hollow backing.
    let mut hollow: Vec<String> = Vec::new();
    for (subject, targets) in &formalizes_of {
        if !shape_file.contains_key(subject) {
            continue; // not an authored shape node — this is a constraint→retired-shape record
        }
        for target in targets {
            if !defined.contains(target) {
                hollow.push(format!(
                    "  {subject}  ->  {target}  (retained shape's backing defined nowhere)"
                ));
            }
        }
    }
    hollow.sort();
    assert!(
        hollow.is_empty(),
        "census invariant 2 (no hollow backing) FAILED: {} logic:formalizes target(s) \
         resolve to no defined subject — the backing is cosmetic (passes the purity gate's \
         presence check yet names no real logic: source):\n{}",
        hollow.len(),
        hollow.join("\n")
    );
}

fn rel(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}
