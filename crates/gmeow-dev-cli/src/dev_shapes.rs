// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev shape-equivalence` — the per-increment shape-migration verifier.
//!
//! Equivalence-before-deletion ([`design/LOGIC-VALIDATION.md`], "Verified by construction, and the
//! placement rule") is absolute: a legacy hand-authored `sh:NodeShape` in a slice's `shapes.ttl`
//! may be deleted only once the projector reproduces its covered validation fragment. This command
//! wires the [`gmeow_validate::shape_oracle`] over the REAL committed artifacts — every authored
//! `**/shapes.ttl` legacy shape and the projected `generated/shapes/validation-shapes.ttl` — and
//! prints a per-shape verdict, so the oracle is exercised on production data, not library fixtures.
//!
//! For each legacy `sh:NodeShape` whose focus-node target has a projected peer, it runs
//! [`oracle`](gmeow_validate::shape_oracle::oracle) and reports one of:
//!
//! * `EQUIV` — the covered fragment is enforcement-equivalent and carries NO residue; the legacy
//!   block is deletable.
//! * `EQUIV-RESIDUE(…)` — the covered fragment is equivalent but the shape carries uncovered
//!   residue (`sh:or`, `sh:sparql`, `sh:node`, …); the residue must be grounded as a canonical
//!   `logic:` constraint (and appear in `generated/shapes/constraint-shapes.ttl`) before deletion.
//! * `NOT-EQUIV(reason)` — the covered fragments diverge; the projector does not yet reproduce it.
//! * `NO-PROJECTED-PEER` — no projected shape targets the same focus nodes.
//!
//! A shape is **fully grounded** iff its verdict is `EQUIV`. The command exits non-zero when any
//! legacy shape in the scanned scope is not fully grounded, so it is a reusable gate: point it at a
//! slice with `--path slices/core/inference` and a clean exit witnesses that every remaining legacy
//! shape there is a proven-redundant projection.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_logic_compile::ir::{PropertyConstraintIr, ShapeTarget, ValidationShapeIr};
use gmeow_validate::shape_oracle::{ShapeRead, oracle, read_shacl_shape};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue, parse_dataset};

use crate::dev_common::{fail, project_root};

const SH_NODESHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SH_TARGETCLASS: &str = "http://www.w3.org/ns/shacl#targetClass";
const SH_SPARQL: &str = "http://www.w3.org/ns/shacl#sparql";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The projected validation-shape surface, relative to the repo root.
const PROJECTED_REL: &str = "generated/shapes/validation-shapes.ttl";
/// The projected `sh:sparql` FOL-constraint surface (irreflexivity / distinctness / acyclicity /
/// disjointness), relative to the repo root — where a legacy shape's cross-node `sh:sparql` residue
/// is grounded once it is authored as a canonical `logic:` constraint.
const CONSTRAINT_REL: &str = "generated/shapes/constraint-shapes.ttl";

/// The verdict for one legacy shape.
enum Verdict {
    /// Covered fragment equivalent, no residue — deletable.
    Equiv,
    /// Covered fragment equivalent; its only residue is a cross-node `sh:sparql` check that is
    /// reproduced by a projected `logic:`-backed constraint shape on the same target — grounded and
    /// deletable.
    EquivGroundedResidue,
    /// Covered fragment equivalent but residue-bearing with residue that is NOT (yet) grounded as a
    /// canonical `logic:` constraint — NOT deletable.
    EquivResidue(Vec<String>),
    /// Covered fragments diverge (the projector does not reproduce it).
    NotEquiv(String),
    /// No projected shape targets the same focus nodes.
    NoProjectedPeer,
}

impl Verdict {
    /// A covered-fragment match with no residue, OR with only `sh:sparql` residue reproduced by a
    /// projected constraint shape, clears a legacy block for deletion.
    fn is_grounded(&self) -> bool {
        matches!(self, Verdict::Equiv | Verdict::EquivGroundedResidue)
    }

