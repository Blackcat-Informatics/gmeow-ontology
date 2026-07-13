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

use gmeow_logic_compile::ir::{
    ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShaclNodeKind, ShapeTarget,
    ShapeValue, ValidationShapeIr,
};
use gmeow_logic_compile::projections::lift::{certify, lift};
use gmeow_validate::shape_oracle::{OracleVerdict, ShapeRead, oracle, read_shacl_shape};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue, parse_dataset};

use crate::dev_common::{fail, project_root};

const SH_NODESHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SH_SPARQL: &str = "http://www.w3.org/ns/shacl#sparql";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The projected validation-shape surface, relative to the repo root.
const PROJECTED_REL: &str = "generated/shapes/validation-shapes.ttl";
/// The projected `sh:sparql` FOL-constraint surface (irreflexivity / distinctness / acyclicity /
/// disjointness), relative to the repo root — where a legacy shape's cross-node `sh:sparql` residue
/// is grounded once it is authored as a canonical `logic:` constraint.
const CONSTRAINT_REL: &str = "generated/shapes/constraint-shapes.ttl";
/// Canonical procedural-constraint projection. Residue is grounded only through an exact
/// `logic:formalizes <legacy-shape-IRI>` link in this or the constraint surface.
const PROCEDURAL_REL: &str = "generated/shapes/procedural-constraints.ttl";
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
const GMEOW_ENFORCES_FAILURE_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

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

/// A legacy shape is GROUNDED (safe to delete) by `proj_ir` when the projection enforces at least
/// everything the legacy does (`legacy_subsumed_by_projected`) AND does not TIGHTEN any per-path
/// cardinality bound the legacy set. A stricter projected bound would reject legacy-valid data — a
/// behavior change, not a faithful reproduction — so cardinality must be EQUAL on shared paths;
/// projected-EXTRA components (a class the hand-authored shape omitted) are more-complete
/// enforcement, not a divergence, and are admitted by the subsumption leg.
fn grounds(read: &ShapeRead, proj_ir: &ValidationShapeIr, v: &OracleVerdict) -> bool {
    v.legacy_subsumed_by_projected
        && read.ir.properties.iter().all(|lp| {
            proj_ir
                .properties
                .iter()
                .filter(|pp| pp.path == lp.path)
                .all(|pp| pp.min_count == lp.min_count && pp.max_count == lp.max_count)
        })
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

/// Read every `sh:NodeShape` in `ds`. A shape that fails to read (malformed) is reported to the
/// caller through `errors`, never silently dropped. The vector stays in deterministic IRI order
/// and deliberately retains multiple shapes with the same target: each authored block is an
/// independent migration obligation and must receive its own equivalence verdict.
fn read_shapes(ds: &RdfDataset, errors: &mut Vec<String>) -> Vec<(String, ShapeRead)> {
    let mut out = Vec::new();
    for iri in node_shape_iris(ds) {
        match read_shacl_shape(ds, &iri) {
            Ok(read) => out.push((iri, read)),
            Err(e) => errors.push(format!("{iri}: {e}")),
        }
    }
    out
}

/// Index projected shapes by focus target. The generated validation surface is
/// canonicalized to one aggregate declarative shape per target. A duplicate is
/// an oracle error: choosing either candidate could conceal enforcement carried
/// by the other and therefore cannot authorize deletion.
fn shapes_by_target(
    ds: &RdfDataset,
    errors: &mut Vec<String>,
) -> BTreeMap<ShapeTarget, (String, ShapeRead)> {
    let mut out = BTreeMap::new();
    for (iri, read) in read_shapes(ds, errors) {
        let target = read.ir.target.clone();
        match out.entry(target) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((iri, read));
            }
            std::collections::btree_map::Entry::Occupied(entry) => errors.push(format!(
                "projected shapes {} and {} share target {}; aggregate them before equivalence testing",
                entry.get().0,
                iri,
                target_label(entry.key())
            )),
        }
    }
    out
}

/// The set of `sh:targetClass` IRIs that carry a projected FOL-constraint shape (irreflexivity /
/// distinctness / acyclicity / disjointness) in `constraint-shapes.ttl`. A legacy shape whose only
/// residue is a cross-node `sh:sparql` on a class in this set has that residue grounded by a
/// canonical `logic:` constraint.
fn formalized_shape_iris(ds: &RdfDataset) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(formalizes) = ds.term_id_by_value(&TermValue::iri(LOGIC_FORMALIZES)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(formalizes), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.insert(iri.to_owned());
        }
    }
    out
}

/// Failure classes carried by each exact `logic:formalizes` replacement. This keeps typed
/// conformance diagnostics part of the migration proof even though they do not alter which data
/// graph conforms.
fn collect_formalized_failure_classes(
    ds: &RdfDataset,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut out: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let (Some(formalizes), Some(enforces)) = (
        ds.term_id_by_value(&TermValue::iri(LOGIC_FORMALIZES)),
        ds.term_id_by_value(&TermValue::iri(GMEOW_ENFORCES_FAILURE_CLASS)),
    ) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(formalizes), None, GraphMatch::Any) {
        let TermRef::Iri(legacy) = ds.resolve(q.o) else {
            continue;
        };
        for fc in ds.quads_for_pattern(Some(q.s), Some(enforces), None, GraphMatch::Any) {
            if let TermRef::Iri(failure) = ds.resolve(fc.o) {
                out.entry(legacy.to_owned())
                    .or_default()
                    .insert(failure.to_owned());
            }
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
        ShapeTarget::DirectClass(c) => format!("directClass {c}"),
        ShapeTarget::Sparql(select) => format!("sparql {select}"),
    }
}

/// The shared read-only oracle context: the projected validation-shape surface (indexed by focus
/// target), the functional-property max-count fold, and the set of target classes carrying a
/// projected FOL constraint shape. Built once from the committed generated surfaces; reused by
/// [`shape_equivalence`] (report) and [`shape_migrate`] (the prune phase), so the two share ONE
/// equivalence judgment.
struct OracleCtx {
    projected: BTreeMap<ShapeTarget, (String, ShapeRead)>,
    functional_max: BTreeMap<String, u32>,
    formalized_shapes: std::collections::BTreeSet<String>,
    formalized_failure_classes: BTreeMap<String, std::collections::BTreeSet<String>>,
    object_properties: std::collections::BTreeSet<String>,
}

