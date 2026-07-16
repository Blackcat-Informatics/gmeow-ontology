// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single canonical assembly of a [`AuthoringPacket`]. Every step is
//! deterministic: term partition is IRI-sorted, every neighbour/closure set is
//! `BTreeSet`-ordered, exemplar order is `(tier desc, IRI asc)`, and the grounding
//! cross-table is emitted in a fixed `(term, attribute, predicate)` order — so a
//! packet assembled twice from identical inputs is identical.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use gmeow_docs::i18n::Translations;
use gmeow_docs::i18n_compile::LOCALIZABLE_PREDICATES;
use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::sssom::equivalence_cells;
use gmeow_slice_quality::dataset_from_paths;
use gmeow_slice_quality::graph;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};

use crate::error;
use crate::model::{
    Annotation, AuthoringPacket, ClosureEntry, CoveredTerm, GroundingAttribute, GroundingCell,
    GroundingMargins, ObjTerm, Triple,
};
use crate::{digest, ns};

/// The SINGLE canonical partition chunk size (`~25` terms per batch). Both
/// [`assemble_packet`]'s own partition and the pipeline `slice_brief` stage's batch
/// enumeration ([`batch_count`]) read this one constant — re-exported from the crate
/// root — so the CLI and the projection can never partition a slice's terms
/// differently.
pub const CHUNK: usize = 25;

/// The number of `CHUNK`-sized batches a slice with `filtered_len` (axis-)filtered
/// defined terms partitions into (`0` for an empty slice). This is the SINGLE
/// canonical batch-enumeration arithmetic: the pipeline `slice_brief` stage calls it
/// (via the crate root re-export) to know how many packets to materialize per slice,
/// rather than re-deriving `div_ceil(CHUNK)` against a second copy of `CHUNK`.
#[must_use]
pub fn batch_count(filtered_len: usize) -> usize {
    filtered_len.div_ceil(CHUNK)
}

/// The half-open term-index range batch `batch` covers within a `filtered_len`-term
/// (axis-)filtered, IRI-sorted term list, or `None` if `batch` is out of range
/// (`batch * CHUNK >= filtered_len`). The single canonical partition-range
/// arithmetic underlying [`assemble_packet`]'s batch slice.
#[must_use]
pub fn batch_range(batch: usize, filtered_len: usize) -> Option<Range<usize>> {
    let start = batch * CHUNK;
    if start >= filtered_len {
        return None;
    }
    let end = (start + CHUNK).min(filtered_len);
    Some(start..end)
}

/// The six authoring "coat" predicates whose per-term presence count is a term's
/// coat-completeness (annotation completeness): `rdfs:label`, `skos:definition`,
/// `skos:example`, `gmeow:useWhen`, `gmeow:avoidWhen`, `gmeow:howToUse`. Fully
/// qualified so the reading is source-only and stable. Within the SHACL-conforming
/// terms this count is the ORDERING key of the SINGLE canonical exemplar tiering
/// ([`exemplar_tiers`]) — both the pipeline stage and the `gmeow slice brief` CLI
/// derive their injected exemplar tiers from it, so an in-repo slice's CLI brief and
/// its committed projection tier identically.
const COAT_PREDICATES: [&str; 6] = [
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/2004/02/skos/core#example",
    "https://blackcatinformatics.ca/gmeow/useWhen",
    "https://blackcatinformatics.ca/gmeow/avoidWhen",
    "https://blackcatinformatics.ca/gmeow/howToUse",
];

/// The injected inputs to a single packet assembly. Exemplar tiers are injected by
/// the caller (dependency inversion), so the library never picks a scoring authority.
/// Both callers inject the SAME authority — [`exemplar_tiers`] — whose ranks are
/// GATED by SHACL per-term conformance (a term is eligible iff it passed the
/// validation gate — no `sh:Violation`-severity result names it as focus node) and
/// ORDERED by coat completeness. A term with any shape violation is rank `0` and is
/// never surfaced: exemplars are coats that passed the SHACL validation gate.
pub struct BriefInputs<'a> {
    /// The slice directory (holding `manifest.ttl`, `module.ttl`, …).
    pub slice_dir: &'a Path,
    /// The subdomain axis to filter terms by (local-name prefix). `None` = whole slice.
    pub axis: Option<&'a str>,
    /// The zero-based batch index of the `CHUNK`-term chunk to cover. `None` with no
    /// axis = the whole slice as one packet (batch 0).
    pub batch: Option<u32>,
    /// `term-IRI -> quality tier rank` (higher = better). Empty => no exemplars.
    pub exemplar_tiers: &'a BTreeMap<String, i64>,
    /// The number of exemplar coats to seek.
    pub exemplar_target: usize,
}