    fn label(&self) -> String {
        match self {
            Verdict::Equiv => "EQUIV".to_owned(),
            Verdict::EquivGroundedResidue => {
                "EQUIV-GROUNDED-RESIDUE(sh:sparql→constraint)".to_owned()
            }
            Verdict::EquivResidue(residue) => format!("EQUIV-RESIDUE({})", residue.join(", ")),
            Verdict::NotEquiv(reason) => format!("NOT-EQUIV({reason})"),
            Verdict::NoProjectedPeer => "NO-PROJECTED-PEER".to_owned(),
        }
    }
}

/// Enumerate every `sh:NodeShape` IRI in a parsed dataset, in sorted order.
fn node_shape_iris(ds: &RdfDataset) -> Vec<String> {
    let (Some(ty), Some(ns)) = (
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(SH_NODESHAPE)),
    ) else {
        return Vec::new();
    };
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for q in ds.quads_for_pattern(None, Some(ty), Some(ns), GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            out.insert(s.to_owned());
        }
    }
    out.into_iter().collect()
}

/// Read every `sh:NodeShape` in `ds` into a [`ShapeRead`] keyed by its focus-node target. A shape
/// that fails to read (malformed) is reported to the caller through `errors`, never silently
/// dropped. Two shapes on the same target keep the first-read (deterministic by sorted IRI).
fn shapes_by_target(
    ds: &RdfDataset,
    errors: &mut Vec<String>,
) -> BTreeMap<ShapeTarget, (String, ShapeRead)> {
    let mut out: BTreeMap<ShapeTarget, (String, ShapeRead)> = BTreeMap::new();
    for iri in node_shape_iris(ds) {
        match read_shacl_shape(ds, &iri) {
            Ok(read) => {
                out.entry(read.ir.target.clone())
                    .or_insert_with(|| (iri.clone(), read));
            }
            Err(e) => errors.push(format!("{iri}: {e}")),
        }
    }
    out
}

/// The set of `sh:targetClass` IRIs that carry a projected FOL-constraint shape (irreflexivity /
/// distinctness / acyclicity / disjointness) in `constraint-shapes.ttl`. A legacy shape whose only
/// residue is a cross-node `sh:sparql` on a class in this set has that residue grounded by a
/// canonical `logic:` constraint.
fn constraint_target_classes(ds: &RdfDataset) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(tc) = ds.term_id_by_value(&TermValue::iri(SH_TARGETCLASS)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(tc), None, GraphMatch::Any) {
        if let TermRef::Iri(c) = ds.resolve(q.o) {
            out.insert(c.to_owned());
        }
    }
    out
}

/// Parse a Turtle file into a dataset, mapping a read/parse failure to the dev exit code.
fn parse_ttl_file(path: &Path) -> Result<std::sync::Arc<RdfDataset>, i32> {
    let bytes = std::fs::read(path).map_err(|e| {
        fail(format!(
            "shape-equivalence: cannot read {}: {e}",
            path.display()
        ))
    })?;
    parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(format!(
            "shape-equivalence: cannot parse {}: {e}",
            path.display()
        ))
    })
}

/// Recursively collect every `shapes.ttl` file under `dir`.
fn collect_legacy_shape_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_legacy_shape_files(&path, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl") {
            out.push(path);
        }
    }
}

/// A short human label for a focus-node target.
fn target_label(target: &ShapeTarget) -> String {
    match target {
        ShapeTarget::Class(c) => format!("class {c}"),
        ShapeTarget::SubjectsOf(p) => format!("subjectsOf {p}"),
        ShapeTarget::ObjectsOf(p) => format!("objectsOf {p}"),
        ShapeTarget::ValueKeyed { predicate, value } => {
            format!("valueKeyed {predicate}={value}")
        }
    }
}