impl OracleCtx {
    /// Load the context from the committed generated surfaces under `root`, mapping any read/parse
    /// failure to the dev exit code.
    fn load(root: &Path, tool: &str) -> Result<Self, i32> {
        let projected_ds = parse_ttl_file(&root.join(PROJECTED_REL))?;
        let mut proj_errors = Vec::new();
        let projected = shapes_by_target(&projected_ds, &mut proj_errors);
        for e in &proj_errors {
            eprintln!("{tool}: projected shape read error: {e}");
        }
        if !proj_errors.is_empty() {
            return Err(1);
        }

        // Functional-property max-counts ride a SEPARATE projected shape targeted at the property's
        // SUBJECTS (`sh:targetSubjectsOf P`), because `owl:FunctionalProperty` constrains every
        // subject of P, not only the instances of one class. A legacy shape authored the same bound
        // per-path on its target CLASS, so a `targetClass C` comparison would miss it. A
        // `SubjectsOf(P)` max bound covers every C instance's P (it is the stronger,
        // class-independent form), so fold each such per-path max into the class comparison.
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

        // The projected FOL-constraint surface: which target classes have a `logic:`-backed
        // constraint shape (so a legacy shape's `sh:sparql` cross-node residue on that class is
        // grounded).
        let mut formalized_shapes = std::collections::BTreeSet::new();
        let mut formalized_failure_classes: BTreeMap<String, std::collections::BTreeSet<String>> =
            BTreeMap::new();
        for rel in [CONSTRAINT_REL, PROCEDURAL_REL] {
            let ds = parse_ttl_file(&root.join(rel))?;
            formalized_shapes.extend(formalized_shape_iris(&ds));
            for (legacy, failures) in collect_formalized_failure_classes(&ds) {
                formalized_failure_classes
                    .entry(legacy)
                    .or_default()
                    .extend(failures);
            }
        }

        Ok(Self {
            projected,
            functional_max,
            formalized_shapes,
            formalized_failure_classes,
            object_properties: object_property_iris(root),
        })
    }

    /// The equivalence verdict for one legacy shape `read` (identified by `iri`, focus `target`)
    /// against the projected surface — the single per-shape judgment shared by the report and the
    /// prune phase.
    fn verdict(&self, iri: &str, target: &ShapeTarget, read: &ShapeRead) -> Verdict {
        // Strip a redundant `sh:nodeKind sh:IRI` on an `owl:ObjectProperty` path: in GMEOW's
        // IRI-named-individual convention an object-property value IS an IRI, so the node-kind is
        // definitionally satisfied — not an enforcement the projection must reproduce.
        let stripped = ShapeRead {
            ir: strip_redundant_iri_nodekind(&read.ir, &self.object_properties),
            unsupported: read.unsupported.clone(),
        };
        let read = &stripped;
        // A residue that is ONLY `sh:sparql` is grounded when the shape's target class carries a
        // projected `logic:` constraint shape.
        let sparql_only_residue_grounded = |unsupported: &[String]| {
            !unsupported.is_empty()
                && unsupported.iter().all(|p| p == SH_SPARQL)
                && self.formalized_shapes.contains(iri)
        };
        let formalized_failure_matches = || match &read.ir.failure_class {
            None => true,
            Some(expected) => self
                .formalized_failure_classes
                .get(iri)
                .is_some_and(|actual| actual.len() == 1 && actual.contains(expected)),
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
                    let max = self.functional_max.get(&lp.path).copied();
                    max.is_some()
                        .then(|| {
                            PropertyConstraintIr::new(
                                lp.path.clone(),
                                None,
                                max,
                                Some(ConstraintProvenance::OwlRestriction),
                                vec![],
                            )
                            .ok()
                        })
                        .flatten()
                })
                .collect();
            if props.len() == read.ir.properties.len() && !props.is_empty() {
                ValidationShapeIr::new(format!("synth:{iri}"), target.clone(), props, None).ok()
            } else {
                None
            }
        };
        match self.projected.get(target) {
            None => match synth_from_functional(read) {
                Some(synth) => {
                    let v = oracle(read, &synth);
                    if grounds(read, &synth, &v) && !v.residue_bearing {
                        Verdict::Equiv
                    } else if grounds(read, &synth, &v) {
                        Verdict::EquivResidue(v.unsupported.clone())
                    } else {
                        Verdict::NoProjectedPeer
                    }
                }
                None if sparql_only_residue_grounded(&read.unsupported)
                    && formalized_failure_matches() =>
                {
                    // Pure procedural constraints intentionally have no declarative class-shape
                    // peer. An exact logic:formalizes link proves that the canonical constraint
                    // projects the legacy shape's only enforcement residue.
                    Verdict::EquivGroundedResidue
                }
                None => Verdict::NoProjectedPeer,
            },
            Some((_, proj)) => {
                if read.ir.failure_class != proj.ir.failure_class {
                    return Verdict::NotEquiv(format!(
                        "typed failure class differs: legacy={:?}, projected={:?}",
                        read.ir.failure_class, proj.ir.failure_class
                    ));
                }
                // Fold only functional-property maxima. A legacy minimum is never invented here:
                // it must already be present in the projected IR. This prevents a hand-authored
                // `sh:minCount` from receiving credit merely because the legacy shape asserted it.
                // The fold is CREDIT-ONLY: it fires only when the legacy shape authored the SAME
                // per-path bound the functional shape carries. A legacy shape that authored NO max
                // must never be compared against a folded one — the functional bound rides its own
                // global `sh:targetSubjectsOf` shape whether or not the legacy block exists, so
                // attributing it to the class comparison would manufacture a spurious tightening.
                let mut proj_ir = proj.ir.clone();
                for pc in proj_ir.properties.iter_mut() {
                    let legacy_max = read
                        .ir
                        .properties
                        .iter()
                        .find(|lp| lp.path == pc.path)
                        .and_then(|lp| lp.max_count);
                    if pc.max_count.is_none()
                        && let Some(&m) = self.functional_max.get(&pc.path)
                        && legacy_max == Some(m)
                    {
                        pc.max_count = Some(m);
                    }
                    // Credit a legacy `sh:nodeKind` implied by a co-present projected component:
                    // a class-typed value is an IRI, a datatype-typed value is a literal.
                    let legacy_nk = read
                        .ir
                        .properties
                        .iter()
                        .find(|lp| lp.path == pc.path)
                        .and_then(|lp| {
                            lp.components.iter().find_map(|c| match c {
                                ConstraintComponent::NodeKindShacl(k) => Some(*k),
                                _ => None,
                            })
                        });
                    if let Some(nk) = legacy_nk {
                        let has =
                            |pred: fn(&ConstraintComponent) -> bool| pc.components.iter().any(pred);
                        let has_nk = has(|c| matches!(c, ConstraintComponent::NodeKindShacl(_)));
                        let implied = (matches!(nk, ShaclNodeKind::Iri)
                            && has(|c| matches!(c, ConstraintComponent::Class(_))))
                            || (matches!(nk, ShaclNodeKind::Literal)
                                && has(|c| matches!(c, ConstraintComponent::Datatype(_))));
                        if !has_nk && implied {
                            pc.components.push(ConstraintComponent::NodeKindShacl(nk));
                        }
                    }
                }
                // Add functional/existence-covered legacy paths the projected class shape omits
                // ENTIRELY (a functional max-1 rides only its `sh:targetSubjectsOf` shape).
                for lp in &read.ir.properties {
                    if proj_ir.properties.iter().all(|pc| pc.path != lp.path) {
                        let max = self.functional_max.get(&lp.path).copied();
                        if max.is_some()
                            && let Ok(p) = PropertyConstraintIr::new(
                                lp.path.clone(),
                                None,
                                max,
                                Some(ConstraintProvenance::OwlRestriction),
                                vec![],
                            )
                        {
                            proj_ir.properties.push(p);
                        }
                    }
                }
                // Drop node-level components the projection derives that the legacy shape did not
                // carry — additional orthogonal enforcement, never a tightening.
                proj_ir
                    .node_components
                    .retain(|c| read.ir.node_components.contains(c));
                let v = oracle(read, &proj_ir);
                let grounded = grounds(read, &proj_ir, &v);
                if grounded && !v.residue_bearing {
                    Verdict::Equiv
                } else if grounded && sparql_only_residue_grounded(&v.unsupported) {
                    Verdict::EquivGroundedResidue
                } else if grounded {
                    Verdict::EquivResidue(v.unsupported.clone())
                } else {
                    Verdict::NotEquiv(v.reason)
                }
            }
        }
    }
}