/// Assemble the authoring packet for `inputs`.
///
/// # Errors
/// Hard-fails if the slice cannot be read, declares no `gmeow:Slice`, or the batch
/// request is out of range. A missing translation / external mapping is NOT an
/// error — it is recorded as an explicit "absent" grounding cell.
pub fn assemble_packet(inputs: &BriefInputs) -> gmeow_errors::Result<AuthoringPacket> {
    let slice_dir = inputs.slice_dir;

    // The slice authoring dataset (module + examples + tests + mappings) and the
    // slice identity — loaded through the SHARED helpers so `exemplar_tiers`
    // and this assembly read byte-identical inputs.
    let ds = load_slice_dataset(slice_dir)?;
    let slice_iri = slice_identity(slice_dir)?;

    // 1. PARTITION.
    let terms_all = defined_terms(&ds, &slice_iri);
    let filtered: Vec<String> = match inputs.axis {
        Some(ax) => terms_all
            .iter()
            .filter(|t| ns::local_name(t).starts_with(ax))
            .cloned()
            .collect(),
        None => terms_all.clone(),
    };
    let (batch_index, covered_iris): (u32, Vec<String>) = match (inputs.axis, inputs.batch) {
        (None, None) => (0, filtered.clone()),
        _ => {
            let b = inputs.batch.unwrap_or(0);
            let Some(range) = batch_range(b as usize, filtered.len()) else {
                return Err(gmeow_errors::Diag::of_kind(error::Partition {
                    detail: format!(
                        "batch {b} out of range: term {} >= {} filtered term(s) in slice {slice_iri}\
                         {}",
                        (b as usize) * CHUNK,
                        filtered.len(),
                        inputs
                            .axis
                            .map(|a| format!(" (axis \"{a}\")"))
                            .unwrap_or_default()
                    ),
                }));
            };
            (b, filtered[range].to_vec())
        }
    };
    let covered_set: BTreeSet<&String> = covered_iris.iter().collect();

    // 2. PER-TERM CONTENT.
    let terms: Vec<CoveredTerm> = covered_iris
        .iter()
        .map(|iri| build_covered_term(&ds, iri))
        .collect();

    // 3. EXEMPLARS (injected tiers; no scoring authority in the library).
    let mut cand: Vec<(i64, String)> = terms_all
        .iter()
        .filter_map(|t| {
            inputs
                .exemplar_tiers
                .get(t)
                .filter(|&&r| r > 0)
                .map(|&r| (r, t.clone()))
        })
        .collect();
    cand.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let found = cand.len().min(inputs.exemplar_target);
    let exemplars: Vec<String> = cand
        .into_iter()
        .take(inputs.exemplar_target)
        .map(|(_, t)| t)
        .collect();
    let exemplar_shortfall = inputs.exemplar_target.saturating_sub(found);

    // Alignment linkage (external + relations for the disagreement check).
    let view = DslView::new(&ds);
    let cells = equivalence_cells(&view);
    let mut ext_by_subject: BTreeMap<String, Vec<ExtCell>> = BTreeMap::new();
    let mut by_ext_target: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut related: BTreeSet<(String, String)> = BTreeSet::new();
    for c in &cells {
        let subj_covered = covered_set.contains(&c.subject);
        if ns::is_internal(&c.obj) {
            if subj_covered && covered_set.contains(&c.obj) {
                related.insert(ordered_pair(&c.subject, &c.obj));
            }
        } else {
            if subj_covered {
                ext_by_subject
                    .entry(c.subject.clone())
                    .or_default()
                    .push(ExtCell {
                        obj: c.obj.clone(),
                        predicate: c.predicate.clone(),
                        object_label: c.object_label.clone(),
                        confidence: c.confidence,
                    });
                by_ext_target
                    .entry(c.obj.clone())
                    .or_default()
                    .insert(c.subject.clone());
            }
        }
    }
    for subjects in by_ext_target.values() {
        let v: Vec<&String> = subjects.iter().collect();
        for i in 0..v.len() {
            for j in (i + 1)..v.len() {
                related.insert(ordered_pair(v[i], v[j]));
            }
        }
    }
    for cell_vec in ext_by_subject.values_mut() {
        cell_vec.sort_by(|a, b| {
            a.predicate
                .cmp(&b.predicate)
                .then_with(|| a.obj.cmp(&b.obj))
        });
    }

    // 5/6. CROSS-LINGUAL JOIN inputs.
    let catalog = purrdf::slice::SliceCatalog::discover(
        slice_dir,
        purrdf::SliceVocab::for_namespace(ns::GMEOW),
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!(
                "{}: slice-catalog discovery failed: {e}",
                slice_dir.display()
            ),
        })
    })?;
    let translations = Translations::from_catalog(&catalog);
    let langs = [GroundingAttribute::Fr, GroundingAttribute::Zh];

    // Translation-disagreement map: (term, lang, predicate-curie) -> counterpart term.
    let mut conflict_map: BTreeMap<(String, &'static str, String), String> = BTreeMap::new();
    for (a, b) in &related {
        for attr in langs {
            let lang = attr.lang().expect("language attribute");
            for pred in LOCALIZABLE_PREDICATES {
                let va = translations.lookup(a, pred, lang);
                let vb = translations.lookup(b, pred, lang);
                if let (Some(x), Some(y)) = (va, vb)
                    && x != y
                {
                    let key = ns::curie(pred);
                    conflict_map
                        .entry((a.clone(), lang, key.clone()))
                        .or_insert_with(|| b.clone());
                    conflict_map
                        .entry((b.clone(), lang, key))
                        .or_insert_with(|| a.clone());
                }
            }
        }
    }

    // Packet identity (mint before cells, which reference it).
    let axis_seg = inputs
        .axis
        .map(ns::safe_segment)
        .unwrap_or_else(|| "whole".to_string());
    let packet_iri = format!("{slice_iri}/authoring-packet/{axis_seg}/batch-{batch_index}");

    // 4/5/6. GROUNDING CELLS.
    let mut grounding: Vec<GroundingCell> = Vec::new();
    for term in &terms {
        let seg = ns::safe_segment(ns::local_name(&term.iri));
        // English is always present (the source language).
        grounding.push(GroundingCell {
            cell_iri: format!("{packet_iri}/cell/{seg}/en"),
            term: term.iri.clone(),
            attribute: GroundingAttribute::En,
            present: true,
            predicate: None,
            value: None,
            external_entity: None,
            external_label: None,
            align_predicate: None,
            confidence: None,
            conflict: false,
            conflict_with: None,
        });
        // fr / zh, one cell per (localizable predicate present on the term).
        let preds = term_localizable_predicates(term);
        for attr in langs {
            let lang = attr.lang().expect("language attribute");
            for pred in &preds {
                let curie = ns::curie(pred);
                let pred_seg = ns::safe_segment(&curie);
                let hit = translations.lookup(&term.iri, pred, lang);
                let (conflict, conflict_with) =
                    match conflict_map.get(&(term.iri.clone(), lang, curie.clone())) {
                        Some(other) if hit.is_some() => (true, Some(other.clone())),
                        _ => (false, None),
                    };
                grounding.push(GroundingCell {
                    cell_iri: format!("{packet_iri}/cell/{seg}/{}/{pred_seg}", attr.tag()),
                    term: term.iri.clone(),
                    attribute: attr,
                    present: hit.is_some(),
                    predicate: Some(curie),
                    value: hit.map(str::to_string),
                    external_entity: None,
                    external_label: None,
                    align_predicate: None,
                    confidence: None,
                    conflict,
                    conflict_with,
                });
            }
        }
        // external-mapped: one present cell per mapping, or one explicit-absent cell.
        match ext_by_subject.get(&term.iri) {
            Some(cell_vec) if !cell_vec.is_empty() => {
                for (i, ext) in cell_vec.iter().enumerate() {
                    grounding.push(GroundingCell {
                        cell_iri: format!("{packet_iri}/cell/{seg}/external/{i}"),
                        term: term.iri.clone(),
                        attribute: GroundingAttribute::ExternalMapped,
                        present: true,
                        predicate: None,
                        value: None,
                        external_entity: Some(ext.obj.clone()),
                        external_label: (!ext.object_label.is_empty())
                            .then(|| ext.object_label.clone()),
                        align_predicate: Some(ns::local_name(&ext.predicate).to_string()),
                        confidence: ext.confidence,
                        conflict: false,
                        conflict_with: None,
                    });
                }
            }
            _ => grounding.push(GroundingCell {
                cell_iri: format!("{packet_iri}/cell/{seg}/external/absent"),
                term: term.iri.clone(),
                attribute: GroundingAttribute::ExternalMapped,
                present: false,
                predicate: None,
                value: None,
                external_entity: None,
                external_label: None,
                align_predicate: None,
                confidence: None,
                conflict: false,
                conflict_with: None,
            }),
        }
    }
    // exemplar attribute cells: one present cell per exemplar term.
    for ex in &exemplars {
        let seg = ns::safe_segment(ns::local_name(ex));
        grounding.push(GroundingCell {
            cell_iri: format!("{packet_iri}/cell/{seg}/exemplar"),
            term: ex.clone(),
            attribute: GroundingAttribute::Exemplar,
            present: true,
            predicate: None,
            value: None,
            external_entity: None,
            external_label: None,
            align_predicate: None,
            confidence: None,
            conflict: false,
            conflict_with: None,
        });
    }
    grounding.sort_by(|a, b| {
        (
            &a.term,
            a.attribute,
            a.predicate.as_deref().unwrap_or(""),
            &a.cell_iri,
        )
            .cmp(&(
                &b.term,
                b.attribute,
                b.predicate.as_deref().unwrap_or(""),
                &b.cell_iri,
            ))
    });

    // 7. PER-ATTRIBUTE MARGINS (the sparse cross-table's present/absent counts) and
    //    the DIGEST over the same semantic body the sparse turtle emits.
    let margins = GroundingMargins::from_cells(&grounding);
    let digest = digest::packet_digest(
        &slice_iri,
        inputs.axis,
        batch_index,
        &terms,
        &exemplars,
        &margins,
        &grounding,
    );

    Ok(AuthoringPacket {
        packet_iri,
        source_slice: slice_iri,
        axis: inputs.axis.map(str::to_string),
        batch: batch_index,
        digest,
        term_count: terms.len(),
        exemplar_shortfall,
        margins,
        terms,
        exemplars,
        grounding,
    })
}

