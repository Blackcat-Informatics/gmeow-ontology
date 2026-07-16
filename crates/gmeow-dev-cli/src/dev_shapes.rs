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
use gmeow_logic_compile::projections::subsumption::{enforcement_key, subsumes};
use gmeow_validate::shape_oracle::{
    OracleVerdict, RAW_SPARQL_TARGET_RESIDUE, ShapeRead, TARGETLESS_SELECT, oracle,
    read_shacl_shape, semantic_cross_check, semantic_witness_plan, shape_subgraph_ttl,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue, parse_dataset};

use crate::dev_common::{fail, project_root};

const SH_NODESHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SH_SPARQL: &str = "http://www.w3.org/ns/shacl#sparql";
const SH_OR: &str = "http://www.w3.org/ns/shacl#or";
/// `sh:node` / `sh:xone` — structural, machine-readable residue constructs. Their grounded
/// clearance is stricter than the `sh:sparql`/`sh:or` trust anchor: the exact `logic:formalizes`
/// record must ALSO survive the semantic witness cross-check (near-misses generated FROM the
/// construct, focus-flag agreement required on the projected constraint surface).
const SH_NODE: &str = "http://www.w3.org/ns/shacl#node";
const SH_XONE: &str = "http://www.w3.org/ns/shacl#xone";
const SH_TARGET: &str = "http://www.w3.org/ns/shacl#target";
const SH_SELECT: &str = "http://www.w3.org/ns/shacl#select";
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
/// Re-exported from the shared shape-grounding module (one authority for the record
/// vocabulary across the migration oracle and the pipeline's grounding ledger).
const LOGIC_FORMALIZES: &str = gmeow_validate::shape_grounding::LOGIC_FORMALIZES;

/// The verdict for one legacy shape.
enum Verdict {
    /// Covered fragment equivalent, no residue — deletable.
    Equiv,
    /// Covered fragment equivalent; its only residue is a cross-node `sh:sparql` check that is
    /// reproduced by a projected `logic:`-backed constraint shape on the same target — grounded and
    /// deletable.
    EquivGroundedResidue,
    /// Grounded through the SEMANTIC discipline for structural residue (`sh:node` / `sh:xone` /
    /// a raw `sh:SPARQLTarget`): the exact `logic:formalizes` record is present, the typed
    /// failure class matches, AND the record's projected constraint surface reproduced the
    /// construct's semantics under the witness cross-check — grounded and deletable.
    EquivGroundedResidueSemantic,
    /// Covered fragment equivalent but residue-bearing with residue that is NOT (yet) grounded as a
    /// canonical `logic:` constraint — NOT deletable.
    EquivResidue(Vec<String>),
    /// Covered fragments diverge (the projector does not reproduce it).
    NotEquiv(String),
    /// No projected shape targets the same focus nodes.
    NoProjectedPeer,
}

impl Verdict {
    /// A covered-fragment match with no residue, OR with residue grounded by an exact
    /// `logic:formalizes` record (the `sh:sparql`/`sh:or` trust anchor, or the semantic
    /// witness-cross-checked discipline), clears a legacy block for deletion.
    fn is_grounded(&self) -> bool {
        matches!(
            self,
            Verdict::Equiv | Verdict::EquivGroundedResidue | Verdict::EquivGroundedResidueSemantic
        )
    }

    fn label(&self) -> String {
        match self {
            Verdict::Equiv => "EQUIV".to_owned(),
            Verdict::EquivGroundedResidue => {
                "EQUIV-GROUNDED-RESIDUE(sh:sparql→constraint)".to_owned()
            }
            Verdict::EquivGroundedResidueSemantic => {
                "EQUIV-GROUNDED-RESIDUE(record+witness-cross-check)".to_owned()
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
    gmeow_validate::shape_grounding::formalizes_records(ds)
        .into_values()
        .flatten()
        .collect()
}

/// Failure classes carried by each exact `logic:formalizes` replacement, keyed by the
/// FORMALIZED (legacy) IRI. This keeps typed conformance diagnostics part of the migration
/// proof even though they do not alter which data graph conforms. Derived from the shared
/// shape-grounding record scan (one implementation with the pipeline's grounding ledger).
fn collect_formalized_failure_classes(
    ds: &RdfDataset,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let records = gmeow_validate::shape_grounding::formalizes_records(ds);
    let by_record = gmeow_validate::shape_grounding::record_failure_classes(ds);
    let mut out: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for (record, legacies) in records {
        if let Some(failures) = by_record.get(&record) {
            for legacy in legacies {
                out.entry(legacy)
                    .or_default()
                    .extend(failures.iter().cloned());
            }
        }
    }
    out
}

/// α-canonicalize one SPARQL `sh:select` body for the raw-target identity clearance: tokenize
/// (IRIs whole; `{ } ( ) [ ] , ;` and a standalone `.` as single-char tokens), fix the focus
/// variable (`$this` ≡ `?this`), rename every other variable to `?vN` in first-occurrence order,
/// read each anonymous `[]` node as a FRESH variable (SPARQL bnode-in-query semantics), and
/// uppercase bare keyword tokens. Two selects with equal canonical forms select/flag exactly the
/// same focus nodes over every graph — variable names, whitespace, and keyword case are the only
/// degrees of freedom this normalization removes, so equality is a SOUND (never over-claiming)
/// witness of semantic identity.
fn sparql_alpha_canonical(select: &str) -> String {
    // Tokenize.
    let mut toks: Vec<String> = Vec::new();
    let mut chars = select.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '<' {
            let mut t = String::new();
            for ch in chars.by_ref() {
                t.push(ch);
                if ch == '>' {
                    break;
                }
            }
            toks.push(t);
        } else if matches!(c, '{' | '}' | '(' | ')' | '[' | ']' | ',' | ';' | '.') {
            toks.push(c.to_string());
            chars.next();
        } else {
            let mut t = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || matches!(ch, '{' | '}' | '(' | ')' | '[' | ']' | ',' | ';')
                {
                    break;
                }
                t.push(ch);
                chars.next();
            }
            // A trailing statement-terminating '.' is its own token (IRIs are consumed whole
            // above, and the corpus selects carry no decimal literals).
            while t.ends_with('.') {
                t.pop();
                if !t.is_empty() {
                    toks.push(std::mem::take(&mut t));
                }
                toks.push(".".to_owned());
            }
            if !t.is_empty() {
                toks.push(t);
            }
        }
    }
    // α-rename.
    let mut rename: BTreeMap<String, String> = BTreeMap::new();
    let mut fresh = 0usize;
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        let t = &toks[i];
        if t == "." {
            // A statement-terminating '.' is pure SPARQL syntax (an optional separator between
            // triple patterns); two selects that differ only in trailing terminators are
            // α-equivalent, so it never contributes to the identity key.
            i += 1;
            continue;
        }
        if t == "$this" || t == "?this" {
            out.push("?this".to_owned());
        } else if t == "[" && toks.get(i + 1).map(String::as_str) == Some("]") {
            out.push(format!("?v{fresh}"));
            fresh += 1;
            i += 2;
            continue;
        } else if let Some(rest) = t.strip_prefix('?').or_else(|| t.strip_prefix('$')) {
            let key = rest.to_owned();
            let name = rename.entry(key).or_insert_with(|| {
                let n = format!("?v{fresh}");
                fresh += 1;
                n
            });
            out.push(name.clone());
        } else if t.chars().all(|ch| ch.is_ascii_alphabetic()) {
            out.push(t.to_ascii_uppercase());
        } else {
            out.push(t.clone());
        }
        i += 1;
    }
    out.join(" ")
}

/// The `sh:select` bodies of every `sh:sparql` constraint on `subject`, sorted for set
/// comparison.
fn sparql_constraint_selects(ds: &RdfDataset, subject: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(sid), Some(sparql), Some(select)) = (
        ds.term_id_by_value(&TermValue::iri(subject)),
        ds.term_id_by_value(&TermValue::iri(SH_SPARQL)),
        ds.term_id_by_value(&TermValue::iri(SH_SELECT)),
    ) else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(sid), Some(sparql), None, GraphMatch::Any) {
        for sel in ds.quads_for_pattern(Some(q.o), Some(select), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = ds.resolve(sel.o) {
                out.push(lexical.to_owned());
            }
        }
    }
    out.sort();
    out
}

/// The raw `sh:target [ … sh:select "…" ]` body on `subject`, when it carries exactly one.
fn raw_target_select(ds: &RdfDataset, subject: &str) -> Option<String> {
    let (sid, target, select) = (
        ds.term_id_by_value(&TermValue::iri(subject))?,
        ds.term_id_by_value(&TermValue::iri(SH_TARGET))?,
        ds.term_id_by_value(&TermValue::iri(SH_SELECT))?,
    );
    let mut found: Option<String> = None;
    for q in ds.quads_for_pattern(Some(sid), Some(target), None, GraphMatch::Any) {
        for sel in ds.quads_for_pattern(Some(q.o), Some(select), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = ds.resolve(sel.o) {
                if found.is_some() {
                    return None; // ambiguous — never ground on a multi-target guess
                }
                found = Some(lexical.to_owned());
            }
        }
    }
    found
}

/// The canonical enforcement key of a raw-SPARQL-target block: the α-canonical target select
/// plus the sorted α-canonical `sh:sparql` select set.
fn raw_sparql_block_key(target_select: &str, constraint_selects: &[String]) -> String {
    let mut selects: Vec<String> = constraint_selects
        .iter()
        .map(|s| sparql_alpha_canonical(s))
        .collect();
    selects.sort();
    format!(
        "target={} constraints={}",
        sparql_alpha_canonical(target_select),
        selects.join(" | ")
    )
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

/// Whether an authored `.ttl` file DECLARES at least one `sh:NodeShape` (an actual
/// `?s rdf:type sh:NodeShape` quad — a mention inside a comment or a prose literal never
/// counts). A cheap `NodeShape` text pre-filter (the local name is prefix-independent) bounds
/// the parses to the handful of shape-bearing files. A file that PASSES the pre-filter but
/// fails to parse is INCLUDED so the scan surfaces the parse error instead of skipping it in
/// silence (hard-fail, never paper over).
fn declares_node_shape(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        // An unreadable candidate is included so the scan reports the read error.
        return true;
    };
    if !text.contains("NodeShape") {
        return false;
    }
    let Ok(ds) = parse_dataset(text.as_bytes(), "text/turtle", None) else {
        return true;
    };
    let (Some(ty), Some(ns)) = (
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(SH_NODESHAPE)),
    ) else {
        return false;
    };
    ds.quads_for_pattern(None, Some(ty), Some(ns), GraphMatch::Any)
        .next()
        .is_some()
}