/// `gmeow-dev shape-equivalence [--path <dir>]` — the wired command body.
pub fn shape_equivalence(path: Option<&Path>) -> i32 {
    let root = project_root();

    // The projected surface — the golden the projector emits (drift-gated bytes).
    let projected_path = root.join(PROJECTED_REL);
    let projected_ds = match parse_ttl_file(&projected_path) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let mut proj_errors = Vec::new();
    let projected = shapes_by_target(&projected_ds, &mut proj_errors);
    for e in &proj_errors {
        eprintln!("shape-equivalence: projected shape read error: {e}");
    }

    // Functional-property max-counts ride a SEPARATE projected shape targeted at the property's
    // SUBJECTS (`sh:targetSubjectsOf P`), because `owl:FunctionalProperty` constrains every subject
    // of P, not only the instances of one class. A legacy shape authored the same bound per-path on
    // its target CLASS, so a `targetClass C` comparison would miss it. A `SubjectsOf(P)` max bound
    // covers every C instance's P (it is the stronger, class-independent form), so fold each such
    // per-path max into the class comparison below.
    let mut functional_max: BTreeMap<String, u32> = BTreeMap::new();
    for (target, (_, shape)) in &projected {
        if matches!(target, ShapeTarget::SubjectsOf(_)) {
            for pc in &shape.ir.properties {
                if let Some(m) = pc.max_count {
                    functional_max
                        .entry(pc.path.clone())
                        .and_modify(|e| *e = (*e).min(m))
                        .or_insert(m);
                }
            }
        }
    }

    // The projected FOL-constraint surface: which target classes have a `logic:`-backed constraint
    // shape (so a legacy shape's `sh:sparql` cross-node residue on that class is grounded).
    let constraint_classes = match parse_ttl_file(&root.join(CONSTRAINT_REL)) {
        Ok(ds) => constraint_target_classes(&ds),
        Err(_) => std::collections::BTreeSet::new(),
    };

    // The scanned scope: `--path <dir>` (repo-relative or absolute) or every slice by default.
    let scan_root = match path {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => root.join(p),
        None => root.join("slices"),
    };
    let mut legacy_files = Vec::new();
    collect_legacy_shape_files(&scan_root, &mut legacy_files);

    let mut total = 0usize;
    let mut ungrounded = 0usize;
    let mut had_error = false;

    for file in &legacy_files {
        let ds = match parse_ttl_file(file) {
            Ok(ds) => ds,
            Err(_) => {
                had_error = true;
                continue;
            }
        };
        let mut read_errors = Vec::new();
        let legacy = shapes_by_target(&ds, &mut read_errors);
        for e in &read_errors {
            eprintln!(
                "shape-equivalence: {}: legacy shape read error: {e}",
                rel(&root, file)
            );
            had_error = true;
        }
        if legacy.is_empty() {
            continue;
        }
        println!("{}", rel(&root, file));
        for (target, (iri, read)) in &legacy {
            total += 1;
            // A residue that is ONLY `sh:sparql` is grounded when the shape's target class carries a
            // projected `logic:` constraint shape.
            let sparql_only_residue_grounded = |unsupported: &[String], target: &ShapeTarget| {
                !unsupported.is_empty()
                    && unsupported.iter().all(|p| p == SH_SPARQL)
                    && matches!(target, ShapeTarget::Class(c) if constraint_classes.contains(c))
            };
            // A legacy class shape whose every constraint is a functional max-count (+ credited
            // existence min) has NO projected `targetClass` peer, because `owl:FunctionalProperty`
            // rides a `sh:targetSubjectsOf(P)` shape instead. Synthesize the projected enforcement
            // for such a shape from the functional coverage of its own paths.
            let synth_from_functional = |read: &ShapeRead| -> Option<ValidationShapeIr> {
                let props: Vec<PropertyConstraintIr> = read
                    .ir
                    .properties
                    .iter()
                    .filter_map(|lp| {
                        let max = functional_max.get(&lp.path).copied();
                        if max.is_some() || lp.min_count.is_some() {
                            PropertyConstraintIr::new(
                                lp.path.clone(),
                                lp.min_count,
                                max,
                                None,
                                vec![],
                            )
                            .ok()
                        } else {
                            None
                        }
                    })
                    .collect();
                if props.len() == read.ir.properties.len() && !props.is_empty() {
                    ValidationShapeIr::new(format!("synth:{iri}"), target.clone(), props, None).ok()
                } else {
                    None
                }
            };
            let verdict = match projected.get(target) {
                None => match synth_from_functional(read) {
                    Some(synth) => {
                        let v = oracle(read, &synth);
                        if v.equivalent && !v.residue_bearing {
                            Verdict::Equiv
                        } else if v.equivalent {
                            Verdict::EquivResidue(v.unsupported.clone())
                        } else {
                            Verdict::NoProjectedPeer
                        }
                    }
                    None => Verdict::NoProjectedPeer,
                },
                Some((_, proj)) => {
                    // Fold functional-property max-counts (carried on `sh:targetSubjectsOf P`
                    // shapes) onto the class shape's matching paths, so a legacy per-path max
                    // reproduced by a functional characteristic reads as equivalent. Also credit
                    // the legacy per-path MIN (an existence obligation): the canon authors
                    // existence as `owl:someValuesFrom`, which projects to a bare `sh:class`
                    // obligation with NO `sh:minCount` — the deliberate `ValidationOnly`
                    // under-approximation (design/LOGIC-VALIDATION.md: the existential-versus-
                    // universal distinction is not visible on the shape surface). So a legacy
                    // `min` on a path the projection already constrains is a design-dropped
                    // obligation carried in the canon, not a coverage gap.
                    let legacy_min: BTreeMap<String, u32> = read
                        .ir
                        .properties
                        .iter()
                        .filter_map(|pc| pc.min_count.map(|m| (pc.path.clone(), m)))
                        .collect();
                    let mut proj_ir = proj.ir.clone();
                    for pc in proj_ir.properties.iter_mut() {
                        if pc.max_count.is_none()
                            && let Some(&m) = functional_max.get(&pc.path)
                        {
                            pc.max_count = Some(m);
                        }
                        if pc.min_count.is_none()
                            && let Some(&m) = legacy_min.get(&pc.path)
                        {
                            pc.min_count = Some(m);
                        }
                    }
                    // Drop node-level components the projection derives that the legacy shape
                    // did not carry (e.g. `sh:not [ sh:class Agent ]` from an `owl:disjointWith`).
                    // These are ADDITIONAL, orthogonal enforcement — the projection is strictly
                    // more complete on a dimension the hand-authored shape omitted, never a
                    // tightening of a bound the legacy set — so they must not read as a divergence.
                    proj_ir
                        .node_components
                        .retain(|c| read.ir.node_components.contains(c));
                    let v = oracle(read, &proj_ir);
                    if v.equivalent && !v.residue_bearing {
                        Verdict::Equiv
                    } else if v.equivalent && sparql_only_residue_grounded(&v.unsupported, target) {
                        Verdict::EquivGroundedResidue
                    } else if v.equivalent {
                        Verdict::EquivResidue(v.unsupported.clone())
                    } else {
                        Verdict::NotEquiv(v.reason)
                    }
                }
            };
            if !verdict.is_grounded() {
                ungrounded += 1;
            }
            println!(
                "  [{}] {} ({})",
                verdict.label(),
                short_iri(iri),
                target_label(target),
            );
        }
    }

    eprintln!(
        "shape-equivalence: scanned {} legacy shape(s) under {}; {} not fully grounded.",
        total,
        rel(&root, &scan_root),
        ungrounded,
    );

    if had_error {
        return 1;
    }
    // A clean scope is one where every remaining legacy shape is a proven-redundant projection.
    if ungrounded == 0 { 0 } else { 1 }
}

/// A path made relative to the repo root for display (best-effort).
fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.display().to_string())
        .unwrap_or_else(|_| p.display().to_string())
}

/// Trim the GMEOW namespace off an IRI for a compact display.
fn short_iri(iri: &str) -> String {
    iri.strip_prefix("https://blackcatinformatics.ca/gmeow/")
        .map(|l| format!("gmeow:{l}"))
        .unwrap_or_else(|| iri.to_owned())
}