/// One external-mapped alignment cell for a covered term (the external half of a
/// `gmeow:TermEquivalence`).
struct ExtCell {
    obj: String,
    predicate: String,
    object_label: String,
    confidence: Option<f64>,
}

/// The `(min, max)` string-ordered pair — the canonical key of an equivalence
/// relation so `(a,b)` and `(b,a)` are the same relation.
fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Load the slice's authoring dataset exactly the way [`assemble_packet`] reads it:
/// the slice graph (module + examples + tests, via `slice_ttl_paths`) PLUS the
/// alignment linkage under `mappings/` the external-grounding JOIN needs
/// (`slice_ttl_paths` omits the latter, so it is gathered explicitly). This is the
/// SINGLE dataset-loading path both the packet assembly and [`exemplar_tiers`]
/// go through, so their inputs are byte-identical.
///
/// # Errors
/// Hard-fails if any of the slice's Turtle sources cannot be read or parsed.
fn load_slice_dataset(slice_dir: &Path) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let mut paths = gmeow_slice_quality::report::slice_ttl_paths(slice_dir);
    collect_ttl(&slice_dir.join("mappings"), &mut paths)?;
    paths.sort();
    paths.dedup();
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    dataset_from_paths(&path_refs)
}

/// The slice's identity IRI — the sole `gmeow:Slice` individual declared by
/// `manifest.ttl` (which is NOT part of the slice graph). Shared by the packet
/// assembly and [`exemplar_tiers`] so both agree on the slice IRI.
///
/// # Errors
/// Hard-fails if `manifest.ttl` cannot be read or declares no `gmeow:Slice`.
fn slice_identity(slice_dir: &Path) -> gmeow_errors::Result<String> {
    let manifest = slice_dir.join("manifest.ttl");
    let mds = dataset_from_paths(&[manifest.as_path()])?;
    graph::instances_of(&mds, &graph::g("Slice"))
        .into_iter()
        .next()
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(error::Partition {
                detail: format!("{} declares no gmeow:Slice", manifest.display()),
            })
        })
}