/// The scanned scope: `--path <dir>` (repo-relative or absolute) or every slice by default.
fn scan_scope(root: &Path, path: Option<&Path>) -> PathBuf {
    match path {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => root.join(p),
        None => root.join("slices"),
    }
}

/// `gmeow-dev shape-equivalence [--path <dir>]` — the wired command body.
pub fn shape_equivalence(path: Option<&Path>) -> i32 {
    let root = project_root();
    let ctx = match OracleCtx::load(&root, "shape-equivalence") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let scan_root = scan_scope(&root, path);
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
        let legacy = read_shapes(&ds, &mut read_errors);
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
        for (iri, read) in &legacy {
            let target = &read.ir.target;
            total += 1;
            let verdict = ctx.verdict(iri, target, read);
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

/// `gmeow-dev shape-lift [--path <dir>]` — the LIFT (the projector's Galois lower adjoint) as a
/// migration review tool. For every legacy `shapes.ttl` shape in scope it proposes the OWL/RDFS
/// axioms whose forward `derive_validation_shapes` would reproduce the shape, and CERTIFIES the
/// proposal by re-deriving the shape from it (`lift` then `derive` re-attains the covered fragment
/// — the `certify` round-trip). The proposal is human-review output that seeds the module.ttl
/// grounding a `shape-equivalence` run then proves `EQUIV`; it is NEVER written to `slices/` (the
/// canon, not a lift, is the authoring ground — Principle 4). The `residue` names the components no
/// OWL antecedent can carry (a genuine ValidationOnly obligation to author in the `logic:` canon).
///
/// Exits non-zero when any proposal fails to certify (a lift that does not re-derive its own shape
/// would be an unsound migration suggestion) or a shape cannot be read.
pub fn shape_lift(path: Option<&Path>) -> i32 {
    let root = project_root();
    let scan_root = match path {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => root.join(p),
        None => root.join("slices"),
    };
    let mut legacy_files = Vec::new();
    collect_legacy_shape_files(&scan_root, &mut legacy_files);

    let mut total = 0usize;
    let mut uncertified = 0usize;
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
        let legacy = read_shapes(&ds, &mut read_errors);
        for e in &read_errors {
            eprintln!(
                "shape-lift: {}: legacy shape read error: {e}",
                rel(&root, file)
            );
            had_error = true;
        }
        if legacy.is_empty() {
            continue;
        }
        println!("{}", rel(&root, file));
        for (iri, read) in &legacy {
            total += 1;
            let proposal = lift(&read.ir);
            println!("  # {iri} — proposed OWL antecedent:");
            for line in proposal.axioms_ttl.lines() {
                println!("  {line}");
            }
            if !proposal.residue.is_empty() {
                println!(
                    "  # residue (no OWL antecedent — author in the logic: canon as ValidationOnly): {}",
                    proposal.residue.join(", ")
                );
            }
            if let Err(e) = certify(&read.ir) {
                eprintln!(
                    "shape-lift: {iri}: the lifted proposal does not re-derive the shape: {e}"
                );
                uncertified += 1;
            }
        }
    }
    println!(
        "shape-lift: proposed OWL for {total} legacy shape(s); {uncertified} failed to certify."
    );
    if had_error || uncertified > 0 { 1 } else { 0 }
}

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

/// Recursively collect every authored `module.ttl` under `dir`.
fn collect_module_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_module_files(&path, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("module.ttl") {
            out.push(path);
        }
    }
}

/// The set of class IRIs a single authored file DECLARES (`<K> a owl:Class`).
fn declared_class_iris(ds: &RdfDataset) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let (Some(ty), Some(cls)) = (
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(OWL_CLASS)),
    ) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(ty), Some(cls), GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            out.insert(s.to_owned());
        }
    }
    out
}

/// Build `class IRI → declaring module.ttl` over every authored slice module and the root ontology,
/// so a shape's target class routes its lifted OWL restrictions to the file that already declares the
/// class (`design/LOGIC-VALIDATION.md`: an author adds a constraint by writing ordinary slice OWL).
fn class_owner_modules(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut modules = Vec::new();
    collect_module_files(&root.join("slices"), &mut modules);
    let ont = root.join("ontology/gmeow.ttl");
    if ont.is_file() {
        modules.push(ont);
    }
    let mut owner = BTreeMap::new();
    for m in modules {
        if let Ok(ds) = parse_ttl_file(&m) {
            for k in declared_class_iris(&ds) {
                owner.entry(k).or_insert_with(|| m.clone());
            }
        }
    }
    owner
}

const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// The reasoner-safe OWL a shape lowers to: class-level `rdfs:subClassOf` restriction / node axioms
/// (routed to the target class's owner module) and per-property `owl:FunctionalProperty` declarations
/// (routed to each property's owner module), plus the residue that has no reasoner-safe antecedent.
struct MigrateEmit {
    class_stmts: Vec<String>,
    residue: Vec<String>,
}

/// Whether a component is a datatype facet (`MinLength`/`MaxLength`/`NumericRange`/`PrecisionRange`).
/// A facet lowers to an `owl:onDatatype` + `owl:withRestrictions` filler — which the reasoner treats
/// as OUT of its EL/DL fragment (the whole facet vocabulary is `unsupported`), so the migrator NEVER
/// emits it and records the facet as residue instead.
fn is_facet(c: &ConstraintComponent) -> bool {
    matches!(
        c,
        ConstraintComponent::MinLength(_)
            | ConstraintComponent::MaxLength(_)
            | ConstraintComponent::NumericRange { .. }
            | ConstraintComponent::PrecisionRange { .. }
    )
}

/// Whether a component constrains the path's values to LITERALS: a literal `sh:hasValue`, an
/// `sh:datatype`, `sh:nodeKind sh:Literal`, a datatype facet (length/numeric/precision/datetime
/// range), `sh:languageIn`, or an `sh:in` set with a literal member. Such a path can never satisfy
/// `owl:allValuesFrom owl:Thing` (the derive projects it to `sh:nodeKind sh:BlankNodeOrIRI`, which
/// rejects every literal), so no owl:Thing carrier may be emitted for it.
fn constrains_to_literals(c: &ConstraintComponent) -> bool {
    is_facet(c)
        || matches!(
            c,
            ConstraintComponent::HasValue(ShapeValue::Literal { .. })
                | ConstraintComponent::Datatype(_)
                | ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal)
                | ConstraintComponent::DateTimeRange { .. }
                | ConstraintComponent::LanguageIn(_)
        )
        || matches!(c, ConstraintComponent::In(vals) if vals
            .iter()
            .any(|v| matches!(v, ShapeValue::Literal { .. })))
}

/// A `<K> rdfs:subClassOf [ a owl:Restriction ; owl:onProperty <P> ; owl:<pred> <filler> ] .` axiom.
fn restriction(k: &str, p: &str, pred: &str, filler: &str) -> String {
    format!(
        "<{k}> rdfs:subClassOf [ a owl:Restriction ; owl:onProperty <{p}> ; owl:{pred} {filler} ] ."
    )
}