/// Recursively collect every AUTHORED `.ttl` file under `dir` that declares at least one
/// `sh:NodeShape` — the legacy-shape scan universe. Generated projections (`generated/`),
/// build artifacts (`target/`), and hidden directories (`.git`, `.worktrees`, …) are never
/// authored surfaces and are excluded. A `dir` that is itself a single `.ttl` file scopes the
/// scan to exactly that file.
fn collect_legacy_shape_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.is_file() {
        if dir.extension().and_then(|e| e.to_str()) == Some("ttl") && declares_node_shape(dir) {
            out.push(dir.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name == "generated" || name == "target" || name.starts_with('.') {
                continue;
            }
            collect_legacy_shape_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl")
            && declares_node_shape(&path)
        {
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
        ShapeTarget::Sparql(select) if select == TARGETLESS_SELECT => "targetless".to_owned(),
        ShapeTarget::Sparql(select) => {
            // One line, bounded width: the raw select is a multi-line body.
            let flat: String = select.split_whitespace().collect::<Vec<_>>().join(" ");
            let compact: String = flat.chars().take(72).collect();
            let ellipsis = if flat.chars().count() > 72 { "…" } else { "" };
            format!("sparql {compact}{ellipsis}")
        }
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
    /// The parsed constraint/procedural projection surfaces themselves, retained so the
    /// semantic clearance path can extract a `logic:formalizes` record's shape subgraph and run
    /// it as a REAL SHACL validator in the witness cross-check.
    constraint_surfaces: Vec<std::sync::Arc<RdfDataset>>,
}

impl OracleCtx {
    /// Build the context from already-parsed surfaces: the projected validation-shape dataset
    /// and the constraint/procedural projection datasets. This is the SINGLE construction path —
    /// [`Self::load`] wraps it with file I/O and the tests drive it with in-memory datasets, so
    /// both exercise the same verdict machinery. `Err` carries the projected-surface read errors.
    fn from_surfaces(
        projected_ds: &RdfDataset,
        constraint_surfaces: Vec<std::sync::Arc<RdfDataset>>,
        object_properties: std::collections::BTreeSet<String>,
    ) -> Result<Self, Vec<String>> {
        let mut proj_errors = Vec::new();
        let projected = shapes_by_target(projected_ds, &mut proj_errors);
        if !proj_errors.is_empty() {
            return Err(proj_errors);
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

        // The projected FOL-constraint surface: which legacy shapes have a `logic:`-backed
        // constraint shape (so a legacy shape's `sh:sparql` cross-node residue is grounded).
        let mut formalized_shapes = std::collections::BTreeSet::new();
        let mut formalized_failure_classes: BTreeMap<String, std::collections::BTreeSet<String>> =
            BTreeMap::new();
        for ds in &constraint_surfaces {
            formalized_shapes.extend(formalized_shape_iris(ds));
            for (legacy, failures) in collect_formalized_failure_classes(ds) {
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
            object_properties,
            constraint_surfaces,
        })
    }

    /// Load the context from the committed generated surfaces under `root`, mapping any read/parse
    /// failure to the dev exit code.
    fn load(root: &Path, tool: &str) -> Result<Self, i32> {
        let projected_ds = parse_ttl_file(&root.join(PROJECTED_REL))?;
        let mut constraint_surfaces = Vec::new();
        for rel in [CONSTRAINT_REL, PROCEDURAL_REL] {
            constraint_surfaces.push(parse_ttl_file(&root.join(rel))?);
        }
        Self::from_surfaces(
            &projected_ds,
            constraint_surfaces,
            object_property_iris(root),
        )
        .map_err(|errors| {
            for e in &errors {
                eprintln!("{tool}: projected shape read error: {e}");
            }
            1
        })
    }

    /// The semantic witness agreement for one structurally-residue-bearing legacy shape: extract
    /// the shape's own subgraph and its exact `logic:formalizes` record subgraph(s), generate the
    /// witness plan FROM the legacy constructs, and require focus-flag agreement on both sides.
    /// `include_covered` widens the plan to the covered property fragment for a shape with NO
    /// declarative projected peer (a raw-SPARQL-target block), whose record must reproduce the
    /// covered enforcement too.
    fn semantic_agreement(
        &self,
        iri: &str,
        read: &ShapeRead,
        legacy_ds: &RdfDataset,
        include_covered: bool,
    ) -> gmeow_errors::Result<()> {
        let legacy_ttl = shape_subgraph_ttl(legacy_ds, &[iri.to_owned()], "l");
        let mut record_ttl = String::new();
        for (i, ds) in self.constraint_surfaces.iter().enumerate() {
            let Some(formalizes) = ds.term_id_by_value(&TermValue::iri(LOGIC_FORMALIZES)) else {
                continue;
            };
            let Some(legacy_id) = ds.term_id_by_value(&TermValue::iri(iri)) else {
                continue;
            };
            let roots: Vec<String> = ds
                .quads_for_pattern(None, Some(formalizes), Some(legacy_id), GraphMatch::Any)
                .filter_map(|q| match ds.resolve(q.s) {
                    TermRef::Iri(s) => Some(s.to_owned()),
                    _ => None,
                })
                .collect();
            if !roots.is_empty() {
                record_ttl.push_str(&shape_subgraph_ttl(ds, &roots, &format!("r{i}")));
            }
        }
        if record_ttl.is_empty() {
            return Err(crate::error::clearance(format!(
                "semantic clearance: no logic:formalizes <{iri}> record subject found on the \
                 projected constraint surfaces"
            )));
        }
        let plan = semantic_witness_plan(legacy_ds, iri, read).map_err(crate::error::clearance)?;
        let mut witnesses = plan.conforming;
        witnesses.extend(plan.residue);
        if include_covered {
            witnesses.extend(plan.covered);
        }
        semantic_cross_check(&legacy_ttl, &record_ttl, &witnesses).map_err(crate::error::clearance)
    }

    /// Whether an exact `logic:formalizes <iri>` record on a projected constraint surface
    /// REPLICATES a raw-SPARQL-target legacy block verbatim: the record's own raw `sh:target`
    /// select and its full `sh:sparql` select set are α-equivalent (variable renaming,
    /// whitespace, keyword case — nothing else) to the legacy block's. α-equivalent selects
    /// choose the same focus set and flag the same nodes over every graph, so this is a
    /// VERIFIED reproduction — strictly stronger evidence than the `sh:sparql` trust anchor,
    /// available where the witness synthesizer cannot model the target's join skeleton.
    fn record_replicates_raw_sparql_block(
        &self,
        iri: &str,
        target: &ShapeTarget,
        legacy_ds: &RdfDataset,
    ) -> bool {
        let ShapeTarget::Sparql(legacy_target_select) = target else {
            return false;
        };
        let legacy_selects = sparql_constraint_selects(legacy_ds, iri);
        if legacy_selects.is_empty() {
            return false; // nothing to replicate — not this clearance's shape
        }
        let legacy_key = raw_sparql_block_key(legacy_target_select, &legacy_selects);
        for ds in &self.constraint_surfaces {
            for (record, formalized) in gmeow_validate::shape_grounding::formalizes_records(ds) {
                if !formalized.contains(iri) {
                    continue;
                }
                let Some(record_target) = raw_target_select(ds, &record) else {
                    continue;
                };
                let record_selects = sparql_constraint_selects(ds, &record);
                if record_selects.is_empty() {
                    continue;
                }
                if raw_sparql_block_key(&record_target, &record_selects) == legacy_key {
                    return true;
                }
            }
        }
        false
    }

    /// The verdict for one legacy shape across ALL its focus selectors: a multi-target shape
    /// (SHACL unions the focus sets of multi-valued `sh:targetClass` / `sh:targetSubjectsOf`)
    /// applies the SAME constraint payload under every target, so the block-level judgment is
    /// the WORST per-target verdict — every target's obligation must be reproduced before the
    /// block is deletable.
    fn verdict_all(&self, iri: &str, read: &ShapeRead, legacy_ds: &RdfDataset) -> Verdict {
        let mut worst = self.verdict(iri, &read.ir.target, read, legacy_ds);
        for t in &read.extra_targets {
            let mut retargeted = read.clone();
            retargeted.ir.target = t.clone();
            let v = self.verdict(iri, t, &retargeted, legacy_ds);
            if verdict_rank(&v) > verdict_rank(&worst) {
                worst = v;
            }
        }
        worst
    }

    /// The equivalence verdict for one legacy shape `read` (identified by `iri`, focus `target`,
    /// authored in `legacy_ds`) against the projected surface — the single per-shape judgment
    /// shared by the report and the prune phase.
    fn verdict(
        &self,
        iri: &str,
        target: &ShapeTarget,
        read: &ShapeRead,
        legacy_ds: &RdfDataset,
    ) -> Verdict {
        // Strip a redundant `sh:nodeKind sh:IRI` on an `owl:ObjectProperty` path: in GMEOW's
        // IRI-named-individual convention an object-property value IS an IRI, so the node-kind is
        // definitionally satisfied — not an enforcement the projection must reproduce.
        let stripped = ShapeRead {
            ir: strip_redundant_iri_nodekind(&read.ir, &self.object_properties),
            unsupported: read.unsupported.clone(),
            extra_targets: read.extra_targets.clone(),
        };
        let read = &stripped;
        // A residue that is ONLY `sh:sparql` / `sh:or` constructs is grounded when an EXACT
        // `logic:formalizes <legacy-shape-IRI>` record projects it onto the canonical
        // constraint/procedural surface (`sh:or` joined `sh:sparql` here for the disjunctive
        // obligations — a value-branch `sh:or` a record replicates procedurally). This trust
        // anchor is UNCHANGED for the existing paths.
        // A value-keyed `sh:SPARQLTarget` (`?this a C ; P value`) contributes, beside its
        // procedural `sh:sparql` body, one residue token per additional `?this a C` type
        // constraint the value-keyed reader could not fold into the `(predicate, value)` key.
        // A formalizing record's own class guard reproduces that type refinement, so it rides
        // the same `sh:sparql` trust anchor (trusted, not re-verified — it is not a covered
        // declarative component).
        let value_keyed_type_refinement =
            |p: &str| p.starts_with("sh:SPARQLTarget additional type constraint");
        let sparql_only_residue_grounded = |unsupported: &[String]| {
            !unsupported.is_empty()
                && unsupported
                    .iter()
                    .all(|p| p == SH_SPARQL || p == SH_OR || value_keyed_type_refinement(p))
                && self.formalized_shapes.contains(iri)
        };
        // The SEMANTIC clearance for STRUCTURAL residue (`sh:node` / `sh:xone` / a raw
        // `sh:SPARQLTarget`): the record trust anchor alone is NOT enough, because these
        // constructs are machine-readable — so their semantics are VERIFIED, not trusted. The
        // exact `logic:formalizes` record must exist AND its projected constraint surface must
        // reproduce the construct's judgments under the witness cross-check (near-misses
        // generated FROM the legacy construct; focus-flag agreement required on both sides). A
        // record whose lowered constraint does not reproduce the residue semantics NEVER clears.
        let structural = |p: &str| p == SH_NODE || p == SH_XONE || p == RAW_SPARQL_TARGET_RESIDUE;
        let semantic_residue_grounded = |unsupported: &[String], include_covered: bool| {
            let eligible = !unsupported.is_empty()
                && unsupported
                    .iter()
                    .all(|p| p == SH_SPARQL || p == SH_OR || structural(p))
                && unsupported.iter().any(|p| structural(p))
                && self.formalized_shapes.contains(iri);
            if !eligible {
                return false;
            }
            match self.semantic_agreement(iri, read, legacy_ds, include_covered) {
                Ok(()) => true,
                Err(e) => {
                    // A record exists but did NOT reproduce the residue semantics: the shape
                    // stays ungrounded, and the refusal's detail must reach the operator — a
                    // silently-dropped clearance failure is undiagnosable at wave scale.
                    eprintln!("shape-oracle: semantic clearance for <{iri}> not granted: {e}");
                    false
                }
            }
        };
        let formalized_failure_matches = || match &read.ir.failure_class {
            None => true,
            Some(expected) => self
                .formalized_failure_classes
                .get(iri)
                .is_some_and(|actual| actual.len() == 1 && actual.contains(expected)),
        };
        // The record-anchored failure identity: the legacy shape's typed failure class is
        // preserved by an exact `logic:formalizes <legacy-shape-IRI>` record carrying EXACTLY
        // that class. A multi-shape class target aggregates to ONE projected declarative shape
        // carrying ONE failure class, so a finer legacy obligation's typed identity rides its
        // formalizing record instead of the (necessarily single) class annotation.
        let failure_preserved_by_record = || {
            read.ir.failure_class.is_some()
                && self.formalized_shapes.contains(iri)
                && formalized_failure_matches()
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
                None if matches!(target, ShapeTarget::Sparql(s) if s != TARGETLESS_SELECT)
                    && read.ir.properties.is_empty()
                    && read.ir.node_components.is_empty()
                    && !read.unsupported.is_empty()
                    && read
                        .unsupported
                        .iter()
                        .all(|p| p == SH_SPARQL || p == RAW_SPARQL_TARGET_RESIDUE)
                    && read
                        .unsupported
                        .iter()
                        .any(|p| p == RAW_SPARQL_TARGET_RESIDUE)
                    && self.formalized_shapes.contains(iri)
                    && formalized_failure_matches()
                    && self.record_replicates_raw_sparql_block(iri, target, legacy_ds) =>
                {
                    // A raw-SPARQL-target block whose ENTIRE enforcement is its raw target +
                    // `sh:sparql` constraints, replicated α-equivalently by its exact
                    // `logic:formalizes` record's projected twin: identity of the select
                    // bodies is a verified reproduction (same focus set, same findings over
                    // every graph), so the block is grounded without a witness plan — which
                    // cannot be synthesized for a join-shaped target skeleton anyway.
                    Verdict::EquivGroundedResidue
                }
                None if semantic_residue_grounded(&read.unsupported, true)
                    && formalized_failure_matches() =>
                {
                    // Structural residue with no declarative peer (a raw-SPARQL-target block, or
                    // sh:node/sh:xone on a target the projector carries no aggregate shape for):
                    // the record exists, the failure identity matches, AND the witness
                    // cross-check proved the record reproduces the construct's semantics — the
                    // covered property fragment included (`include_covered`), because no peer
                    // carries it.
                    Verdict::EquivGroundedResidueSemantic
                }
                None if matches!(target, ShapeTarget::Sparql(s) if s == TARGETLESS_SELECT)
                    && read.ir.properties.is_empty()
                    && read.ir.node_components.is_empty()
                    && read.ir.failure_class.is_none()
                    && read.unsupported.is_empty() =>
                {
                    // A truly targetless documentation-only marker block: SHACL gives it an
                    // EMPTY focus set, so it enforces nothing — trivially reproduced by the
                    // (empty) projection and deletable as-is.
                    Verdict::Equiv
                }
                None if matches!(target, ShapeTarget::Sparql(_)) => {
                    // A raw-SPARQL-target block is WHOLE-SHAPE residue by construction (its
                    // focus selection has no OWL/RDFS antecedent, so no declarative peer can
                    // exist): not yet grounded, but the honest verdict is its residue, not a
                    // missing peer.
                    Verdict::EquivResidue(read.unsupported.clone())
                }
                None if read.ir.properties.is_empty()
                    && read.ir.node_components.is_empty()
                    && read.unsupported.is_empty()
                    && self.formalized_shapes.contains(iri)
                    && formalized_failure_matches() =>
                {
                    // An enforcement-free class-target block whose obligation now rides its
                    // exact `logic:formalizes` record: the legacy block flags nothing over any
                    // graph (deleting it loses no enforcement — trivial subsumption), and the
                    // canonical record carries the class's intended obligation onto the
                    // projected constraint surface, so the block's identity is record-grounded.
                    Verdict::EquivGroundedResidue
                }
                None => Verdict::NoProjectedPeer,
            },
            Some((_, proj)) => {
                // A differing typed failure class is admissible ONLY through the record-anchored
                // identity: an exact `logic:formalizes` record carrying the legacy class. Such a
                // shape can never be plain `Equiv` — its deletion clearance rests on the record,
                // so it is decided by the grounded-residue path below.
                let record_backed = failure_preserved_by_record();
                let failure_via_record =
                    read.ir.failure_class != proj.ir.failure_class && record_backed;
                if read.ir.failure_class != proj.ir.failure_class && !failure_via_record {
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
                // Record-backed node-kind tightening: a legacy `sh:nodeKind sh:IRI` on a path
                // whose projected peer carries only the weaker `sh:BlankNodeOrIRI` (the
                // `owl:allValuesFrom owl:Thing` carrier) is credited ONLY when an exact
                // `logic:formalizes <legacy-shape-IRI>` record with the legacy failure class
                // exists — the record's projected procedural twin carries the IRI-only
                // tightening (the same trust anchor that grounds sh:sparql residue). Any credit
                // marks the shape record-grounded, never plain `Equiv`.
                let mut record_credit_used = false;
                if record_backed {
                    for pc in proj_ir.properties.iter_mut() {
                        let legacy_iri_nk = read.ir.properties.iter().any(|lp| {
                            lp.path == pc.path
                                && lp.components.iter().any(|c| {
                                    matches!(
                                        c,
                                        ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)
                                    )
                                })
                        });
                        let proj_bnode_or_iri = pc.components.iter().any(|c| {
                            matches!(
                                c,
                                ConstraintComponent::NodeKindShacl(ShaclNodeKind::BlankNodeOrIri)
                            )
                        });
                        let proj_iri = pc.components.iter().any(|c| {
                            matches!(c, ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri))
                        });
                        if legacy_iri_nk && proj_bnode_or_iri && !proj_iri {
                            pc.components
                                .push(ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri));
                            record_credit_used = true;
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
                if failure_via_record {
                    // Record-anchored clearance: the projected declarative surface must enforce
                    // at least everything the covered legacy fragment does (the Galois soundness
                    // direction — a stricter aggregate peer, whose extra bounds were proven by
                    // the sibling blocks that contributed them, is admissible), and any residue
                    // must be exactly the sh:sparql / sh:or constructs the exact record projects,
                    // or structural residue the record reproduced under the witness cross-check.
                    return if v.legacy_subsumed_by_projected
                        && (v.unsupported.is_empty()
                            || sparql_only_residue_grounded(&v.unsupported))
                    {
                        Verdict::EquivGroundedResidue
                    } else if v.legacy_subsumed_by_projected
                        && semantic_residue_grounded(&v.unsupported, false)
                    {
                        Verdict::EquivGroundedResidueSemantic
                    } else if !v.legacy_subsumed_by_projected {
                        Verdict::NotEquiv(format!(
                            "record-formalized shape's covered fragment is not subsumed by the \
                             projected peer: {}",
                            v.reason
                        ))
                    } else {
                        Verdict::EquivResidue(v.unsupported.clone())
                    };
                }
                if grounded && !v.residue_bearing {
                    if record_credit_used {
                        Verdict::EquivGroundedResidue
                    } else {
                        Verdict::Equiv
                    }
                } else if grounded
                    && sparql_only_residue_grounded(&v.unsupported)
                    && formalized_failure_matches()
                {
                    Verdict::EquivGroundedResidue
                } else if grounded
                    && semantic_residue_grounded(&v.unsupported, false)
                    && formalized_failure_matches()
                {
                    // Structural residue beside a projected declarative peer: the peer carries
                    // the covered fragment (proven by the oracle above), so the witness
                    // cross-check verifies the RESIDUE constructs only against the record.
                    Verdict::EquivGroundedResidueSemantic
                } else if grounded {
                    Verdict::EquivResidue(v.unsupported.clone())
                } else {
                    Verdict::NotEquiv(v.reason)
                }
            }
        }
    }
}

/// The severity order of a verdict, worst-highest, for the multi-target fold: a block clears
/// only on its WEAKEST target's judgment.
fn verdict_rank(v: &Verdict) -> u8 {
    match v {
        Verdict::Equiv => 0,
        Verdict::EquivGroundedResidue => 1,
        Verdict::EquivGroundedResidueSemantic => 2,
        Verdict::EquivResidue(_) => 3,
        Verdict::NoProjectedPeer => 4,
        Verdict::NotEquiv(_) => 5,
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
            let verdict = ctx.verdict_all(iri, read, &ds);
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
/// After the per-shape lift output it prints the block-classification + subsumption-lattice
/// pre-analysis over the same scope ([`lattice_report`]): the CLASSIFICATION census by target
/// mechanism, the CONTRADICTIONS between blocks over overlapping focus sets, the REDUNDANCIES
/// (`A ⊑ B` entailments under the authored `rdfs:subClassOf` hierarchy), and the HOIST
/// candidates (identical constraints on sibling classes). Report-only: nothing is written.
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
    let mut all_blocks: Vec<(String, ShapeRead)> = Vec::new();
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
                // The certify detail embeds enforcement keys whose fields are joined by
                // control-character separators (NUL / U+001F); a raw control byte makes
                // downstream text tooling (grep, pagers) treat the whole report stream as
                // binary, so render each as a plain space for the console.
                let detail: String = e
                    .to_string()
                    .chars()
                    .map(|c| {
                        if c.is_control() && c != '\n' && c != '\t' {
                            ' '
                        } else {
                            c
                        }
                    })
                    .collect();
                eprintln!(
                    "shape-lift: {iri}: the lifted proposal does not re-derive the shape: {detail}"
                );
                uncertified += 1;
            }
        }
        all_blocks.extend(legacy);
    }
    let hier = ClassHierarchy::load(&root);
    print!("{}", lattice_report(&all_blocks, &hier));
    println!(
        "shape-lift: proposed OWL for {total} legacy shape(s); {uncertified} failed to certify."
    );
    if had_error || uncertified > 0 { 1 } else { 0 }
}

// ─── Block classification + subsumption-lattice pre-analysis (the shape-lift report tail) ───

const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// The merged authored `rdfs:subClassOf` hierarchy: the DIRECT named-class superclass edges
/// read from the root ontology plus every slice `module.ttl` (the same authored surfaces the
/// sibling loaders [`class_owner_modules`] / [`object_property_iris`] parse), and its
/// transitive closure. Blank-node class expressions (restrictions) are never hierarchy edges.
struct ClassHierarchy {
    /// class IRI → its DIRECT named superclasses.
    direct: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// class IRI → ALL its named superclasses (transitive).
    closure: BTreeMap<String, std::collections::BTreeSet<String>>,
}

impl ClassHierarchy {
    /// Build the hierarchy from direct edges, saturating the transitive closure.
    fn from_direct(direct: BTreeMap<String, std::collections::BTreeSet<String>>) -> Self {
        let mut closure = direct.clone();
        loop {
            let mut grew = false;
            let subs: Vec<String> = closure.keys().cloned().collect();
            for sub in &subs {
                let supers: Vec<String> = closure[sub].iter().cloned().collect();
                let mut add = std::collections::BTreeSet::new();
                for sup in &supers {
                    if let Some(grands) = closure.get(sup) {
                        add.extend(
                            grands
                                .iter()
                                .filter(|g| !closure[sub].contains(*g))
                                .cloned(),
                        );
                    }
                }
                if !add.is_empty() {
                    closure.get_mut(sub).expect("key exists").extend(add);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        Self { direct, closure }
    }

    /// Load the hierarchy from the authored dataset: the root ontology and every slice
    /// `module.ttl` — the same file universe the other dev-shape loaders parse.
    fn load(root: &Path) -> Self {
        let mut modules = Vec::new();
        collect_module_files(&root.join("slices"), &mut modules);
        let ont = root.join("ontology/gmeow.ttl");
        if ont.is_file() {
            modules.push(ont);
        }
        let mut direct: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        for m in modules {
            let Ok(ds) = parse_ttl_file(&m) else { continue };
            let Some(sco) = ds.term_id_by_value(&TermValue::iri(RDFS_SUBCLASSOF)) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(sco), None, GraphMatch::Any) {
                if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                    direct.entry(s.to_owned()).or_default().insert(o.to_owned());
                }
            }
        }
        Self::from_direct(direct)
    }

    /// Whether every instance of class `sub` is an instance of class `sup`: the same class, or
    /// `sup` a (transitive) authored superclass of `sub`.
    fn class_covers(&self, sup: &str, sub: &str) -> bool {
        sup == sub || self.closure.get(sub).is_some_and(|s| s.contains(sup))
    }
}

/// The target-mechanism tag of one focus selector, for the CLASSIFICATION census. A
/// value-keyed target IS an `sh:target [ a sh:SPARQLTarget … ]` in the legacy surface, so it
/// tags `sparql-target`; the [`TARGETLESS_SELECT`] sentinel (a truly targetless block) tags
/// `no-target`.
fn target_mechanism(t: &ShapeTarget) -> &'static str {
    match t {
        ShapeTarget::Class(_) | ShapeTarget::DirectClass(_) => "targetClass",
        ShapeTarget::SubjectsOf(_) => "targetSubjectsOf",
        ShapeTarget::ObjectsOf(_) => "targetObjectsOf",
        ShapeTarget::Sparql(s) if s == TARGETLESS_SELECT => "no-target",
        ShapeTarget::ValueKeyed { .. } | ShapeTarget::Sparql(_) => "sparql-target",
    }
}

/// ALL of a block's focus selectors: the primary target plus the multi-target extras.
fn block_targets(read: &ShapeRead) -> Vec<ShapeTarget> {
    let mut ts = vec![read.ir.target.clone()];
    ts.extend(read.extra_targets.iter().cloned());
    ts
}

/// The empty-focus sentinel (a truly targetless documentation-only block).
fn is_targetless(t: &ShapeTarget) -> bool {
    matches!(t, ShapeTarget::Sparql(s) if s == TARGETLESS_SELECT)
}

/// A zero-enforcement block: its parsed IR carries no properties, no node-level components,
/// no typed failure class, and no residue — it flags nothing on any graph.
fn is_noop(read: &ShapeRead) -> bool {
    read.ir.properties.is_empty()
        && read.ir.node_components.is_empty()
        && read.ir.failure_class.is_none()
        && read.unsupported.is_empty()
}

/// Whether the focus sets of two selectors can share a node: the same non-targetless selector,
/// or two class targets related by the authored subclass hierarchy (a `Sub` instance is also a
/// `Super` instance, so the two shapes validate it jointly).
fn focus_overlaps(a: &ShapeTarget, b: &ShapeTarget, hier: &ClassHierarchy) -> bool {
    if is_targetless(a) || is_targetless(b) {
        return false;
    }
    if a == b {
        return true;
    }
    match (a, b) {
        (ShapeTarget::Class(x), ShapeTarget::Class(y)) => {
            hier.class_covers(x, y) || hier.class_covers(y, x)
        }
        _ => false,
    }
}

/// Whether selector `sup` selects AT LEAST every node `sub` selects: the same non-targetless
/// selector, or a class target that is a (transitive) superclass of `sub`'s class.
fn target_covers(sup: &ShapeTarget, sub: &ShapeTarget, hier: &ClassHierarchy) -> bool {
    if is_targetless(sup) || is_targetless(sub) {
        return false;
    }
    if sup == sub {
        return true;
    }
    match (sup, sub) {
        (ShapeTarget::Class(s), ShapeTarget::Class(c)) => hier.class_covers(s, c),
        _ => false,
    }
}

/// A compact single-line rendering of a [`ShapeValue`].
fn value_desc(v: &ShapeValue) -> String {
    match v {
        ShapeValue::Iri(i) => format!("<{i}>"),
        ShapeValue::Literal {
            lexical,
            datatype,
            lang,
        } => match (datatype, lang) {
            (Some(dt), _) => format!("\"{lexical}\"^^<{dt}>"),
            (None, Some(l)) => format!("\"{lexical}\"@{l}"),
            (None, None) => format!("\"{lexical}\""),
        },
    }
}

/// A compact single-line rendering of a constraint component (SHACL vocabulary where one
/// exists; the IR debug form for the exotic ADL/OPT components).
fn component_desc(c: &ConstraintComponent) -> String {
    match c {
        ConstraintComponent::Class(k) => format!("sh:class <{k}>"),
        ConstraintComponent::Datatype(d) => format!("sh:datatype <{d}>"),
        ConstraintComponent::NodeKindShacl(k) => format!("sh:nodeKind sh:{}", k.as_str()),
        ConstraintComponent::HasValue(v) => format!("sh:hasValue {}", value_desc(v)),
        ConstraintComponent::In(vs) => format!(
            "sh:in ({})",
            vs.iter().map(value_desc).collect::<Vec<_>>().join(" ")
        ),
        ConstraintComponent::Pattern { regex, flags } => match flags {
            Some(f) => format!("sh:pattern {regex:?} flags {f:?}"),
            None => format!("sh:pattern {regex:?}"),
        },
        ConstraintComponent::MinLength(n) => format!("sh:minLength {n}"),
        ConstraintComponent::MaxLength(n) => format!("sh:maxLength {n}"),
        other => format!("{other:?}"),
    }
}

/// A compact single-line rendering of one property constraint (path, cardinality, components).
fn property_desc(pc: &PropertyConstraintIr) -> String {
    let mut parts = vec![format!("sh:path <{}>", pc.path)];
    if pc.inverse {
        parts.push("inverse".to_owned());
    }
    if let Some(m) = pc.min_count {
        parts.push(format!("sh:minCount {m}"));
    }
    if let Some(m) = pc.max_count {
        parts.push(format!("sh:maxCount {m}"));
    }
    parts.extend(pc.components.iter().map(component_desc));
    parts.join(" ; ")
}

/// The enforcement key of ONE property constraint, via a fixed-target probe shape so
/// presentation/provenance never perturbs constraint identity (the hoist grouping key).
fn property_enforcement_key(pc: &PropertyConstraintIr) -> String {
    let probe = ValidationShapeIr::new(
        "https://blackcatinformatics.ca/gmeow/lattice-probe-shape",
        ShapeTarget::Class("https://blackcatinformatics.ca/gmeow/lattice-probe-class".to_owned()),
        vec![pc.clone()],
        None,
    )
    .expect("the single-property probe shape always builds");
    enforcement_key(&probe)
}

/// The `sh:hasValue` members of a property constraint.
fn has_values(pc: &PropertyConstraintIr) -> Vec<&ShapeValue> {
    pc.components
        .iter()
        .filter_map(|c| match c {
            ConstraintComponent::HasValue(v) => Some(v),
            _ => None,
        })
        .collect()
}

/// The `sh:in` value sets of a property constraint.
fn in_sets(pc: &PropertyConstraintIr) -> Vec<&Vec<ShapeValue>> {
    pc.components
        .iter()
        .filter_map(|c| match c {
            ConstraintComponent::In(vs) => Some(vs),
            _ => None,
        })
        .collect()
}

/// The `sh:datatype` IRIs of a property constraint.
fn datatypes(pc: &PropertyConstraintIr) -> Vec<&String> {
    pc.components
        .iter()
        .filter_map(|c| match c {
            ConstraintComponent::Datatype(d) => Some(d),
            _ => None,
        })
        .collect()
}

/// The joint-unsatisfiability findings between two SAME-PATH property constraints from two
/// blocks whose focus sets overlap: cross-pair `min > max` cardinality, an `sh:hasValue`
/// excluded by the other block's `sh:in` set, a required value under two disjoint `sh:in`
/// sets or two distinct `sh:datatype`s, and two distinct required values under a `maxCount 1`
/// cap. Empty when the pair is jointly satisfiable (as far as these decidable checks reach).
fn property_contradictions(pa: &PropertyConstraintIr, pb: &PropertyConstraintIr) -> Vec<String> {
    let mut out = Vec::new();
    if pa.path != pb.path || pa.inverse != pb.inverse {
        return out;
    }
    let p = &pa.path;
    // Cross-pair cardinality: one block's floor above the other's ceiling.
    for (lo, hi) in [(pa, pb), (pb, pa)] {
        if let (Some(min), Some(max)) = (lo.min_count, hi.max_count)
            && min > max
        {
            out.push(format!(
                "path <{p}>: sh:minCount {min} vs sh:maxCount {max} (min > max across the pair)"
            ));
        }
        // A required fixed value under the other block's zero ceiling.
        if !has_values(lo).is_empty() && hi.max_count == Some(0) {
            out.push(format!(
                "path <{p}>: sh:hasValue requires a value but the peer caps sh:maxCount 0"
            ));
        }
    }
    // A required fixed value excluded by the other block's closed value set.
    for (has_side, in_side) in [(pa, pb), (pb, pa)] {
        for v in has_values(has_side) {
            for set in in_sets(in_side) {
                if !set.contains(v) {
                    out.push(format!(
                        "path <{p}>: sh:hasValue {} excluded by sh:in ({})",
                        value_desc(v),
                        set.iter().map(value_desc).collect::<Vec<_>>().join(" ")
                    ));
                }
            }
        }
    }
    // Two distinct required values under a one-value ceiling.
    let cap = pa.max_count.into_iter().chain(pb.max_count).min();
    if cap.is_some_and(|m| m <= 1) {
        for x in has_values(pa) {
            for y in has_values(pb) {
                if x != y {
                    out.push(format!(
                        "path <{p}>: sh:hasValue {} vs sh:hasValue {} under sh:maxCount {} (two required values, at most one allowed)",
                        value_desc(x),
                        value_desc(y),
                        cap.expect("cap is Some inside is_some_and"),
                    ));
                }
            }
        }
    }
    // Checks below bite only when SOME value must exist on the path.
    let value_forced = pa.min_count.unwrap_or(0) >= 1
        || pb.min_count.unwrap_or(0) >= 1
        || !has_values(pa).is_empty()
        || !has_values(pb).is_empty();
    if value_forced {
        // Disjoint closed value sets: every value violates one side.
        for sa in in_sets(pa) {
            for sb in in_sets(pb) {
                if sa.iter().all(|v| !sb.contains(v)) {
                    out.push(format!(
                        "path <{p}>: disjoint sh:in sets ({}) vs ({}) with a required value",
                        sa.iter().map(value_desc).collect::<Vec<_>>().join(" "),
                        sb.iter().map(value_desc).collect::<Vec<_>>().join(" ")
                    ));
                }
            }
        }
        // Two distinct datatypes: no literal carries both.
        for da in datatypes(pa) {
            for db in datatypes(pb) {
                if da != db {
                    out.push(format!(
                        "path <{p}>: sh:datatype <{da}> vs sh:datatype <{db}> (no value can carry both)"
                    ));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The block-classification + subsumption-lattice pre-analysis over the scanned legacy blocks:
/// the CLASSIFICATION census (per-block target-mechanism tags + no-op flag, per-tag tally, and
/// the block TOTAL), the CONTRADICTIONS between blocks over overlapping focus sets, the
/// REDUNDANCIES (`A ⊑ B` covered-fragment entailments under the authored subclass hierarchy,
/// via [`subsumes`]), and the HOIST candidates (an identical covered constraint on ≥2 sibling
/// classes, grouped under their shared direct superclass — the LUB). Pure over its inputs so
/// the tests drive it with fixture blocks and a hand-built hierarchy; every section sorts by
/// IRI for stable output.
fn lattice_report(blocks: &[(String, ShapeRead)], hier: &ClassHierarchy) -> String {
    use std::fmt::Write as _;
    let mut sorted: Vec<(&String, &ShapeRead)> = blocks.iter().map(|(i, r)| (i, r)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::new();

    // ── CLASSIFICATION ──
    out.push_str("CLASSIFICATION\n");
    let mut tally: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (iri, read) in &sorted {
        let tags: std::collections::BTreeSet<&'static str> =
            block_targets(read).iter().map(target_mechanism).collect();
        for t in &tags {
            *tally.entry(t).or_insert(0) += 1;
        }
        let noop = is_noop(read);
        if noop {
            *tally.entry("no-op").or_insert(0) += 1;
        }
        writeln!(
            out,
            "  block <{iri}> tags={}{}",
            tags.iter().copied().collect::<Vec<_>>().join(","),
            if noop { " no-op" } else { "" }
        )
        .expect("String write is infallible");
    }
    for (tag, n) in &tally {
        writeln!(out, "  tally {tag}={n}").expect("String write is infallible");
    }
    writeln!(out, "  TOTAL blocks={}", sorted.len()).expect("String write is infallible");

    // ── CONTRADICTIONS ──
    out.push_str("CONTRADICTIONS\n");
    let mut lines = Vec::new();
    for (i, (ia, ra)) in sorted.iter().enumerate() {
        for (ib, rb) in sorted.iter().skip(i + 1) {
            let ta = block_targets(ra);
            let tb = block_targets(rb);
            if !ta
                .iter()
                .any(|x| tb.iter().any(|y| focus_overlaps(x, y, hier)))
            {
                continue;
            }
            let mut details = Vec::new();
            for pa in &ra.ir.properties {
                for pb in &rb.ir.properties {
                    details.extend(property_contradictions(pa, pb));
                }
            }
            details.sort();
            details.dedup();
            for d in details {
                lines.push(format!("  <{ia}> ⊗ <{ib}> — {d}\n"));
            }
        }
    }
    if lines.is_empty() {
        out.push_str("  none\n");
    } else {
        lines.iter().for_each(|l| out.push_str(l));
    }

    // ── REDUNDANCIES ──
    out.push_str("REDUNDANCIES\n");
    let mut lines = Vec::new();
    for (ia, ra) in &sorted {
        // A block with an empty covered fragment entails nothing — listing it as "redundant"
        // would be vacuous (the no-op census already names it).
        if ra.ir.properties.is_empty() && ra.ir.node_components.is_empty() {
            continue;
        }
        let a_targets = block_targets(ra);
        // A block whose every focus set is empty enforces nothing.
        if a_targets.iter().all(is_targetless) {
            continue;
        }
        for (ib, rb) in &sorted {
            if ia == ib {
                continue;
            }
            let b_targets = block_targets(rb);
            // B must select every node A selects: each of A's (non-empty) focus selectors is
            // covered by some selector of B.
            let covered = a_targets.iter().all(|ta| {
                is_targetless(ta) || b_targets.iter().any(|tb| target_covers(tb, ta, hier))
            });
            if !covered {
                continue;
            }
            // Payload entailment on the shared focus: retarget both covered IRs to one probe
            // selector so [`subsumes`] compares exactly the constraint payloads.
            let probe =
                ShapeTarget::Class("https://blackcatinformatics.ca/gmeow/lattice-probe".to_owned());
            let mut weak = ra.ir.clone();
            weak.target = probe.clone();
            let mut strong = rb.ir.clone();
            strong.target = probe;
            if subsumes(&strong, &weak) {
                let residue_note = if ra.unsupported.is_empty() {
                    ""
                } else {
                    " (covered fragment only — the subsumed block carries residue)"
                };
                lines.push(format!("  <{ia}> ⊑ <{ib}>{residue_note}\n"));
            }
        }
    }
    if lines.is_empty() {
        out.push_str("  none\n");
    } else {
        lines.iter().for_each(|l| out.push_str(l));
    }

    // ── HOISTS ──
    out.push_str("HOISTS\n");
    // (direct superclass = LUB, constraint enforcement key) → (rendering, sibling classes).
    let mut hoists: BTreeMap<(String, String), (String, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    for (_, read) in &sorted {
        for t in block_targets(read) {
            let ShapeTarget::Class(c) = t else { continue };
            let Some(parents) = hier.direct.get(&c) else {
                continue;
            };
            for pc in &read.ir.properties {
                let key = property_enforcement_key(pc);
                let desc = property_desc(pc);
                for parent in parents {
                    hoists
                        .entry((parent.clone(), key.clone()))
                        .or_insert_with(|| (desc.clone(), std::collections::BTreeSet::new()))
                        .1
                        .insert(c.clone());
                }
            }
        }
    }
    let mut lines = Vec::new();
    for ((parent, _), (desc, classes)) in &hoists {
        if classes.len() >= 2 {
            lines.push(format!(
                "  lub=<{parent}> constraint=[{desc}] siblings={}\n",
                classes
                    .iter()
                    .map(|c| format!("<{c}>"))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
    }
    lines.sort();
    if lines.is_empty() {
        out.push_str("  none\n");
    } else {
        lines.iter().for_each(|l| out.push_str(l));
    }
    out
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

/// Splice the given `[start, end)` statement spans out of `text`, longest-offset-first (so
/// earlier spans stay valid), consuming each block's trailing newline run up to one blank line
/// and collapsing the run of blank lines the block leaves behind.
fn splice_out_spans(text: &mut String, mut spans: Vec<(usize, usize)>) {
    spans.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (mut s, mut e) in spans {
        // Consume the trailing newline and one following blank line for tidy output.
        while e < text.len() && (text.as_bytes()[e] == b'\n' || text.as_bytes()[e] == b'\r') {
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
            if ctx.verdict_all(iri, read, &ds).is_grounded() {
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
            let v = ctx.verdict_all(iri, read, &ds);
            if v.is_grounded() {
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
            let spans: Vec<(usize, usize)> = to_delete
                .iter()
                .filter_map(|iri| subject_span(&text, local_name(iri)))
                .collect();
            splice_out_spans(&mut text, spans);
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

    /// The record-anchored clearance: a finer legacy shape whose typed failure class differs
    /// from the (single) class-annotation failure of the aggregate projected peer is grounded
    /// when an EXACT `logic:formalizes <legacy-shape-IRI>` record carries the legacy class and
    /// the projected peer subsumes the covered fragment.
    #[test]
    fn record_anchored_failure_identity_grounds_a_finer_legacy_shape() {
        use gmeow_logic_compile::ir::ShaclNodeKind;
        let target = ShapeTarget::Class("https://example.test/Widget".to_owned());
        // Projected aggregate peer: nodeKind + minCount on the path, class-level failure class.
        let proj_ir = ValidationShapeIr::new(
            "https://example.test/Widget-shape",
            target.clone(),
            vec![
                PropertyConstraintIr::new(
                    "https://example.test/law",
                    Some(1),
                    None,
                    Some(ConstraintProvenance::OwlRestriction),
                    vec![ConstraintComponent::NodeKindShacl(
                        ShaclNodeKind::BlankNodeOrIri,
                    )],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap()
        .with_failure_class("https://example.test/CoarseFailure")
        .unwrap();
        // Legacy finer shape: only the nodeKind (no minCount), a FINER failure class.
        let legacy_ir = ValidationShapeIr::new(
            "https://example.test/LawShape",
            target.clone(),
            vec![
                PropertyConstraintIr::new(
                    "https://example.test/law",
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::NodeKindShacl(
                        ShaclNodeKind::BlankNodeOrIri,
                    )],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap()
        .with_failure_class("https://example.test/FineFailure")
        .unwrap();
        let read = ShapeRead {
            ir: legacy_ir,
            unsupported: vec![],
            extra_targets: vec![],
        };
        let mut projected = BTreeMap::new();
        projected.insert(
            target.clone(),
            (
                "https://example.test/Widget-shape".to_owned(),
                ShapeRead {
                    ir: proj_ir,
                    unsupported: vec![],
                    extra_targets: vec![],
                },
            ),
        );
        let mut ctx = OracleCtx {
            projected,
            functional_max: BTreeMap::new(),
            formalized_shapes: std::collections::BTreeSet::new(),
            formalized_failure_classes: BTreeMap::new(),
            object_properties: std::collections::BTreeSet::new(),
            constraint_surfaces: Vec::new(),
        };
        let ds = parse_dataset(b"", "text/turtle", None).expect("empty dataset parses");
        // WITHOUT the record: the differing failure class blocks deletion.
        let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
        assert!(
            matches!(v, Verdict::NotEquiv(ref r) if r.contains("failure class")),
            "{}",
            v.label()
        );
        // WITH the exact formalizes record carrying the legacy class: grounded.
        ctx.formalized_shapes
            .insert("https://example.test/LawShape".to_owned());
        ctx.formalized_failure_classes.insert(
            "https://example.test/LawShape".to_owned(),
            std::iter::once("https://example.test/FineFailure".to_owned()).collect(),
        );
        let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
        assert!(matches!(v, Verdict::EquivGroundedResidue), "{}", v.label());
        // A record carrying the WRONG failure class never clears the block.
        ctx.formalized_failure_classes.insert(
            "https://example.test/LawShape".to_owned(),
            std::iter::once("https://example.test/OtherFailure".to_owned()).collect(),
        );
        let v = ctx.verdict("https://example.test/LawShape", &target, &read, &ds);
        assert!(
            matches!(v, Verdict::NotEquiv(_)),
            "a wrong-class record must not clear deletion: {}",
            v.label()
        );
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

    /// The workspace root (this crate's manifest sits two levels below it).
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn parse_ttl(ttl: &str) -> std::sync::Arc<RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("test turtle must parse")
    }

    /// Build the oracle context through the SAME construction path the CLI uses
    /// ([`OracleCtx::from_surfaces`]): a projected validation surface plus the
    /// constraint/procedural record surfaces, all as parsed Turtle.
    fn ctx_from(projected_ttl: &str, surfaces: &[&str]) -> OracleCtx {
        let projected = parse_ttl(projected_ttl);
        let surfaces = surfaces.iter().map(|s| parse_ttl(s)).collect();
        OracleCtx::from_surfaces(&projected, surfaces, std::collections::BTreeSet::new())
            .expect("test surfaces must index cleanly")
    }

    // ── Scanner ────────────────────────────────────────────────────────────────

    #[test]
    fn scanner_discovers_root_shape_files_and_skips_generated() {
        // The real repo: the root shapes/ directory is in the universe now.
        let mut files = Vec::new();
        collect_legacy_shape_files(&repo_root().join("shapes"), &mut files);
        assert!(
            files.iter().any(|p| p.ends_with("gmeow-shapes.ttl")),
            "{files:?}"
        );

        // A synthetic tree: an authored declarer is found; a generated/ declarer and a
        // non-declaring .ttl are not.
        let tmp = std::env::temp_dir().join(format!("gmeow-shape-scan-{}", std::process::id()));
        let declarer = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/S> a sh:NodeShape ; sh:targetClass <https://ex/C> .\n";
        std::fs::create_dir_all(tmp.join("generated")).expect("mkdir");
        std::fs::write(tmp.join("a.ttl"), declarer).expect("write");
        std::fs::write(tmp.join("generated/b.ttl"), declarer).expect("write");
        std::fs::write(
            tmp.join("c.ttl"),
            "# NodeShape mentioned in a comment only\n<https://ex/x> <https://ex/p> <https://ex/y> .\n",
        )
        .expect("write");
        let mut found = Vec::new();
        collect_legacy_shape_files(&tmp, &mut found);
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(found, vec![tmp.join("a.ttl")], "{found:?}");
    }

    #[test]
    fn scanner_universe_over_slices_is_unchanged() {
        // The generalized rule must reproduce the previous name-based universe over slices/ up
        // to the files that CONTRIBUTE shapes: every per-slice shapes.ttl that declares at least
        // one sh:NodeShape, and NOTHING else (module.ttl files mention NodeShape only in prose,
        // never as a declaration; a fully-migrated tombstone shapes.ttl declares none and
        // contributed ZERO shapes to the old scan's output too).
        let slices = repo_root().join("slices");
        let mut new_scan = Vec::new();
        collect_legacy_shape_files(&slices, &mut new_scan);
        fn old_rule(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            let mut entries: Vec<PathBuf> =
                entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    old_rule(&path, out);
                } else if path.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl") {
                    out.push(path);
                }
            }
        }
        let mut named = Vec::new();
        old_rule(&slices, &mut named);
        assert!(!new_scan.is_empty());
        // Every scanned file is a per-slice shapes.ttl (nothing NEW joined the universe) …
        for f in &new_scan {
            assert!(
                named.contains(f),
                "a non-shapes.ttl slice file joined the universe: {}",
                f.display()
            );
        }
        // … and the only named files missing from the scan are declaration-free tombstones.
        for f in &named {
            if !new_scan.contains(f) {
                assert!(
                    !declares_node_shape(f),
                    "{} declares sh:NodeShape but was not scanned",
                    f.display()
                );
            }
        }
    }

    // ── Semantic clearance matrix: sh:xone beside a projected declarative peer ──

    const XONE_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/ParamShape> a sh:NodeShape ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] ;\n\
        \x20\x20sh:xone (\n\
        \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/value> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:Literal ] ]\n\
        \x20\x20\x20\x20[ sh:property [ sh:path <https://ex/entity> ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ] ]\n\
        \x20\x20) .\n";

    /// The projected declarative peer reproducing the covered fragment.
    const XONE_PEER_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/Param-shape> a sh:NodeShape ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:property [ sh:path <https://ex/name> ; sh:minCount 1 ; sh:maxCount 1 ] .\n";

    /// The faithful record: flags a focus with NEITHER alternative and a focus with BOTH.
    const XONE_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"exactly one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            { FILTER NOT EXISTS { $this <https://ex/value> ?v } FILTER NOT EXISTS { $this <https://ex/entity> ?e } }\n\
            UNION\n\
            { $this <https://ex/value> ?v2 . $this <https://ex/entity> ?e2 . }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

    /// The WRONG-SEMANTICS record: an at-least-one lowering of the exactly-one obligation.
    const XONE_OR_LOWERED_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/ParamXoneConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/ParamShape> ;\n\
        \x20\x20sh:targetClass <https://ex/Param> ;\n\
        \x20\x20sh:sparql [\n\
        \x20\x20\x20\x20a sh:SPARQLConstraint ;\n\
        \x20\x20\x20\x20sh:message \"at least one of value/entity\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE {\n\
            FILTER NOT EXISTS { $this <https://ex/value> ?v }\n\
            FILTER NOT EXISTS { $this <https://ex/entity> ?e }\n\
        }\"\"\" ;\n\
        \x20\x20] .\n";

    #[test]
    fn xone_clearance_matrix() {
        let ds = parse_ttl(XONE_LEGACY_TTL);
        let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("legacy reads");
        assert!(
            read.unsupported.iter().any(|u| u == SH_XONE),
            "{:?}",
            read.unsupported
        );

        // Correct grounding: record + failure identity + witness agreement → cleared.
        let ctx = ctx_from(XONE_PEER_TTL, &[XONE_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidueSemantic),
            "the faithful record must clear: {}",
            v.label()
        );

        // Missing record → the residue stays ungrounded.
        let ctx = ctx_from(XONE_PEER_TTL, &[]);
        let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "no record must not clear: {}",
            v.label()
        );

        // Wrong-semantics record (an or-lowering): the witness cross-check MUST deny clearance.
        let ctx = ctx_from(XONE_PEER_TTL, &[XONE_OR_LOWERED_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "an or-lowered record must NOT clear: {}",
            v.label()
        );
    }

    #[test]
    fn xone_clearance_rejects_a_mismatched_failure_class() {
        // The same fixture pair with a typed failure class on the legacy shape and its peer;
        // the record carries a DIFFERENT class, so the failure identity blocks clearance.
        let failure = "gmeow:enforcesFailureClass";
        let legacy = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
            XONE_LEGACY_TTL.replace(
                "sh:targetClass <https://ex/Param> ;",
                &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
            )
        );
        let peer = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
            XONE_PEER_TTL.replace(
                "sh:targetClass <https://ex/Param> ;",
                &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
            )
        );
        let wrong_class_record = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
            XONE_RECORD_TTL.replace(
                "sh:targetClass <https://ex/Param> ;",
                &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/G> ;")
            )
        );
        let right_class_record = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{}",
            XONE_RECORD_TTL.replace(
                "sh:targetClass <https://ex/Param> ;",
                &format!("sh:targetClass <https://ex/Param> ;\n  {failure} <https://ex/F> ;")
            )
        );
        let ds = parse_ttl(&legacy);
        let read = read_shacl_shape(&ds, "https://ex/ParamShape").expect("legacy reads");

        let ctx = ctx_from(&peer, &[&right_class_record]);
        let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidueSemantic),
            "the matching failure class clears: {}",
            v.label()
        );

        let ctx = ctx_from(&peer, &[&wrong_class_record]);
        let v = ctx.verdict_all("https://ex/ParamShape", &read, &ds);
        assert!(
            !v.is_grounded(),
            "a mismatched failure class must NOT clear: {}",
            v.label()
        );
    }

    // ── Semantic clearance matrix: a raw-SPARQL-target block (meta-shape style) ──

    const META_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        <https://ex/MetaShape> a sh:NodeShape ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
        \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n";

    /// The faithful record: the SAME focus selection plus the SAME structural constraints,
    /// carried on the projected constraint surface with the exact formalizes back-reference.
    const META_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/MetaConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/MetaShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] ;\n\
        \x20\x20sh:property [ sh:path <https://ex/role> ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .\n";

    /// The WRONG-SEMANTICS record: it silently drops the role obligation.
    const META_WEAK_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/MetaConstraint> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/MetaShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?this a <http://www.w3.org/2002/07/owl#Class> .\n\
                FILTER(STRSTARTS(STR(?this), \"https://example.test/ns/\"))\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:property [ sh:path rdfs:label ; sh:minCount 1 ] .\n";

    #[test]
    fn sparql_target_clearance_matrix() {
        let ds = parse_ttl(META_LEGACY_TTL);
        let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("legacy reads");
        assert!(
            matches!(read.ir.target, ShapeTarget::Sparql(_)),
            "{:?}",
            read.ir.target
        );
        assert!(
            read.unsupported
                .iter()
                .any(|u| u == RAW_SPARQL_TARGET_RESIDUE),
            "{:?}",
            read.unsupported
        );

        // Correct grounding: the record reproduces the structural constraints on the same focus
        // selection → cleared (the covered witnesses are part of the plan: no declarative peer
        // exists for a raw SPARQL target).
        let ctx = ctx_from("", &[META_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidueSemantic),
            "the faithful record must clear: {}",
            v.label()
        );

        // Missing record → whole-shape residue, not cleared.
        let ctx = ctx_from("", &[]);
        let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "no record must not clear: {}",
            v.label()
        );

        // Wrong-semantics record (drops an obligation) → the structural witness cross-check
        // MUST deny clearance.
        let ctx = ctx_from("", &[META_WEAK_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "a record that drops an obligation must NOT clear: {}",
            v.label()
        );
    }

    // ── Raw-SPARQL-target identity clearance (join-shaped target skeleton) ─────

    /// A legacy raw-target block whose focus selection is a variable-class JOIN (outside the
    /// witness synthesizer's skeleton) and whose ONLY enforcement is one sh:sparql constraint
    /// carrying an anonymous `[]` node.
    const JOIN_LEGACY_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        <https://ex/OpenValueShape> a sh:NodeShape ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"\n\
            SELECT ?this WHERE {\n\
                ?profile <https://ex/openValue> ?openClass .\n\
                ?this a ?openClass .\n\
            }\n\
        \"\"\" ] ;\n\
        \x20\x20sh:severity sh:Warning ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:message \"m\" ; sh:select \"\"\"\n\
            SELECT $this WHERE {\n\
                ?profile <https://ex/openValue> ?openClass .\n\
                $this a ?openClass .\n\
                FILTER NOT EXISTS {\n\
                    ?profile <https://ex/descriptor> ?descriptor .\n\
                    [] ?descriptor $this .\n\
                }\n\
            }\n\
        \"\"\" ] .\n";

    /// The faithful projected record: the SAME target select and constraint body up to
    /// whitespace, variable names (`?p2`), and the `[]` node written as an explicit variable.
    const JOIN_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/OpenValueConstraintShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/OpenValueShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?p2 <https://ex/openValue> ?oc . ?this a ?oc . }\"\"\" ] ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ; sh:message \"m\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { ?p2 <https://ex/openValue> ?oc . $this a ?oc . FILTER NOT EXISTS { ?p2 <https://ex/descriptor> ?d . ?subj ?d $this . } }\"\"\" ] .\n";

    /// A DIVERGENT record: same target, but the constraint body checks a different predicate.
    const JOIN_WRONG_RECORD_TTL: &str = "\
        @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        <https://ex/OpenValueConstraintShape> a sh:NodeShape ;\n\
        \x20\x20logic:formalizes <https://ex/OpenValueShape> ;\n\
        \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?p2 <https://ex/openValue> ?oc . ?this a ?oc . }\"\"\" ] ;\n\
        \x20\x20sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ; sh:message \"m\" ;\n\
        \x20\x20\x20\x20sh:select \"\"\"SELECT $this WHERE { ?p2 <https://ex/OTHER> ?oc . $this a ?oc . }\"\"\" ] .\n";

    #[test]
    fn raw_sparql_target_identity_clearance_matrix() {
        let ds = parse_ttl(JOIN_LEGACY_TTL);
        let read = read_shacl_shape(&ds, "https://ex/OpenValueShape").expect("legacy reads");
        assert!(
            matches!(read.ir.target, ShapeTarget::Sparql(_)),
            "{:?}",
            read.ir.target
        );
        assert!(read.ir.properties.is_empty(), "{:?}", read.ir.properties);

        // The α-equivalent record clears the block: identical selects (modulo variable
        // renaming, `[]`, whitespace, keyword case) are a verified reproduction.
        let ctx = ctx_from("", &[JOIN_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidue),
            "the α-equivalent record must clear: {}",
            v.label()
        );

        // No record → whole-shape residue.
        let ctx = ctx_from("", &[]);
        let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "no record must not clear: {}",
            v.label()
        );

        // A record whose constraint body diverges must NOT clear (and the witness path cannot
        // synthesize a plan for the join-shaped target, so the block stays residue).
        let ctx = ctx_from("", &[JOIN_WRONG_RECORD_TTL]);
        let v = ctx.verdict_all("https://ex/OpenValueShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivResidue(_)),
            "a divergent record must NOT clear: {}",
            v.label()
        );
    }

    #[test]
    fn sparql_alpha_canonical_equates_renamings_and_distinguishes_predicates() {
        let a = "SELECT $this WHERE { ?x <https://ex/p> ?y . [] ?y $this . }";
        let b = "SELECT ?this WHERE {\n  ?profile <https://ex/p> ?d .\n  ?subj ?d ?this . }";
        assert_eq!(sparql_alpha_canonical(a), sparql_alpha_canonical(b));
        let c = "SELECT $this WHERE { ?x <https://ex/OTHER> ?y . [] ?y $this . }";
        assert_ne!(sparql_alpha_canonical(a), sparql_alpha_canonical(c));
        // Two `[]` occurrences are DISTINCT fresh variables, never unified.
        let d = "SELECT ?this WHERE { [] <https://ex/p> ?this . [] <https://ex/q> ?this . }";
        let e = "SELECT ?this WHERE { ?s <https://ex/p> ?this . ?s <https://ex/q> ?this . }";
        assert_ne!(sparql_alpha_canonical(d), sparql_alpha_canonical(e));
    }

    #[test]
    fn sparql_target_clearance_rejects_a_mismatched_failure_class() {
        let legacy = META_LEGACY_TTL.replace(
            "<https://ex/MetaShape> a sh:NodeShape ;",
            "<https://ex/MetaShape> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/F> ;",
        );
        let wrong_record = META_RECORD_TTL.replace(
            "<https://ex/MetaConstraint> a sh:NodeShape ;",
            "<https://ex/MetaConstraint> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/G> ;",
        );
        let right_record = META_RECORD_TTL.replace(
            "<https://ex/MetaConstraint> a sh:NodeShape ;",
            "<https://ex/MetaConstraint> a sh:NodeShape ;\n  \
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> <https://ex/F> ;",
        );
        let ds = parse_ttl(&legacy);
        let read = read_shacl_shape(&ds, "https://ex/MetaShape").expect("legacy reads");

        let ctx = ctx_from("", &[&right_record]);
        let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidueSemantic),
            "the matching failure class clears: {}",
            v.label()
        );

        let ctx = ctx_from("", &[&wrong_record]);
        let v = ctx.verdict_all("https://ex/MetaShape", &read, &ds);
        assert!(
            !v.is_grounded(),
            "a mismatched failure class must NOT clear: {}",
            v.label()
        );
    }

    // ── Targetless documentation marker ────────────────────────────────────────

    #[test]
    fn targetless_doc_marker_reads_and_verdicts_equiv() {
        let ttl = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/DocMarker> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"doc-only marker\" ;\n\
             \x20\x20rdfs:comment \"asserts and enforces nothing\" .\n";
        let ds = parse_ttl(ttl);
        let read = read_shacl_shape(&ds, "https://ex/DocMarker").expect("targetless doc reads");
        let ctx = ctx_from("", &[]);
        let v = ctx.verdict_all("https://ex/DocMarker", &read, &ds);
        assert!(
            matches!(v, Verdict::Equiv),
            "a no-op doc marker enforces nothing and is trivially grounded: {}",
            v.label()
        );
    }

    #[test]
    fn enforcement_free_class_block_clears_only_via_its_exact_record() {
        // A class-target block with ZERO enforcement components: deleting it loses nothing,
        // but its identity clears only once its intended obligation rides an exact
        // `logic:formalizes` record on the projected constraint surface.
        let ttl = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/NoOpShape> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20rdfs:comment \"documentation only\" .\n";
        let record = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/CConstraint> a sh:NodeShape ;\n\
             \x20\x20<https://blackcatinformatics.ca/logic/formalizes> <https://ex/NoOpShape> .\n";
        let ds = parse_ttl(ttl);
        let read = read_shacl_shape(&ds, "https://ex/NoOpShape").expect("no-op class block reads");

        // Without a record the honest verdict stays NO-PROJECTED-PEER.
        let ctx = ctx_from("", &[]);
        let v = ctx.verdict_all("https://ex/NoOpShape", &read, &ds);
        assert!(
            matches!(v, Verdict::NoProjectedPeer),
            "no record must not clear an enforcement-free class block: {}",
            v.label()
        );

        // The exact record grounds the block's identity.
        let ctx = ctx_from("", &[record]);
        let v = ctx.verdict_all("https://ex/NoOpShape", &read, &ds);
        assert!(
            matches!(v, Verdict::EquivGroundedResidue),
            "the exact formalizes record must clear: {}",
            v.label()
        );
    }

    // ── Prune-splicer proof against the REAL shapes/gmeow-shapes.ttl ──────────

    /// The real repo-wide shapes file plus its parsed `sh:NodeShape` IRIs.
    fn real_gmeow_shapes() -> (String, Vec<String>) {
        let path = repo_root().join("shapes/gmeow-shapes.ttl");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let ds = parse_dataset(text.as_bytes(), "text/turtle", None)
            .expect("the committed shapes file must parse");
        let iris = node_shape_iris(&ds);
        (text, iris)
    }

    #[test]
    fn splicer_resolves_every_real_block_subject_count_exact() {
        let (text, iris) = real_gmeow_shapes();
        assert_eq!(iris.len(), 85, "the census of the committed file");
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for iri in &iris {
            let span = subject_span(&text, local_name(iri))
                .unwrap_or_else(|| panic!("subject_span failed for {iri}"));
            spans.push(span);
        }
        spans.sort_unstable();
        spans.dedup();
        assert_eq!(spans.len(), 85, "every block resolves to its OWN span");
        for w in spans.windows(2) {
            assert!(
                w[0].1 <= w[1].0,
                "block spans must never overlap: {:?} vs {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn splicing_every_real_block_leaves_valid_turtle_with_zero_shapes() {
        let (text, iris) = real_gmeow_shapes();
        let mut pruned = text.clone();
        let spans: Vec<(usize, usize)> = iris
            .iter()
            .map(|iri| subject_span(&pruned, local_name(iri)).expect("span resolves"))
            .collect();
        splice_out_spans(&mut pruned, spans);
        let ds = parse_dataset(pruned.as_bytes(), "text/turtle", None)
            .expect("the fully-pruned file must stay valid Turtle");
        assert!(
            node_shape_iris(&ds).is_empty(),
            "every block must be gone after the full prune"
        );
    }

    #[test]
    fn splicing_each_real_block_individually_round_trips() {
        let (text, iris) = real_gmeow_shapes();
        for iri in &iris {
            let mut copy = text.clone();
            let span = subject_span(&copy, local_name(iri)).expect("span resolves");
            splice_out_spans(&mut copy, vec![span]);
            let ds = parse_dataset(copy.as_bytes(), "text/turtle", None)
                .unwrap_or_else(|e| panic!("pruning {iri} broke the Turtle: {e}"));
            let remaining = node_shape_iris(&ds);
            assert_eq!(
                remaining.len(),
                84,
                "pruning {iri} must remove exactly one block"
            );
            assert!(!remaining.contains(iri), "{iri} must be the removed block");
        }
    }

    // ── Block classification + subsumption-lattice pre-analysis ────────────────

    /// Fixture blocks read through the real comparison-only reader.
    fn lattice_blocks(ttl: &str) -> Vec<(String, ShapeRead)> {
        let ds = parse_ttl(ttl);
        let mut errors = Vec::new();
        let blocks = read_shapes(&ds, &mut errors);
        assert!(errors.is_empty(), "fixture blocks must read: {errors:?}");
        blocks
    }

    /// A hierarchy from literal direct edges.
    fn lattice_hier(edges: &[(&str, &str)]) -> ClassHierarchy {
        let mut direct: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        for (sub, sup) in edges {
            direct
                .entry((*sub).to_owned())
                .or_default()
                .insert((*sup).to_owned());
        }
        ClassHierarchy::from_direct(direct)
    }

    #[test]
    fn classification_partitions_by_block_with_all_tags() {
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://ex/ClassBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/SubjectsBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetSubjectsOf <https://ex/p> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:maxCount 1 ] .\n\
             <https://ex/SparqlBlock> a sh:NodeShape ;\n\
             \x20\x20sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE { ?this <https://ex/q> ?v . }\"\"\" ] ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/MultiBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:targetSubjectsOf <https://ex/q> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/NoOpBlock> a sh:NodeShape ;\n\
             \x20\x20rdfs:label \"documentation-only marker\" .\n",
        );
        assert_eq!(blocks.len(), 5, "the census is by-block");
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(report.contains("  block <https://ex/ClassBlock> tags=targetClass\n"));
        assert!(report.contains("  block <https://ex/SubjectsBlock> tags=targetSubjectsOf\n"));
        assert!(
            report.contains("  block <https://ex/SparqlBlock> tags=sparql-target\n"),
            "{report}"
        );
        assert!(
            report.contains("  block <https://ex/MultiBlock> tags=targetClass,targetSubjectsOf\n"),
            "a multi-target block lists ALL its tags: {report}"
        );
        assert!(
            report.contains("  block <https://ex/NoOpBlock> tags=no-target no-op\n"),
            "a zero-enforcement block carries the no-op flag: {report}"
        );
        assert!(report.contains("  tally targetClass=2\n"), "{report}");
        assert!(report.contains("  tally targetSubjectsOf=2\n"), "{report}");
        assert!(report.contains("  tally sparql-target=1\n"), "{report}");
        assert!(report.contains("  tally no-target=1\n"), "{report}");
        assert!(report.contains("  tally no-op=1\n"), "{report}");
        assert!(report.contains("  TOTAL blocks=5\n"), "{report}");
    }

    #[test]
    fn contradiction_hasvalue_excluded_by_in_is_reported() {
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/HasBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:hasValue <https://ex/X> ] .\n\
             <https://ex/InBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:in ( <https://ex/Y> <https://ex/Z> ) ] .\n",
        );
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(
            report.contains(
                "  <https://ex/HasBlock> ⊗ <https://ex/InBlock> — path <https://ex/p>: \
                 sh:hasValue <https://ex/X> excluded by sh:in (<https://ex/Y> <https://ex/Z>)"
            ),
            "{report}"
        );
    }

    #[test]
    fn contradiction_min_over_max_across_subclass_related_targets() {
        // The overlap rides the hierarchy: Sub ⊑ Super, so a Sub instance is validated by both.
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/MinBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Sub> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/q> ; sh:minCount 2 ] .\n\
             <https://ex/MaxBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Super> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/q> ; sh:maxCount 1 ] .\n",
        );
        let hier = lattice_hier(&[("https://ex/Sub", "https://ex/Super")]);
        let report = lattice_report(&blocks, &hier);
        assert!(
            report.contains(
                "  <https://ex/MaxBlock> ⊗ <https://ex/MinBlock> — path <https://ex/q>: \
                 sh:minCount 2 vs sh:maxCount 1 (min > max across the pair)"
            ),
            "{report}"
        );
        // Without the hierarchy the focus sets never meet — no contradiction.
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(report.contains("CONTRADICTIONS\n  none\n"), "{report}");
    }

    #[test]
    fn contradiction_incompatible_datatypes_on_a_required_path() {
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             <https://ex/DecBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:datatype xsd:decimal ] .\n\
             <https://ex/StrBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:datatype xsd:string ] .\n",
        );
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(
            report.contains("sh:datatype <http://www.w3.org/2001/XMLSchema#decimal> vs sh:datatype <http://www.w3.org/2001/XMLSchema#string> (no value can carry both)"),
            "{report}"
        );
    }

    #[test]
    fn redundancy_superclass_block_subsumes_subclass_block() {
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/SubBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Sub> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ] .\n\
             <https://ex/SuperBlock> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/Super> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
        );
        let hier = lattice_hier(&[("https://ex/Sub", "https://ex/Super")]);
        let report = lattice_report(&blocks, &hier);
        assert!(
            report.contains("  <https://ex/SubBlock> ⊑ <https://ex/SuperBlock>\n"),
            "the superclass block's stricter payload entails the subclass block: {report}"
        );
        assert!(
            !report.contains("  <https://ex/SuperBlock> ⊑ <https://ex/SubBlock>"),
            "the reverse never holds (the subclass block covers fewer focus nodes): {report}"
        );
        // Without the hierarchy the coverage leg fails — no redundancy.
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(report.contains("REDUNDANCIES\n  none\n"), "{report}");
    }

    #[test]
    fn hoist_identical_constraint_on_two_siblings_reports_the_lub() {
        let blocks = lattice_blocks(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             <https://ex/C1Block> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C1> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n\
             <https://ex/C2Block> a sh:NodeShape ;\n\
             \x20\x20sh:targetClass <https://ex/C2> ;\n\
             \x20\x20sh:property [ sh:path <https://ex/p> ; sh:minCount 1 ; sh:maxCount 1 ] .\n",
        );
        let hier = lattice_hier(&[
            ("https://ex/C1", "https://ex/P0"),
            ("https://ex/C2", "https://ex/P0"),
        ]);
        let report = lattice_report(&blocks, &hier);
        assert!(
            report.contains(
                "  lub=<https://ex/P0> constraint=[sh:path <https://ex/p> ; sh:minCount 1 ; \
                 sh:maxCount 1] siblings=<https://ex/C1>,<https://ex/C2>\n"
            ),
            "{report}"
        );
        // Siblings under DIFFERENT parents never hoist.
        let hier = lattice_hier(&[
            ("https://ex/C1", "https://ex/P0"),
            ("https://ex/C2", "https://ex/P1"),
        ]);
        let report = lattice_report(&blocks, &hier);
        assert!(report.contains("HOISTS\n  none\n"), "{report}");
    }

    #[test]
    fn classification_over_the_real_gmeow_shapes_totals_85() {
        let (text, _) = real_gmeow_shapes();
        let ds = parse_dataset(text.as_bytes(), "text/turtle", None)
            .expect("the committed shapes file must parse");
        let mut errors = Vec::new();
        let blocks = read_shapes(&ds, &mut errors);
        assert!(errors.is_empty(), "every committed block reads: {errors:?}");
        let report = lattice_report(&blocks, &lattice_hier(&[]));
        assert!(
            report.contains("  TOTAL blocks=85\n"),
            "the by-block census of the committed file is exactly 85: {}",
            report.lines().rev().take(12).collect::<Vec<_>>().join("\n")
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