/// A pre-loaded SHACL shape union, assembled once via [`load_shape_union`] and reused
/// across per-slice [`exemplar_tiers`] calls so the pipeline stage never reloads the
/// shape files per slice.
pub type ShapeUnion = purrdf::shapes::shapes::Shapes;

/// Load the repo's SHACL shape union ONCE — the same union the live `make validate`
/// gate enforces (`purrdf::shapes::shape_union::load_shapes`) — for reuse across
/// per-slice [`exemplar_tiers`] calls. The pipeline stage loads it once per `run()`
/// and threads it into every slice's tiering; the CLI loads it once for the single
/// slice it briefs. Both therefore gate exemplars against a byte-identical shape
/// union, preserving CLI↔pipeline parity in a checkout.
///
/// # Errors
/// Hard-fails if the shape union cannot be assembled (e.g. `generated/shapes/` is
/// missing — the shape gate is a REQUIRED input, never silently skipped or degraded
/// to an ungated tiering).
pub fn load_shape_union(repo_root: &Path) -> gmeow_errors::Result<ShapeUnion> {
    purrdf::shapes::shape_union::load_shapes(repo_root)
        .map(|(_dataset, shapes)| shapes)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!(
                    "{}: SHACL shape-union load failed: {e}",
                    repo_root.display()
                ),
            })
        })
}