/// Explicit class-scoped closed-world authority for projecting a required path without adding
/// an OWL existential (and therefore without minting reasoner witnesses into the closure).
fn required_path_closure(k: &str, p: &str) -> String {
    format!(
        "[ a <https://blackcatinformatics.ca/logic/ClosureEntry> ; <https://blackcatinformatics.ca/logic/onClass> <{k}> ; <https://blackcatinformatics.ca/logic/closureKey> \"{p}\" ; <https://blackcatinformatics.ca/logic/closureValue> <https://blackcatinformatics.ca/logic/ClosedWorldClosure> ] ."
    )
}

/// Lower a `Class`-target shape IR to reasoner-safe OWL (`design/LOGIC-VALIDATION.md` grounding
/// style: existence → class-scoped closed-world authority, universal type / range →
/// `owl:allValuesFrom` (+ a faceted
/// datatype for a numeric bound), at-most-one → `owl:FunctionalProperty` (never the cardinality
/// family, which is conditionally out of the reasoner's EL/DL fragment). Anything with no
/// reasoner-safe antecedent — a pattern, a non-class `sh:not`, a value-keyed / cross-node condition,
/// an uncredited bare `sh:nodeKind`, a max ≠ 1 count, a property that cannot be corpus-consistently
/// functional, a bare existence on a path that is not a declared `owl:ObjectProperty` — is recorded
/// as residue for targeted authoring (a FOL family or a `logic:formalizes` backing), never emitted
/// as an out-of-fragment or category-error axiom.
fn reasoner_safe_emit(
    k: &str,
    ir: &ValidationShapeIr,
    functional_safe: &std::collections::BTreeSet<String>,
    object_properties: &std::collections::BTreeSet<String>,
    data: &InstanceData,
) -> MigrateEmit {
    let mut class_stmts = Vec::new();
    let mut residue = Vec::new();

    for pc in &ir.properties {
        let p = &pc.path;
        if pc.inverse || pc.reifier_shape.is_some() || pc.reification_required {
            residue.push(format!(
                "{p}: inverse/reifier obligation (no reasoner-safe class antecedent)"
            ));
            continue;
        }
        let class_c = pc.components.iter().find_map(|c| match c {
            ConstraintComponent::Class(c) => Some(c.clone()),
            _ => None,
        });
        let datatype = pc.components.iter().find_map(|c| match c {
            ConstraintComponent::Datatype(d) => Some(d.clone()),
            _ => None,
        });
        // A `sh:nodeKind` with no `sh:class`/`sh:datatype` grounds to the corresponding OWL top: a
        // `sh:Literal` value is an `rdfs:Literal`, a `sh:BlankNodeOrIRI` value is an `owl:Thing`. A
        // bare `sh:IRI` has no faithful universal filler (owl:Thing admits bnodes), but the oracle
        // credits it, so it needs no axiom and is not residue.
        let nodekind_filler = pc.components.iter().find_map(|c| match c {
            ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal) => {
                Some(RDFS_LITERAL.to_owned())
            }
            ConstraintComponent::NodeKindShacl(ShaclNodeKind::BlankNodeOrIri) => {
                Some(OWL_THING.to_owned())
            }
            _ => None,
        });
        let min_present = pc.min_count.is_some_and(|m| m >= 1);

        // The type/range filler + its restriction predicate. A datatype grounds to a BARE
        // `owl:allValuesFrom <D>` — the facet vocabulary (numeric/length range) is out of the
        // reasoner's fragment and is recorded as residue below, never emitted.
        let mut universal_filler: Option<String> = None;
        if let Some(c) = &class_c {
            // Data-soundness gate: a class obligation the real data violates would red `make validate`
            // once projected + enforced (the hand-authored shape was never enforced). Skip it.
            if data.class_obligation_conforms(k, p, c) {
                universal_filler = Some(c.clone());
            } else {
                residue.push(format!("{p}: sh:class {c} — example data has a non-conforming value (would red make validate); left ungrounded"));
            }
        } else if let Some(dt) = &datatype {
            // The derive recognises xsd:-namespaced and rdfs:Literal fillers as datatypes (sh:datatype,
            // which a literal satisfies). Any OTHER datatype IRI (e.g. rdf:langString) derives to
            // sh:class — which a literal value can NEVER satisfy — so gate it with the same
            // data-conformance check, and leave it ungrounded when the data carries a literal value.
            let recognized =
                dt.starts_with("http://www.w3.org/2001/XMLSchema#") || dt == RDFS_LITERAL;
            if recognized || data.class_obligation_conforms(k, p, dt) {
                universal_filler = Some(dt.clone());
            } else {
                residue.push(format!("{p}: datatype {dt} derives to sh:class (unrecognised by the projector) and the data has a literal value; left ungrounded"));
            }
        } else if let Some(nk) = &nodekind_filler {
            universal_filler = Some(nk.clone());
        } else if min_present {
            // Bare existence with no class/datatype/node-kind filler. The vacuous
            // `owl:allValuesFrom owl:Thing` carrier is sound ONLY for a declared
            // owl:ObjectProperty (GMEOW individuals are IRI-named, so the derived
            // `sh:nodeKind sh:BlankNodeOrIRI` enforces nothing new). On a datatype-valued or
            // untyped path the same axiom is a category error — the derived node-kind rejects
            // every literal value — so the un-projectable existence is residue instead.
            if object_properties.contains(p) && !pc.components.iter().any(constrains_to_literals) {
                universal_filler = Some(OWL_THING.to_owned());
            } else {
                residue.push(format!(
                    "{p}: sh:minCount on a non-object (literal-valued) path — owl:allValuesFrom \
                     owl:Thing would derive sh:nodeKind sh:BlankNodeOrIRI and reject every literal; \
                     left as residue"
                ));
            }
        }
        if let Some(f) = &universal_filler {
            class_stmts.push(restriction(k, p, "allValuesFrom", &format!("<{f}>")));
            // The class-scoped closed-world authority projects `sh:minCount 1` ONLY when it rides
            // an `owl:allValuesFrom` restriction on the same (class, path); without the carrier the
            // entry is inert AND its bare `closureKey` opts the property's rdfs:domain/range into
            // the closed-world reading — a global side effect no legacy class shape asked for.
            if min_present {
                class_stmts.push(required_path_closure(k, p));
            }
        }

        // At-most-one is credited ONLY when the property is ALREADY declared owl:FunctionalProperty
        // (the oracle folds its projected sh:targetSubjectsOf max bound). The migrator NEVER declares
        // a NEW functional property: functionality is a GLOBAL semantic characteristic that a
        // class-scoped `sh:maxCount 1` does not entail, and mis-declaring it reds `make validate` (a
        // multi-valued datum on another class) or a domain test. A max-1 on a not-already-functional
        // property, and any max > 1, is residue.
        match pc.max_count {
            Some(1) if functional_safe.contains(p) => {}
            Some(1) => {
                residue.push(format!("{p}: sh:maxCount 1 but the property is not already owl:FunctionalProperty — declaring it would be an unsound global claim; left as residue"));
            }
            Some(n) if n > 1 => {
                residue.push(format!(
                    "{p}: sh:maxCount {n} (> 1) has no reasoner-safe antecedent"
                ));
            }
            _ => {}
        }

        // Fixed value → owl:hasValue (IRI or plain literal only; a typed/lang literal has none).
        for c in &pc.components {
            match c {
                ConstraintComponent::HasValue(ShapeValue::Iri(v)) => {
                    class_stmts.push(restriction(k, p, "hasValue", &format!("<{v}>")));
                }
                ConstraintComponent::HasValue(ShapeValue::Literal {
                    lexical,
                    datatype: None,
                    lang: None,
                }) => {
                    class_stmts.push(restriction(
                        k,
                        p,
                        "hasValue",
                        &format!("\"{}\"", lexical.replace('\\', "\\\\").replace('"', "\\\"")),
                    ));
                }
                ConstraintComponent::HasValue(_) => {
                    residue.push(format!(
                        "{p}: sh:hasValue with a typed/lang literal has no owl:hasValue antecedent"
                    ));
                }
                ConstraintComponent::NodeKindShacl(nk) => {
                    // Literal → rdfs:Literal and BlankNodeOrIRI → owl:Thing are grounded above. A
                    // sh:IRI is credited by the oracle ONLY when a co-present sh:class makes the value
                    // an IRI-named individual; a BARE sh:IRI (no class) has no OWL antecedent for
                    // "any IRI" and the oracle does not credit it — genuine ValidationOnly residue.
                    if matches!(nk, ShaclNodeKind::Iri) && class_c.is_none() {
                        // Credited (and stripped by the oracle) when the path is an object property:
                        // an object-property value is definitionally an IRI in GMEOW. Only a bare
                        // sh:IRI on a NON-object property is genuine ValidationOnly residue.
                        if !object_properties.contains(p) {
                            residue.push(format!("{p}: bare sh:nodeKind sh:IRI on a non-object property (ValidationOnly, needs logic: backing)"));
                        }
                    } else if !matches!(
                        nk,
                        ShaclNodeKind::Iri | ShaclNodeKind::Literal | ShaclNodeKind::BlankNodeOrIri
                    ) {
                        residue.push(format!(
                            "{p}: sh:nodeKind {} has no reasoner-safe filler",
                            nk.as_str()
                        ));
                    }
                }
                ConstraintComponent::Pattern { regex, .. } => {
                    residue.push(format!("{p}: sh:pattern {regex} (regex-dialect residue)"));
                }
                _ if is_facet(c) => {
                    residue.push(format!("{p}: datatype facet (numeric/length range) is out of the reasoner fragment — ValidationOnly residue"));
                }
                ConstraintComponent::LanguageIn(_) => {
                    residue.push(format!("{p}: sh:languageIn (no OWL antecedent)"))
                }
                ConstraintComponent::In(_) => residue.push(format!(
                    "{p}: sh:in value set on a property (no owl:oneOf antecedent on a path)"
                )),
                ConstraintComponent::DateTimeRange { .. } => residue.push(format!(
                    "{p}: sh:min/maxInclusive datetime range (non-numeric facet)"
                )),
                ConstraintComponent::QualifiedValueShape { .. } => residue.push(format!(
                    "{p}: sh:qualifiedValueShape (no reasoner-safe antecedent)"
                )),
                ConstraintComponent::Not(_) => residue.push(format!(
                    "{p}: property-level sh:not (no reasoner-safe antecedent)"
                )),
                _ => {}
            }
        }
    }

    // Node-level components: owl:disjointWith (sh:not [ sh:class D ]) and owl:oneOf (all-IRI sh:in).
    for c in &ir.node_components {
        match c {
            ConstraintComponent::Not(inner) => match inner.as_ref() {
                ConstraintComponent::Class(d) => {
                    class_stmts.push(format!("<{k}> owl:disjointWith <{d}> ."))
                }
                _ => residue.push(
                    "node sh:not negates a non-class shape (only owl:disjointWith grounds)"
                        .to_owned(),
                ),
            },
            ConstraintComponent::In(vals) => {
                let iris: Option<Vec<&str>> = vals
                    .iter()
                    .map(|v| match v {
                        ShapeValue::Iri(i) => Some(i.as_str()),
                        ShapeValue::Literal { .. } => None,
                    })
                    .collect();
                match iris {
                    Some(iris) if !iris.is_empty() => {
                        let members = iris
                            .iter()
                            .map(|i| format!("<{i}>"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        class_stmts.push(format!("<{k}> owl:oneOf ( {members} ) ."));
                    }
                    _ => residue.push(
                        "node sh:in carries literal members (owl:oneOf enumerates IRIs only)"
                            .to_owned(),
                    ),
                }
            }
            other => residue.push(format!(
                "node-level component {other:?} has no reasoner-safe antecedent"
            )),
        }
    }

    let _ = RDFS_LITERAL;
    MigrateEmit {
        class_stmts,
        residue,
    }
}

/// Recursively collect every authored `.ttl` under `dir` that can carry a multi-valued instance
/// datum — the example graphs and conformance fixtures AND the slice `module.ttl` files (reference
/// individuals, like the affect reference frames that each carry two `gmeow:hasAxis` values, are
/// declared in `module.ttl`, not under `examples/`). Only the pure shape/meta surfaces
/// (`shapes.ttl`, `manifest.ttl`, `structural.ttl`) are skipped — their `sh:property` / `sh:path`
/// repetition is shape syntax, not a functional-property counterexample.
fn collect_instance_data_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_instance_data_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl")
            && !matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("shapes.ttl") | Some("manifest.ttl") | Some("structural.ttl")
            )
        {
            out.push(path);
        }
    }
}