/// Walk up from `slice_dir` to the repo root — the nearest ancestor carrying the
/// generated SHACL shape directory (`generated/shapes/`) that [`load_shape_union`]
/// reads. In an in-repo checkout this resolves to the SAME root the pipeline threads
/// as its `input.root`, so the `gmeow slice brief` CLI gates exemplars against the
/// same shape union as the committed pipeline projection.
///
/// # Errors
/// Hard-fails if no ancestor carries `generated/shapes/` — the slice is not inside a
/// gmeow checkout, so there is no shape gate to run (never silently degrade to an
/// ungated tiering).
pub fn resolve_repo_root(slice_dir: &Path) -> gmeow_errors::Result<PathBuf> {
    let start = slice_dir
        .canonicalize()
        .unwrap_or_else(|_| slice_dir.to_path_buf());
    for ancestor in start.ancestors() {
        if ancestor.join("generated").join("shapes").is_dir() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(gmeow_errors::Diag::of_kind(error::Io {
        detail: format!(
            "{}: no ancestor carries generated/shapes/ — cannot resolve the SHACL shape \
             union required to gate exemplars by per-term conformance",
            slice_dir.display()
        ),
    }))
}

/// The SINGLE canonical per-term exemplar tiering, shared by the pipeline `slice_brief`
/// stage and the `gmeow slice brief` CLI. A term is an ELIGIBLE exemplar iff it PASSED
/// the SHACL validation gate — validating the slice's authoring dataset (loaded exactly
/// as [`assemble_packet`] loads it) against the repo shape union `shapes` yields NO
/// `sh:Violation`-severity result naming it as focus node — AND it carries a non-empty
/// authoring coat. Within the conforming set, coat completeness (the count of the six
/// [`COAT_PREDICATES`] present, `0..=6`) is the ORDERING key, so a fuller conforming
/// coat outranks a sparser one.
///
/// Concretely the returned `term-IRI -> rank` map sets
/// `rank = if term conforms { completeness_count(0..=6) } else { 0 }`. Selection in
/// [`assemble_packet`] gates on `rank > 0`, so a term with ANY shape violation (rank
/// `0`) is never surfaced as an exemplar, and neither is a conforming-but-empty coat.
/// This makes every surfaced exemplar provably a coat that passed the validation gate
/// — not merely a source-only completeness count that could hide a shape violation.
///
/// Conformance is computed as the ABSENCE of a violating result (SHACL emits
/// violations, not affirmative per-node conforms flags), so a term's eligibility is
/// its complement in the violating-focus-node set. Deterministic: the dataset load,
/// term enumeration, and validation are all deterministic, so the pipeline projection
/// and the live CLI brief tier an in-repo slice's terms identically.
///
/// # Errors
/// Hard-fails if the slice graph cannot be read, `manifest.ttl` declares no
/// `gmeow:Slice`, or SHACL validation errors (a malformed slice / shape — never a
/// silent skip).
pub fn exemplar_tiers(
    slice_dir: &Path,
    shapes: &ShapeUnion,
) -> gmeow_errors::Result<BTreeMap<String, i64>> {
    let ds = load_slice_dataset(slice_dir)?;
    let slice_iri = slice_identity(slice_dir)?;
    let terms = defined_terms(&ds, &slice_iri);

    // The SHACL per-term conformance gate: validate the slice's authoring dataset
    // against the repo shape union and collect the focus-node term IRIs that carry a
    // Violation-severity result. A term "passed the gate" iff it is NOT in this set.
    let report = purrdf::shapes::engine::validate_dataset(&ds, shapes).map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!("{}: SHACL validation failed: {e}", slice_dir.display()),
        })
    })?;
    let violating: BTreeSet<String> = report
        .results
        .iter()
        .filter(|r| r.severity == purrdf::shapes::report::Severity::Violation)
        .map(purrdf::shapes::report::ValidationResult::focus_value)
        .collect();

    let pred_ids: Vec<Option<purrdf::TermId>> =
        COAT_PREDICATES.iter().map(|p| graph::id(&ds, p)).collect();
    let mut tiers: BTreeMap<String, i64> = BTreeMap::new();
    for term in &terms {
        // A violating term is ineligible (rank 0) regardless of coat completeness; a
        // conforming term ranks by the count of coat predicates present.
        let rank = if violating.contains(term) {
            0
        } else if let Some(tid) = graph::id(&ds, term) {
            let mut count = 0i64;
            for pid in pred_ids.iter().flatten() {
                if graph::has_any(&ds, tid, *pid) {
                    count += 1;
                }
            }
            count
        } else {
            0
        };
        tiers.insert(term.clone(), rank);
    }
    Ok(tiers)
}

/// Every IRI subject the slice defines (`rdfs:isDefinedBy` the slice IRI, excluding
/// the slice individual itself), sorted ascending and deduped.
///
/// This is the SINGLE canonical slice-membership rule: the pipeline's `slice_brief`
/// stage calls this exact function (via the crate root re-export) rather than keeping
/// a second copy, so the two callers can never drift on what counts as "in the slice".
#[must_use]
pub fn defined_terms(ds: &RdfDataset, slice_iri: &str) -> Vec<String> {
    let (Some(pred), Some(slice_id)) = (
        graph::id(ds, ns::RDFS_IS_DEFINED_BY),
        graph::id(ds, slice_iri),
    ) else {
        return Vec::new();
    };
    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(pred), Some(slice_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) if iri != slice_iri => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The localizable annotation predicates the term actually carries an English coat
/// value for (sorted, deduped) — the set the fr/zh JOIN iterates.
fn term_localizable_predicates(term: &CoveredTerm) -> Vec<String> {
    let localizable: HashSet<&str> = LOCALIZABLE_PREDICATES.iter().copied().collect();
    let mut preds: BTreeSet<String> = BTreeSet::new();
    for a in &term.coat {
        if localizable.contains(a.predicate.as_str()) {
            preds.insert(a.predicate.clone());
        }
    }
    preds.into_iter().collect()
}

/// Build the full authoring content of one covered term.
fn build_covered_term(ds: &RdfDataset, iri: &str) -> CoveredTerm {
    let mut coat: BTreeSet<Annotation> = BTreeSet::new();
    let mut axioms: BTreeSet<Triple> = BTreeSet::new();
    let mut blank_stack: Vec<purrdf::TermId> = Vec::new();

    if let Some(tid) = graph::id(ds, iri) {
        for q in ds.quads_for_pattern(Some(tid), None, None, GraphMatch::Any) {
            let TermRef::Iri(pred) = ds.resolve(q.p) else {
                continue;
            };
            let pred = pred.to_owned();
            match ds.resolve(q.o) {
                TermRef::Literal {
                    lexical,
                    datatype,
                    language,
                    ..
                } => {
                    if pred == ns::GMEOW_DEFINITION_DIGEST {
                        continue; // content-address metadata, not part of the coat
                    }
                    let _ = datatype; // coat carries lexical + language; datatype elided
                    coat.insert(Annotation {
                        predicate: pred,
                        language: language.map(str::to_string),
                        value: lexical.to_owned(),
                    });
                }
                other => {
                    if let TermRef::Blank { .. } = other {
                        blank_stack.push(q.o);
                    }
                    axioms.insert(Triple {
                        predicate: pred,
                        object: obj_term(ds, q.o),
                    });
                }
            }
        }
    }

    // depth-1 CBD: the blank-node closure reachable from the term's axioms.
    let mut neighbors: BTreeSet<Triple> = BTreeSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(bid) = blank_stack.pop() {
        let key = match ds.resolve(bid) {
            TermRef::Blank { label, .. } => label.to_owned(),
            _ => continue,
        };
        if !visited.insert(key) {
            continue;
        }
        for q in ds.quads_for_pattern(Some(bid), None, None, GraphMatch::Any) {
            let TermRef::Iri(pred) = ds.resolve(q.p) else {
                continue;
            };
            if let TermRef::Blank { .. } = ds.resolve(q.o) {
                blank_stack.push(q.o);
            }
            neighbors.insert(Triple {
                predicate: pred.to_owned(),
                object: obj_term(ds, q.o),
            });
        }
    }

    let coat: Vec<Annotation> = coat.into_iter().collect();
    let axioms: Vec<Triple> = axioms.into_iter().collect();
    let neighbors: Vec<Triple> = neighbors.into_iter().collect();

    let label = coat
        .iter()
        .find(|a| a.predicate == ns::RDFS_LABEL)
        .map(|a| a.value.clone());
    let definition = coat
        .iter()
        .find(|a| a.predicate == ns::SKOS_DEFINITION)
        .map(|a| a.value.clone());

    // definitional-dependency closure: label + definition of every referenced term.
    let mut refs: BTreeSet<String> = BTreeSet::new();
    for t in axioms.iter().chain(neighbors.iter()) {
        if let ObjTerm::Iri(i) = &t.object
            && i != iri
        {
            refs.insert(i.clone());
        }
    }
    let mut closure: Vec<ClosureEntry> = Vec::new();
    for r in refs {
        let Some(rid) = graph::id(ds, &r) else {
            continue;
        };
        let rlabel = graph::id(ds, ns::RDFS_LABEL).and_then(|p| graph::one_lit(ds, rid, p));
        let rdef = graph::id(ds, ns::SKOS_DEFINITION).and_then(|p| graph::one_lit(ds, rid, p));
        if rlabel.is_some() || rdef.is_some() {
            closure.push(ClosureEntry {
                iri: r,
                label: rlabel,
                definition: rdef,
            });
        }
    }

    let definition_digest = graph::id(ds, ns::GMEOW_DEFINITION_DIGEST)
        .and_then(|p| graph::id(ds, iri).and_then(|tid| graph::one_lit(ds, tid, p)));

    let content_digest = digest::term_content_digest(&digest::TermDigestInput {
        iri,
        definition_digest,
        label: &label,
        definition: &definition,
        coat: &coat,
        axioms: &axioms,
        neighbors: &neighbors,
        closure: &closure,
    });

    CoveredTerm {
        iri: iri.to_string(),
        label,
        definition,
        coat,
        axioms,
        neighbors,
        closure,
        content_digest,
    }
}

/// Resolve an object term id to an owned [`ObjTerm`]. A quoted-triple object (RDF
/// 1.2) is kept losslessly as its rendered form under the IRI variant rather than
/// silently dropped.
fn obj_term(ds: &RdfDataset, oid: purrdf::TermId) -> ObjTerm {
    match ds.resolve(oid) {
        TermRef::Iri(i) => ObjTerm::Iri(i.to_owned()),
        TermRef::Blank { label, .. } => ObjTerm::Blank(label.to_owned()),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => ObjTerm::Literal {
            lexical: lexical.to_owned(),
            datatype: resolve_iri(ds, datatype).unwrap_or_else(|| ns::XSD_STRING.to_string()),
            language: language.map(str::to_string),
        },
        TermRef::Triple { s, p, o } => ObjTerm::Iri(format!(
            "<<{} {} {}>>",
            term_render(ds, s),
            term_render(ds, p),
            term_render(ds, o)
        )),
    }
}

/// The IRI string of a term id, if it resolves to an IRI.
fn resolve_iri(ds: &RdfDataset, id: purrdf::TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Iri(i) => Some(i.to_owned()),
        _ => None,
    }
}