/// The asserted instance-data graph the migrator consults for its soundness gates: every subject's
/// asserted `rdf:type` set, and every `(subject, predicate) → object IRIs` edge, aggregated across
/// all authored instance data. Used to reject a grounding whose enforcement the real data violates.
struct InstanceData {
    /// subject IRI → its asserted `rdf:type` class IRIs.
    types: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// (subject IRI, predicate IRI) → the object IRIs.
    edges: BTreeMap<(String, String), Vec<String>>,
    /// (subject IRI, predicate IRI) pairs that carry a LITERAL object — a literal can never satisfy
    /// an `sh:class` obligation, so such a pair makes a class grounding non-conforming.
    literal_edges: std::collections::BTreeSet<(String, String)>,
}

impl InstanceData {
    fn load(root: &Path) -> Self {
        let mut files = Vec::new();
        collect_instance_data_files(&root.join("slices"), &mut files);
        collect_instance_data_files(&root.join("examples"), &mut files);
        let mut types: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        let mut edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut literal_edges: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        let rdf_type = RDF_TYPE;
        for f in &files {
            let Ok(ds) = parse_ttl_file(f) else { continue };
            for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
                let (TermRef::Iri(s), TermRef::Iri(p)) = (ds.resolve(q.s), ds.resolve(q.p)) else {
                    continue;
                };
                match ds.resolve(q.o) {
                    TermRef::Iri(o) if p == rdf_type => {
                        types.entry(s.to_owned()).or_default().insert(o.to_owned());
                    }
                    TermRef::Iri(o) => {
                        edges
                            .entry((s.to_owned(), p.to_owned()))
                            .or_default()
                            .push(o.to_owned());
                    }
                    TermRef::Literal { .. } => {
                        literal_edges.insert((s.to_owned(), p.to_owned()));
                    }
                    _ => {}
                }
            }
        }
        Self {
            types,
            edges,
            literal_edges,
        }
    }

    /// Whether an `sh:class C` obligation on `(K, P)` CONFORMS to the data: no `K`-instance carries a
    /// `P`-value that is not (asserted) a `C`, and no `K`-instance carries a LITERAL `P`-value (a
    /// literal cannot be an instance of `C`). Conservative on subclassing — it reads asserted types
    /// only (a value typed a subclass of `C` reads as non-conforming), so it may skip a
    /// genuinely-groundable shape rather than emit one the reasoned validator would reject.
    fn class_obligation_conforms(&self, k: &str, p: &str, c: &str) -> bool {
        for (subj, ty) in &self.types {
            if !ty.contains(k) {
                continue;
            }
            // A literal value can never satisfy sh:class.
            if self.literal_edges.contains(&(subj.clone(), p.to_owned())) {
                return false;
            }
            if let Some(vals) = self.edges.get(&(subj.clone(), p.to_owned())) {
                for v in vals {
                    if !self.types.get(v).is_some_and(|vt| vt.contains(c)) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// The set of property IRIs declared `owl:ObjectProperty` anywhere in the authored tree. In GMEOW's
/// IRI-named-individual convention an object-property value is always an IRI, so a `sh:nodeKind sh:IRI`
/// on such a path is redundant (definitionally satisfied), which the oracle and the migrator credit.
fn object_property_iris(root: &Path) -> std::collections::BTreeSet<String> {
    let mut modules = Vec::new();
    collect_module_files(&root.join("slices"), &mut modules);
    let ont = root.join("ontology/gmeow.ttl");
    if ont.is_file() {
        modules.push(ont);
    }
    let mut out = std::collections::BTreeSet::new();
    let op = "http://www.w3.org/2002/07/owl#ObjectProperty";
    for m in modules {
        let Ok(ds) = parse_ttl_file(&m) else { continue };
        let (Some(ty), Some(opo)) = (
            ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
            ds.term_id_by_value(&TermValue::iri(op)),
        ) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(ty), Some(opo), GraphMatch::Any) {
            if let TermRef::Iri(s) = ds.resolve(q.s) {
                out.insert(s.to_owned());
            }
        }
    }
    out
}

/// A copy of `ir` with every redundant `sh:nodeKind sh:IRI` component removed from an
/// `owl:ObjectProperty` forward path (GMEOW individuals are IRI-named, so it enforces nothing). A
/// property left with no cardinality and no components after the strip is dropped entirely.
fn strip_redundant_iri_nodekind(
    ir: &ValidationShapeIr,
    object_properties: &std::collections::BTreeSet<String>,
) -> ValidationShapeIr {
    let mut props = Vec::new();
    for pc in &ir.properties {
        let redundant = object_properties.contains(&pc.path)
            && !pc.inverse
            && pc.reifier_shape.is_none()
            && !pc.reification_required
            && pc
                .components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)));
        if redundant {
            let kept: Vec<ConstraintComponent> = pc
                .components
                .iter()
                .filter(|c| !matches!(c, ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)))
                .cloned()
                .collect();
            if pc.min_count.is_none() && pc.max_count.is_none() && kept.is_empty() {
                continue;
            }
            if let Ok(p) = PropertyConstraintIr::new(
                &pc.path,
                pc.min_count,
                pc.max_count,
                pc.cardinality_provenance,
                kept,
            ) {
                props.push(p);
            }
        } else {
            props.push(pc.clone());
        }
    }
    let mut stripped = ir.clone();
    stripped.properties = props;
    stripped
}