/// A compact rendering of a term id for a quoted-triple object.
fn term_render(ds: &RdfDataset, id: purrdf::TermId) -> String {
    match ds.resolve(id) {
        TermRef::Iri(i) => format!("<{i}>"),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal { lexical, .. } => format!("\"{lexical}\""),
        TermRef::Triple { .. } => "<<...>>".to_string(),
    }
}

/// Recursively collect existing `.ttl` files under `dir` into `out`. A directory
/// that does not exist is a legitimate "absent" input (`Ok`, nothing collected);
/// any OTHER `read_dir`/entry/file-type error (permission denied, I/O error,
/// not-a-directory, symlink loop, ...) is a HARD FAIL — propagated, never
/// swallowed into a silent "no translations" result.
///
/// # Errors
/// Propagates any `read_dir`/entry/`file_type` error other than
/// [`std::io::ErrorKind::NotFound`].
fn collect_ttl(dir: &Path, out: &mut Vec<PathBuf>) -> gmeow_errors::Result<()> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: read_dir failed: {e}", dir.display()),
            }));
        }
    };
    for entry in rd {
        let entry = entry.map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: directory entry read failed: {e}", dir.display()),
            })
        })?;
        let file_type = entry.file_type().map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: file_type failed: {e}", entry.path().display()),
            })
        })?;
        let p = entry.path();
        if file_type.is_dir() {
            collect_ttl(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod collect_ttl_tests {
    use super::collect_ttl;
    use std::path::PathBuf;

    /// A deterministic (non-random) scratch path under the process temp dir,
    /// namespaced by test name so parallel tests never collide.
    fn scratch_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gmeow-slice-brief-collect_ttl-{test_name}"))
    }

    /// A missing directory is a legitimate "absent" input: `Ok(())`, nothing
    /// collected — never an error, and never silently treated as anything but
    /// empty.
    #[test]
    fn absent_directory_is_ok_and_empty() {
        let dir = scratch_path("absent_directory_is_ok_and_empty");
        // Idempotent: make sure no leftover state from a prior aborted run exists.
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists(), "precondition: {dir:?} must not exist");

        let mut out = Vec::new();
        let result = collect_ttl(&dir, &mut out);

        assert!(
            result.is_ok(),
            "a NotFound read_dir must be treated as absent (Ok), got {result:?}"
        );
        assert!(
            out.is_empty(),
            "an absent directory must collect zero paths, got {out:?}"
        );
    }

    /// A `read_dir` failure that is NOT `NotFound` (here: the parent path
    /// component is a plain file, so the OS refuses with `NotADirectory`/`ENOTDIR`)
    /// MUST propagate as an `Err`, never be laundered into "no .ttl files here".
    /// This is deterministic and does not depend on running as non-root (unlike a
    /// permission-bits test, which root would bypass).
    #[test]
    fn unreadable_non_directory_parent_errors() {
        let marker_file = scratch_path("unreadable_non_directory_parent_errors");
        let _ = std::fs::remove_file(&marker_file);
        std::fs::write(&marker_file, b"not a directory").expect("write marker file");

        // `marker_file` is a plain file, so `marker_file/mappings` cannot be a
        // directory: `read_dir` must fail with something other than `NotFound`.
        let bogus_dir = marker_file.join("mappings");
        let mut out = Vec::new();
        let result = collect_ttl(&bogus_dir, &mut out);

        assert!(
            result.is_err(),
            "a non-NotFound read_dir error must propagate as Err, got {result:?}"
        );
        assert!(
            out.is_empty(),
            "no paths must be collected on the error path, got {out:?}"
        );

        let _ = std::fs::remove_file(&marker_file);
    }
}