/// The set of property IRIs ALREADY declared `owl:FunctionalProperty` anywhere in the authored tree
/// (so the migrator never re-emits an existing declaration).
fn already_functional(root: &Path) -> std::collections::BTreeSet<String> {
    let mut modules = Vec::new();
    collect_module_files(&root.join("slices"), &mut modules);
    let ont = root.join("ontology/gmeow.ttl");
    if ont.is_file() {
        modules.push(ont);
    }
    let mut out = std::collections::BTreeSet::new();
    let fp = "http://www.w3.org/2002/07/owl#FunctionalProperty";
    for m in modules {
        let Ok(ds) = parse_ttl_file(&m) else { continue };
        let (Some(ty), Some(fpo)) = (
            ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
            ds.term_id_by_value(&TermValue::iri(fp)),
        ) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(ty), Some(fpo), GraphMatch::Any) {
            if let TermRef::Iri(s) = ds.resolve(q.s) {
                out.insert(s.to_owned());
            }
        }
    }
    out
}

/// Scan forward from `start` to the end index (exclusive) of the Turtle statement — the terminating
/// top-level `.` (a `.` at bracket/paren depth 0 followed by whitespace or EOF, so a bare decimal or
/// a prefixed-name dot never terminates). Skips `<IRI>`, `"…"` / `"""…"""` string literals, and `#`
/// comments so their contents never trip the scan. Returns `None` if no terminator is found.
fn statement_end(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = start;
    let mut brackets = 0i32;
    let mut parens = 0i32;
    while i < b.len() {
        match b[i] {
            b'<' => {
                i += 1;
                while i < b.len() && b[i] != b'>' {
                    i += 1;
                }
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                if text[i..].starts_with("\"\"\"") {
                    i += 3;
                    while i < b.len() && !text[i..].starts_with("\"\"\"") {
                        i += 1;
                    }
                    i += 2; // land on the last quote; the trailing `i += 1` steps past it
                } else {
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        if b[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
            }
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'(' => parens += 1,
            b')' => parens -= 1,
            b'.' if brackets == 0 && parens == 0 => {
                let boundary = b.get(i + 1).is_none_or(|&n| {
                    n == b' ' || n == b'\n' || n == b'\r' || n == b'\t' || n == b'#'
                });
                if boundary {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Locate the full top-level statement span of the subject whose IRI has local name `local`, written
/// as a `prefix:local` subject at the start of a line. Returns the `[start, end)` byte span covering
/// the whole `prefix:local … .` statement, or `None`.
fn subject_span(text: &str, local: &str) -> Option<(usize, usize)> {
    let needle = format!(":{local}");
    let mut from = 0;
    while let Some(rel) = text[from..].find(&needle) {
        let abs = from + rel;
        from = abs + 1;
        // The character after the local name must be a token boundary (not part of a longer name).
        let after = abs + needle.len();
        let boundary = text[after..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == ';' || c == '.');
        if !boundary {
            continue;
        }
        // The subject sits at column 0: everything from the line start to the colon is a prefix name.
        let line_start = text[..abs].rfind('\n').map(|n| n + 1).unwrap_or(0);
        let prefix = &text[line_start..abs];
        let is_subject = !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
        if !is_subject {
            continue;
        }
        let end = statement_end(text, line_start)?;
        return Some((line_start, end));
    }
    None
}

/// The local name of a GMEOW/authored IRI (the part after the last `/` or `#`).
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// `gmeow-dev shape-migrate [--path <dir>] [--apply]` — the INJECT phase of the automated shape
/// migration. For every hand-authored `Class`-target `sh:NodeShape` in scope it lifts the OWL/RDFS
/// antecedent (`crate::projections::lift`), classifies GROUNDABLE (empty residue — the projector
/// reproduces it wholesale) vs RESIDUE (a component with no OWL antecedent — a FOL family, a pattern,
/// a value-keyed target), and routes the restriction statements to the module.ttl that declares the
/// target class. `--apply` appends the not-yet-present statements to that module (idempotent by exact
/// text); the default is a dry-run report. Injection is class-target only — domain/range and
/// value-keyed grounding need a closure/SPARQL-target authoring step this phase leaves to review.
///
/// After an `--apply` run the caller regenerates (`make regenerate`) and prunes the now-equivalent
/// blocks (`shape-migrate --prune`); equivalence is proven by the oracle, never trusted.
pub fn shape_migrate(path: Option<&Path>, apply: bool) -> i32 {
    let root = project_root();
    let class_owner = class_owner_modules(&root);
    // A max-1 is credited only for properties ALREADY declared owl:FunctionalProperty (the oracle
    // folds their projected bound). The migrator never introduces a NEW functional characteristic.
    let functional_safe = already_functional(&root);
    let data = InstanceData::load(&root);
    let ctx = match OracleCtx::load(&root, "shape-migrate") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let scan_root = scan_scope(&root, path);
    let mut legacy_files = Vec::new();
    collect_legacy_shape_files(&scan_root, &mut legacy_files);

    // Accumulate per-module appends so a module targeted by many shapes is written once.
    let mut appends: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let (mut groundable, mut residue_only, mut skipped, mut had_error) =
        (0usize, 0usize, 0usize, false);

    for file in &legacy_files {
        let ds = match parse_ttl_file(file) {
            Ok(ds) => ds,
            Err(_) => {
                had_error = true;
                continue;
            }
        };
        let mut read_errors = Vec::new();
        let legacy = read_shapes(&ds, &mut read_errors);
        for e in &read_errors {
            eprintln!(
                "shape-migrate: {}: legacy shape read error: {e}",
                rel(&root, file)
            );
            had_error = true;
        }
        if legacy.is_empty() {
            continue;
        }
        println!("{}", rel(&root, file));
        for (iri, read) in &legacy {
            let target = &read.ir.target;
            // Only class-target shapes are injected here.
            let ShapeTarget::Class(k) = target else {
                println!(
                    "  [SKIP non-class-target] {} ({})",
                    short_iri(iri),
                    target_label(target)
                );
                skipped += 1;
                continue;
            };
            // The derive only emits validation shapes for classes/properties in an authoring
            // namespace (`gmeow_logic_compile::frontend::AUTHORING_NAMESPACES` — the single
            // dogfooding-boundary authority, covering `gmeow:`/`math:`/`lang:`/`logic:`). A shape
            // targeting a genuinely external namespace (imported ontologies such as gUFO/FOAF)
            // can never be reproduced by the projector, so injecting OWL for it is inert — skip it.
            if !gmeow_logic_compile::frontend::is_authoring_namespace(k) {
                println!(
                    "  [SKIP non-dogfooded-namespace] {} ({})",
                    short_iri(iri),
                    short_iri(k)
                );
                skipped += 1;
                continue;
            }
            // A shape the projector already reproduces needs no injection.
            if ctx.verdict(iri, target, read).is_grounded() {
                println!("  [ALREADY-GROUNDED] {}", short_iri(iri));
                continue;
            }
            let emit =
                reasoner_safe_emit(k, &read.ir, &functional_safe, &ctx.object_properties, &data);
            let is_groundable = emit.residue.is_empty();
            if is_groundable {
                groundable += 1;
            } else {
                residue_only += 1;
            }
            let tag = if is_groundable {
                "GROUNDABLE"
            } else {
                "RESIDUE"
            };
            // Route the class restrictions to the class's owner module.
            match class_owner.get(k) {
                Some(module) => {
                    if apply {
                        appends
                            .entry(module.clone())
                            .or_default()
                            .extend(emit.class_stmts.clone());
                    }
                    println!(
                        "  [{tag}] {} → {} (+{} class axiom(s))",
                        short_iri(iri),
                        rel(&root, module),
                        emit.class_stmts.len(),
                    );
                }
                None => {
                    println!(
                        "  [{tag}] {} → NO OWNER MODULE for class {} (declare it first)",
                        short_iri(iri),
                        short_iri(k),
                    );
                    skipped += 1;
                }
            }
            for r in &emit.residue {
                println!("      residue: {r}");
            }
        }
    }

    if apply {
        for (module, mut stmts) in appends {
            let mut text = match std::fs::read_to_string(&module) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("shape-migrate: cannot read {}: {e}", rel(&root, &module));
                    had_error = true;
                    continue;
                }
            };
            // Dedup within this run and against text already present (exact-line idempotence).
            stmts.sort();
            stmts.dedup();
            let fresh: Vec<String> = stmts
                .into_iter()
                .filter(|s| !text.contains(s.as_str()))
                .collect();
            if fresh.is_empty() {
                continue;
            }
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(
                "\n# --- Shape-migration grounding (projected to generated/shapes/validation-shapes.ttl) ---\n",
            );
            // Ensure the prefixes the injected statements use are declared (Turtle honours a @prefix
            // that precedes its first use, so a late declaration before these statements is valid).
            for (pfx, iri) in [
                ("owl:", "http://www.w3.org/2002/07/owl#"),
                ("rdfs:", "http://www.w3.org/2000/01/rdf-schema#"),
                ("xsd:", "http://www.w3.org/2001/XMLSchema#"),
            ] {
                if !text.contains(&format!("@prefix {pfx}")) {
                    text.push_str(&format!("@prefix {pfx} <{iri}> .\n"));
                }
            }
            for s in &fresh {
                text.push_str(s);
                text.push('\n');
            }
            if let Err(e) = std::fs::write(&module, &text) {
                eprintln!("shape-migrate: cannot write {}: {e}", rel(&root, &module));
                had_error = true;
                continue;
            }
            eprintln!(
                "shape-migrate: appended {} statement(s) to {}",
                fresh.len(),
                rel(&root, &module),
            );
        }
    }

    eprintln!(
        "shape-migrate: {} groundable, {} residue-bearing, {} skipped{}.",
        groundable,
        residue_only,
        skipped,
        if apply {
            " (applied)"
        } else {
            " (dry-run — pass --apply to inject)"
        },
    );
    if had_error { 1 } else { 0 }
}

/// `gmeow-dev shape-migrate --prune [--path <dir>] [--apply]` — the PRUNE phase. After a regenerate,
/// it re-runs the oracle over the freshly-projected surface and DELETES every hand-authored
/// `shapes.ttl` block whose verdict is `EQUIV` or `EQUIV-GROUNDED-RESIDUE` (proven-redundant
/// projections). `--apply` rewrites the files; the default lists what would be deleted. A block is
/// removed only when the projector reproduces it — equivalence-before-deletion, enforced by the tool.
pub fn shape_prune(path: Option<&Path>, apply: bool) -> i32 {
    let root = project_root();
    let ctx = match OracleCtx::load(&root, "shape-migrate") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let scan_root = scan_scope(&root, path);
    let mut legacy_files = Vec::new();
    collect_legacy_shape_files(&scan_root, &mut legacy_files);

    let (mut deletable, mut kept, mut had_error) = (0usize, 0usize, false);
    for file in &legacy_files {
        let ds = match parse_ttl_file(file) {
            Ok(ds) => ds,
            Err(_) => {
                had_error = true;
                continue;
            }
        };
        let mut read_errors = Vec::new();
        let legacy = read_shapes(&ds, &mut read_errors);
        for e in &read_errors {
            eprintln!(
                "shape-migrate: {}: legacy shape read error: {e}",
                rel(&root, file)
            );
            had_error = true;
        }
        if legacy.is_empty() {
            continue;
        }
        // Collect the IRIs of blocks proven redundant in THIS file.
        let mut to_delete: Vec<String> = Vec::new();
        for (iri, read) in &legacy {
            let target = &read.ir.target;
            let v = ctx.verdict(iri, target, read);
            if matches!(v, Verdict::Equiv | Verdict::EquivGroundedResidue) {
                to_delete.push(iri.clone());
                deletable += 1;
            } else {
                kept += 1;
            }
        }
        if to_delete.is_empty() {
            continue;
        }
        println!("{}", rel(&root, file));
        for iri in &to_delete {
            println!("  [DELETE] {}", short_iri(iri));
        }
        if apply {
            let mut text = match std::fs::read_to_string(file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("shape-migrate: cannot read {}: {e}", rel(&root, file));
                    had_error = true;
                    continue;
                }
            };
            // Delete longest-offset-first so earlier spans stay valid as we splice.
            let mut spans: Vec<(usize, usize)> = to_delete
                .iter()
                .filter_map(|iri| subject_span(&text, local_name(iri)))
                .collect();
            spans.sort_by_key(|s| std::cmp::Reverse(s.0));
            for (mut s, mut e) in spans {
                // Consume the trailing newline and one following blank line for tidy output.
                while e < text.len() && (text.as_bytes()[e] == b'\n' || text.as_bytes()[e] == b'\r')
                {
                    e += 1;
                    if text[..e].ends_with("\n\n") {
                        break;
                    }
                }
                // Consume a run of immediately-preceding blank lines the block leaves behind.
                while s >= 2 && text[..s].ends_with("\n\n") {
                    s -= 1;
                }
                text.replace_range(s..e, "");
            }
            if let Err(err) = std::fs::write(file, &text) {
                eprintln!("shape-migrate: cannot write {}: {err}", rel(&root, file));
                had_error = true;
            }
        }
    }

    eprintln!(
        "shape-migrate --prune: {} deletable (proven EQUIV), {} kept{}.",
        deletable,
        kept,
        if apply {
            " (applied)"
        } else {
            " (dry-run — pass --apply to delete)"
        },
    );
    if had_error { 1 } else { 0 }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_shapes_with_the_same_target_are_all_retained() {
        let ds = parse_dataset(
            br#"
            @prefix ex: <https://example.test/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:First a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:name ; sh:minCount 1 ] .
            ex:Second a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:code ; sh:minCount 1 ] .
            "#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
        let mut errors = Vec::new();
        let shapes = read_shapes(&ds, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(
            shapes.len(),
            2,
            "no authored shape may disappear by target-key collision"
        );
        assert_eq!(shapes[0].0, "https://example.test/First");
        assert_eq!(shapes[1].0, "https://example.test/Second");
    }

    #[test]
    fn projected_shapes_with_the_same_target_are_rejected() {
        let dataset = parse_dataset(
            br#"
            @prefix ex: <https://example.test/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:First a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:name ; sh:minCount 1 ] .
            ex:Second a sh:NodeShape ;
                sh:targetClass ex:Widget ;
                sh:property [ sh:path ex:code ; sh:minCount 1 ] .
            "#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
        let mut errors = Vec::new();
        let projected = shapes_by_target(&dataset, &mut errors);
        assert_eq!(projected.len(), 1);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("share target"));
    }

    const K: &str = "https://example.test/Constant";
    const P: &str = "https://example.test/isExact";

    fn empty_data() -> InstanceData {
        InstanceData {
            types: BTreeMap::new(),
            edges: BTreeMap::new(),
            literal_edges: std::collections::BTreeSet::new(),
        }
    }

    fn min_plus_hasvalue_shape() -> ValidationShapeIr {
        let pc = PropertyConstraintIr::new(
            P,
            Some(1),
            None,
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::HasValue(ShapeValue::Literal {
                lexical: "true".to_owned(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".to_owned()),
                lang: None,
            })],
        )
        .expect("property constraint builds");
        ValidationShapeIr::new(
            "https://example.test/ConstantShape".to_owned(),
            ShapeTarget::Class(K.to_owned()),
            vec![pc],
            None,
        )
        .expect("shape IR builds")
    }

    #[test]
    fn datatype_valued_existence_never_fabricates_owl_thing() {
        // A shape on a declared owl:DatatypeProperty (i.e. NOT in the object-property set) with
        // only minCount + a literal sh:hasValue: `owl:allValuesFrom owl:Thing` derives
        // `sh:nodeKind sh:BlankNodeOrIRI`, which every literal value violates — the existence
        // must be residue, with no fabricated axiom and no inert closure entry.
        let empty = std::collections::BTreeSet::new();
        let emit = reasoner_safe_emit(K, &min_plus_hasvalue_shape(), &empty, &empty, &empty_data());
        assert!(
            emit.class_stmts.iter().all(|s| !s.contains(OWL_THING)),
            "fabricated owl:Thing axiom on a datatype-valued path: {:?}",
            emit.class_stmts
        );
        assert!(
            emit.class_stmts.iter().all(|s| !s.contains("ClosureEntry")),
            "closure entry without an allValuesFrom carrier is inert: {:?}",
            emit.class_stmts
        );
        assert!(
            emit.residue.iter().any(|r| r.contains("sh:minCount")),
            "the un-projectable existence must be named as residue: {:?}",
            emit.residue
        );
    }

    #[test]
    fn object_property_existence_keeps_the_owl_thing_carrier() {
        // The same bare existence on a declared owl:ObjectProperty stays sound: values are
        // IRI-named individuals, so the owl:Thing carrier (+ its closure entry) is emitted.
        let pc = PropertyConstraintIr::new(
            P,
            Some(1),
            None,
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .expect("property constraint builds");
        let ir = ValidationShapeIr::new(
            "https://example.test/ConstantShape".to_owned(),
            ShapeTarget::Class(K.to_owned()),
            vec![pc],
            None,
        )
        .expect("shape IR builds");
        let functional = std::collections::BTreeSet::new();
        let objects: std::collections::BTreeSet<String> = std::iter::once(P.to_owned()).collect();
        let emit = reasoner_safe_emit(K, &ir, &functional, &objects, &empty_data());
        assert!(
            emit.class_stmts
                .iter()
                .any(|s| s.contains(&format!("allValuesFrom <{OWL_THING}>"))),
            "object-property existence keeps its universal carrier: {:?}",
            emit.class_stmts
        );
        assert!(
            emit.class_stmts.iter().any(|s| s.contains("ClosureEntry")),
            "the carrier's closure entry projects the sh:minCount: {:?}",
            emit.class_stmts
        );
        assert!(emit.residue.is_empty(), "residue: {:?}", emit.residue);
    }
}