#[cfg(test)]
mod partition_tests {
    use super::{CHUNK, batch_count, batch_range};

    /// An empty (zero-term) slice partitions into zero batches, and every batch
    /// index is out of range.
    #[test]
    fn empty_slice_has_zero_batches() {
        assert_eq!(batch_count(0), 0, "an empty slice must have zero batches");
        assert_eq!(
            batch_range(0, 0),
            None,
            "batch 0 of an empty slice must be out of range"
        );
    }

    /// A term count that is an EXACT multiple of `CHUNK` partitions into exactly
    /// `len / CHUNK` full batches — no trailing empty batch.
    #[test]
    fn exact_multiple_has_no_remainder_batch() {
        let len = CHUNK * 3;
        assert_eq!(
            batch_count(len),
            3,
            "an exact multiple of CHUNK must partition into len / CHUNK batches"
        );
        assert_eq!(batch_range(0, len), Some(0..CHUNK));
        assert_eq!(batch_range(1, len), Some(CHUNK..(2 * CHUNK)));
        assert_eq!(batch_range(2, len), Some((2 * CHUNK)..len));
        assert_eq!(
            batch_range(3, len),
            None,
            "the batch just past an exact multiple must be out of range"
        );
    }

    /// A term count with a nonzero remainder over `CHUNK` gets one extra, short
    /// final batch covering only the remainder.
    #[test]
    fn remainder_gets_a_short_final_batch() {
        let len = CHUNK * 2 + 7;
        assert_eq!(
            batch_count(len),
            3,
            "a remainder must round the batch count up (ceil)"
        );
        assert_eq!(batch_range(0, len), Some(0..CHUNK));
        assert_eq!(batch_range(1, len), Some(CHUNK..(2 * CHUNK)));
        assert_eq!(
            batch_range(2, len),
            Some((2 * CHUNK)..len),
            "the final batch must be short, covering only the remainder"
        );
        assert_eq!(
            batch_range(3, len),
            None,
            "past the last (short) batch must be out of range"
        );
    }
}
