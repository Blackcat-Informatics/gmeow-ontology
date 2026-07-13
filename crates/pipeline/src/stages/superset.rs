// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The superset gate: `gmeow.gts` is a superset of `generated/`.
//!
//! For every committed path under `generated/`, this gate resolves the path's
//! carrier representative from the **shipped** bundle and reconstructs the bytes:
//!
//! * an RDF output with a canonical graph fold is reconstructed from one named graph
//!   (Turtle via the wasm-clean renderer; N-Quads via a graph-rooted serialization),
//!   and
//! * a byte-decorated output (including generated RDF reports whose committed files
//!   contain comments / section markers) is a member of one inline content-addressed
//!   archive blob.
//!
//! A committed path with no representative, a representative whose reconstruction
//! does not match the committed bytes, or a carried representative with no
//! committed counterpart is a hard failure — no skips, no optional coverage, no
//! degraded pass. The gate proves equality of the carried set, not merely a
//! one-directional superset: it sweeps both `generated/ -> bundle` (missing /
//! mismatch) and `bundle -> generated/` (orphan).
//!
//! Reconstruction reads the bundle back through [`purrdf::import_gts_events`]
//! and the GTS blob reader, closing the serialize -> parse loop, so it proves
//! byte-reconstructibility from the emitted bundle rather than from in-memory
//! carrier state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use purrdf::RdfDataset;

/// The two terminal bundles cannot byte-contain themselves; they are the only
/// committed paths the gate excludes (a bundle is not a projection of itself).
pub const EXCLUDED: [&str; 2] = ["generated/dist/gmeow.gts", "generated/dist/gmeow-full.gts"];

/// The committed-path -> carrier-representative outcome for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersetReport {
    /// Committed paths with no carrier representative in the bundle.
    pub missing: Vec<String>,
    /// Committed paths whose representative reconstructed to different bytes.
    pub mismatch: Vec<String>,
    /// Carried representatives (blob members / named-graph classes) with no
    /// committed `generated/` counterpart.
    pub orphan: Vec<String>,
}

impl SupersetReport {
    /// The gate passes when every sweep is empty.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.mismatch.is_empty() && self.orphan.is_empty()
    }
}

/// Every committed `generated/` path the shipped bundle carries, mapped to its
/// reconstructed bytes — the pure projection of `gmeow.gts` back onto the flat
/// consumer tree (PIPELINE_SPINE §6). No disk read, no comparison: this is the
/// single reconstruction authority. The superset gate ([`check_superset`]) compares
/// it against the committed tree; the fanout phase ([`crate::fanout()`]) writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleProjection {
    /// Committed repo-relative path -> reconstructed bytes.
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Reconstruct every committed `generated/` file the bundle carries, keyed by its
/// committed repo-relative path (PIPELINE_SPINE §5/§6). Drives off the *bundle's*
/// representatives, never the on-disk tree, so it reconstructs from `gmeow.gts`
/// alone — the property the fanout phase depends on. Two rep classes:
///
/// * **named-graph folds** — each EDOAL projection graph (`…/graph/projections/…`)
///   and RDF-fanout graph (`…/graph/fanout/…`) folds to its committed RDF bytes via
///   [`reconstruct_graph`]; and
/// * **inline blob members** — every archive member resolved to its committed
///   `generated/` path by [`read_blob_members`].
///
/// The two rep classes are disjoint by construction (RDF travels as a named graph,
/// opaque/byte-decorated output as a blob member), so no path is produced twice.
pub fn project_bundle(gts_bytes: &[u8]) -> Result<BundleProjection, gmeow_errors::Diag> {
    let dataset = read_dataset(gts_bytes)?;
    let dataset = dataset.as_ref();
    let blob_members = read_blob_members(gts_bytes)?;
    // The path↔representative map as DATA: the authored gmeow:fanoutExtracts rows read
    // back from the bundle (the pipeline slice rides in the default graph). The gate reads
    // these rows instead of branching in Rust (PIPELINE_SPINE §6/§7).
    let rules = read_fanout_rules(dataset)?;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // The reconstructed paths, for the bijection completeness HARD-FAIL below.
    let mut reconstructed_paths: BTreeSet<String> = BTreeSet::new();

    // Named-graph reps: fold each reconstruction graph to its committed RDF bytes.
    for iri in reconstruction_graph_iris(dataset) {
        let Some(path) =
            edoal_path_for_graph_iri(&iri).or_else(|| rdf_fanout_path_for_graph_iri(&iri))
        else {
            continue;
        };
        reconstructed_paths.insert(path.clone());
        let rep = graph_rep_for_path(&rules, &path).ok_or_else(|| {
            // A reconstruction graph IRI whose committed path resolves no fanout rule is
            // a hole in the promoted map — HARD-fail (the bijection check reports it too).
            stage_err(&format!(
                "reconstruction graph {iri} maps to {path} but no gmeow:fanoutExtracts row"
            ))
        })?;
        let folded = reconstruct_graph(dataset, &rep).ok_or_else(|| {
            stage_err(&format!(
                "reconstruction graph {iri} is present but folds to no bytes"
            ))
        })?;
        if files.insert(path.clone(), folded).is_some() {
            return Err(stage_err(&format!(
                "{path} is carried by two representatives (named graph {iri} collides)"
            )));
        }
    }

    // Inline blob members: the opaque + byte-decorated committed files under
    // `generated/`. Source archive members (`shapes/`, `slices/`, `dsl/`) are carried
    // for self-sufficiency but are not `generated/` targets — skip them.
    for (path, bytes) in blob_members {
        if !path.starts_with("generated/") {
            continue;
        }
        if files.insert(path.clone(), bytes).is_some() {
            return Err(stage_err(&format!(
                "{path} is carried by two representatives (blob member collides with a named graph)"
            )));
        }
    }

    // Bijection completeness HARD-FAIL: the promoted gmeow:fanoutExtracts rows must be a
    // bijection over the reconstruction graphs — no path unmapped/ambiguous, no stale row —
    // so reading the map from data never silently drops a path from fanout.
    check_fanout_bijection(&rules, &reconstructed_paths)?;

    Ok(BundleProjection { files })
}

/// The serialization whose output equals one named graph's committed bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphForm {
    /// Canonical Turtle (single graph, no graph label).
    Turtle,
    /// N-Triples of the projected graph (no graph label).
    NTriples,
    /// N-Quads of the graph re-rooted into the embedded graph label (the `.nq`
    /// 4th-column IRI, which differs from the fanout container IRI). RDFC-canonical.
    NQuads(&'static str),
    /// N-Quads re-rooted into the graph's OWN fanout IRI: the committed `.nq`
    /// carries the fanout container IRI itself as its 4th column. RDFC-canonical.
    NQuadsSelf,
}

/// A committed path carried as the fold of one named graph: the backing graph IRI
/// and the serialization form whose output equals the committed bytes.
struct GraphRep {
    iri: String,
    form: GraphForm,
}

/// The reconstruction-graph namespace family of a `gmeow:FanoutExtraction` row: how
/// the per-file named-graph IRI derives from the committed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FanoutFamily {
    /// `graph/fanout/<path-without-the-generated/-prefix>`.
    RdfFanout,
    /// `graph/projections/<stem>.edoal` for `generated/projections/<stem>.edoal.ttl`.
    Edoal,
}

/// One parsed `gmeow:FanoutExtraction` row — the DATA the superset gate reads in place
/// of the former hard-coded path↔representative branches (`graph_rep_for_path` /
/// `is_rdf_fanout_class` form selection).
#[derive(Debug, Clone)]
struct FanoutRule {
    /// The committed path (exact) or directory prefix (prefix), per `match_prefix`.
    path: String,
    /// `true` = prefix match, `false` = exact match.
    match_prefix: bool,
    /// An optional suffix filter refining a prefix match (the EDOAL `.edoal.ttl` case).
    suffix: Option<String>,
    /// How the reconstruction graph IRI derives from the committed path.
    family: FanoutFamily,
    /// The serialization form whose fold equals the committed bytes.
    form: GraphForm,
}

impl FanoutRule {
    /// Whether this rule matches a committed `generated/` path.
    fn matches(&self, path: &str) -> bool {
        if self.match_prefix {
            path.starts_with(&self.path) && self.suffix.as_deref().is_none_or(|s| path.ends_with(s))
        } else {
            path == self.path
        }
    }
}

/// Collect every `(literal-lexical-form, non-literal-count)` reading of one row's
/// predicate: the objects of `row gmeow:{local}` in the grouped dataset, split into
/// literal lexical forms and a count of non-literal (IRI/blank-node/triple-term)
/// objects. The cardinality/type enforcement lives in [`mandatory_literal`] and
/// [`optional_literal`], which both call this so the scan logic exists once.
fn collect_field(
    by_subject: &BTreeMap<String, Vec<purrdf::RdfQuad>>,
    row: &str,
    pred: &str,
) -> (Vec<String>, usize) {
    let mut literals = Vec::new();
    let mut non_literal = 0usize;
    if let Some(quads) = by_subject.get(row) {
        for quad in quads {
            if quad.predicate != pred {
                continue;
            }
            match &quad.object {
                purrdf::RdfTerm::Literal(lit) => literals.push(lit.lexical_form.clone()),
                _ => non_literal += 1,
            }
        }
    }
    (literals, non_literal)
}

/// A MANDATORY field: `row gmeow:{local}` must have EXACTLY ONE object, and it MUST be
/// a literal. Zero objects, more than one object (duplicate literals, or any mix with
/// non-literals), or a single non-literal object are all a HARD FAIL — no-optionality,
/// never silently the first match.
fn mandatory_literal(
    by_subject: &BTreeMap<String, Vec<purrdf::RdfQuad>>,
    row: &str,
    local: &str,
    gmeow_ns: &str,
) -> Result<String, gmeow_errors::Diag> {
    let pred = format!("{gmeow_ns}{local}");
    let (literals, non_literal) = collect_field(by_subject, row, &pred);
    let total = literals.len() + non_literal;
    if total == 0 {
        return Err(stage_err(&format!("fanout row {row} has no gmeow:{local}")));
    }
    if total > 1 {
        return Err(stage_err(&format!(
            "fanout row {row} has {total} values for gmeow:{local} (want exactly 1)"
        )));
    }
    if non_literal == 1 {
        return Err(stage_err(&format!(
            "fanout row {row} has a non-literal object for gmeow:{local} (want exactly 1 literal)"
        )));
    }
    Ok(literals
        .into_iter()
        .next()
        .expect("total == 1 and non_literal == 0 implies exactly one literal"))
}

/// An OPTIONAL field: `row gmeow:{local}` may have ZERO or ONE object, and any object
/// present MUST be a literal. More than one object, or a single non-literal object, is
/// still a HARD FAIL (no silent tolerance) — only absence is legitimately optional.
fn optional_literal(
    by_subject: &BTreeMap<String, Vec<purrdf::RdfQuad>>,
    row: &str,
    local: &str,
    gmeow_ns: &str,
) -> Result<Option<String>, gmeow_errors::Diag> {
    let pred = format!("{gmeow_ns}{local}");
    let (literals, non_literal) = collect_field(by_subject, row, &pred);
    let total = literals.len() + non_literal;
    if total == 0 {
        return Ok(None);
    }
    if total > 1 {
        return Err(stage_err(&format!(
            "fanout row {row} has {total} values for gmeow:{local} (want at most 1)"
        )));
    }
    if non_literal == 1 {
        return Err(stage_err(&format!(
            "fanout row {row} has a non-literal object for gmeow:{local} (want a literal)"
        )));
    }
    Ok(literals.into_iter().next())
}

/// Read the `gmeow:fanoutExtracts` rows from the bundle dataset — the path↔representative
/// map, promoted from hard-coded Rust branches to authored data (the pipeline slice). Each
/// row carries `gmeow:extractsPath` + `gmeow:extractsMatch` (+ optional
/// `gmeow:extractsSuffix`), `gmeow:extractsGraphFamily`, and `gmeow:extractsForm`. A
/// malformed row — a missing mandatory field, a duplicate value for ANY field (mandatory
/// or optional), a non-literal (IRI/blank-node/triple-term) object on ANY field, or an
/// unknown match/family/form value — is a HARD FAIL, never silently tolerated
/// (no-optionality): see [`mandatory_literal`] / [`optional_literal`].
///
/// A single pass over `dataset.owned_quads()` groups every subject-IRI quad into a
/// `BTreeMap` keyed by subject, so each field read below is a map lookup rather than a
/// fresh scan of the whole dataset (O(R) total instead of O(R·M) over R rows and M
/// fields). `BTreeMap` (not `HashMap`) keeps subject — and hence row — order
/// deterministic.
fn read_fanout_rules(dataset: &RdfDataset) -> Result<Vec<FanoutRule>, gmeow_errors::Diag> {
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    let mut by_subject: BTreeMap<String, Vec<purrdf::RdfQuad>> = BTreeMap::new();
    for quad in dataset.owned_quads() {
        if let purrdf::RdfTerm::Iri(subj) = &quad.subject {
            by_subject.entry(subj.clone()).or_default().push(quad);
        }
    }

    // Every subject carrying gmeow:extractsPath is a fanout row (cardinality/type of
    // that predicate is enforced below by `mandatory_literal`, not here).
    let path_pred = format!("{GMEOW}extractsPath");
    let rows: BTreeSet<String> = by_subject
        .iter()
        .filter(|(_, quads)| quads.iter().any(|quad| quad.predicate == path_pred))
        .map(|(subj, _)| subj.clone())
        .collect();

    let mut rules = Vec::new();
    for row in &rows {
        let path = mandatory_literal(&by_subject, row, "extractsPath", GMEOW)?;
        let match_kind = mandatory_literal(&by_subject, row, "extractsMatch", GMEOW)?;
        let match_prefix = match match_kind.as_str() {
            "prefix" => true,
            "exact" => false,
            other => {
                return Err(stage_err(&format!(
                    "fanout row {row} has unknown gmeow:extractsMatch {other:?} (want exact|prefix)"
                )));
            }
        };
        let suffix = optional_literal(&by_subject, row, "extractsSuffix", GMEOW)?;
        let family =
            match mandatory_literal(&by_subject, row, "extractsGraphFamily", GMEOW)?.as_str() {
                "rdf-fanout" => FanoutFamily::RdfFanout,
                "edoal" => FanoutFamily::Edoal,
                other => {
                    return Err(stage_err(&format!(
                        "fanout row {row} has unknown gmeow:extractsGraphFamily {other:?}"
                    )));
                }
            };
        let form = match mandatory_literal(&by_subject, row, "extractsForm", GMEOW)?.as_str() {
            "turtle" => GraphForm::Turtle,
            "ntriples" => GraphForm::NTriples,
            "nquads-self" => GraphForm::NQuadsSelf,
            "nquads-diagnostics" => GraphForm::NQuads(GRAPH_DIAGNOSTICS_IRI),
            other => {
                return Err(stage_err(&format!(
                    "fanout row {row} has unknown gmeow:extractsForm {other:?}"
                )));
            }
        };
        rules.push(FanoutRule {
            path,
            match_prefix,
            suffix,
            family,
            form,
        });
    }
    Ok(rules)
}

/// Resolve the named-graph representative for a committed `generated/` path by reading
/// the authored `gmeow:fanoutExtracts` rules (no hard-coded branch). Returns `None` when
/// no rule matches (the path is not carried as a fanout named graph — a byte-decorated /
/// opaque output that rides a blob member instead). The graph IRI derives from the
/// matched rule's family; the form is the rule's declared form, so `file == fold` holds
/// by construction (the producing stage emits with the same form).
fn graph_rep_for_path(rules: &[FanoutRule], path: &str) -> Option<GraphRep> {
    let rule = rules.iter().find(|r| r.matches(path))?;
    let iri = match rule.family {
        FanoutFamily::Edoal => edoal_projection_graph_iri(path)?,
        FanoutFamily::RdfFanout => rdf_fanout_graph_iri(path)?,
    };
    Some(GraphRep {
        iri,
        form: rule.form,
    })
}

/// The completeness HARD-FAIL for the promoted fanout map: assert the
/// `gmeow:fanoutExtracts` rows are a BIJECTION over the bundle's reconstruction graphs —
/// every reconstructed path resolves to exactly one row (no unmapped / ambiguous path),
/// and every row is claimed by at least one path (no stale row). This is the guard that
/// promoting the branches to a data lookup did not silently drop a path from fanout.
fn check_fanout_bijection(
    rules: &[FanoutRule],
    paths: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    // Forward: every reconstructed path matches exactly one rule.
    for path in paths {
        let n = rules.iter().filter(|r| r.matches(path)).count();
        if n != 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "reconstructed path {path} matches {n} gmeow:fanoutExtracts rows (want exactly 1)"
                ),
            }));
        }
    }
    // Reverse: every rule is claimed by at least one reconstructed path (no stale row).
    for rule in rules {
        let n = paths.iter().filter(|p| rule.matches(p)).count();
        if n == 0 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "gmeow:fanoutExtracts row for {:?} ({}) claims no reconstructed path (stale row)",
                    rule.path,
                    if rule.match_prefix { "prefix" } else { "exact" }
                ),
            }));
        }
    }
    Ok(())
}

/// The embedded graph label of the committed diagnostics `.nq` files (mirrors
/// `carrier::GRAPH_DIAGNOSTICS`).
pub(crate) const GRAPH_DIAGNOSTICS_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// Whether a committed RDF `generated/` path is carried as an RDF-fanout named graph
/// (vs. an older dedicated rep). Both the gate (claiming the rep) and the carrier
/// (attaching the graph) consult this single predicate, so the wired set is one
/// authority. Classes are added here as their producing stage starts emitting the
/// committed file as the canonical fold of its attached graph.
pub(crate) fn is_rdf_fanout_class(path: &str) -> bool {
    path.starts_with("generated/profiles/")
        || path.starts_with("generated/research-objects/")
        || path == "generated/evals/scores.ttl"
        || path == "generated/foundation/gufo.ttl"
        // The projection-report loss ledger (no RDF-star). The reasoning closure /
        // explanations / crosscheck are RDF-1.2 with reifiers — their graph-less
        // side-tables do not separate cleanly under a per-file fold, so they ride a
        // dedicated reifier-preserving path (below), not the generic fanout.
        || path == "generated/logic/projection-report.ttl"
        || path == "generated/logic/gmeow.relational-core.nt"
        || path == "generated/logic/gmeow.correspondence.nt"
        // The correspondence-laws projection: the on-disk fold of the bundle's
        // `graph/correspondence-laws` named graph (every authored `logic:Correspondence`
        // re-projected with its EXECUTED lens-law discharge verdicts). RDF travels as RDF,
        // so the discharged `logic:SectionLaw` claims land in `generated/` too, not only in
        // the bundle graph.
        || path == "generated/logic/gmeow.correspondence-laws.nt"
        // The quality-assessment projection: the on-disk fold of the bundle's
        // `graph/quality-assessment` named graph (every slice scored against the rubric as
        // `gmeow:QualityAssessment` observations). RDF travels as RDF, so the assessment
        // triples land in `generated/` too, not only in the bundle graph.
        || path == "generated/quality/gmeow.quality-assessment.nt"
        || path == "generated/diagnostics/shacl.nq"
        || path == "generated/diagnostics/logic-compile.nq"
        // The generated constraint catalog: its committed `.nq` carries the fanout
        // graph IRI itself as its 4th column (unlike the diagnostics `.nq`, which
        // restamp to the shared `graph/diagnostics` label), so its reconstruction
        // restamps to its OWN fanout IRI (see `graph_rep_for_path`).
        || path == "generated/catalog/constraint-catalog.nq"
        // The generated term content manifest: like the constraint catalog, its
        // committed `.nq` carries its OWN fanout graph IRI as the 4th column, so it
        // reconstructs via `GraphForm::NQuadsSelf` (see `graph_rep_for_path`).
        || path == "generated/catalog/term-content-manifest.nq"
        // The non-EDOAL RDF projections; EDOAL keeps its dedicated graph/projections/.
        || path == "generated/projections/core-prefixes.ttl"
        || path == "generated/projections/functions.fno.ttl"
        || path == "generated/projections/list-functions.fno.ttl"
}

/// The named graph IRI for any RDF committed file under `generated/` (other than the
/// EDOAL projections): `graph/fanout/<path-without-the-generated/-prefix>`. The
/// producing stage attaches its graph at this IRI and the gate folds it; the mapping
/// is an identity in both directions.
pub(crate) const RDF_FANOUT_NS: &str = "https://blackcatinformatics.ca/gmeow/graph/fanout/";

/// `Some(graph IRI)` for an RDF committed path (`.ttl`/`.nt`/`.nq`) under
/// `generated/`, else `None` (an opaque output, carried as a blob).
pub(crate) fn rdf_fanout_graph_iri(committed_path: &str) -> Option<String> {
    let rest = committed_path.strip_prefix("generated/")?;
    if !(rest.ends_with(".ttl") || rest.ends_with(".nt") || rest.ends_with(".nq")) {
        return None;
    }
    Some(format!("{RDF_FANOUT_NS}{rest}"))
}

/// The committed path for an RDF-fanout graph IRI — the inverse of
/// [`rdf_fanout_graph_iri`], used by the reverse (orphan) sweep.
pub(crate) fn rdf_fanout_path_for_graph_iri(iri: &str) -> Option<String> {
    iri.strip_prefix(RDF_FANOUT_NS)
        .map(|rest| format!("generated/{rest}"))
}

/// The base IRI of every carrier named graph (mirrors `carrier::GRAPH_*`).
pub(crate) const GRAPH_NS: &str = "https://blackcatinformatics.ca/gmeow/graph/";

/// The named-graph IRI for an EDOAL projection committed at
/// `generated/projections/<name>.edoal.ttl`, or `None` for any other path. The
/// stem (`<name>.edoal`) is the per-file graph segment; the producing stage and
/// this gate compute it identically so the mapping is an identity in both
/// directions. EDOAL renders through the wasm-clean canonical-Turtle serializer,
/// so the fold of its named graph reproduces the committed bytes exactly.
pub(crate) fn edoal_projection_graph_iri(committed_path: &str) -> Option<String> {
    let stem = committed_path
        .strip_prefix("generated/projections/")?
        .strip_suffix(".ttl")?;
    if !stem.ends_with(".edoal") {
        return None;
    }
    Some(format!("{GRAPH_NS}projections/{stem}"))
}

/// The committed EDOAL path for a projection graph IRI — the inverse of
/// [`edoal_projection_graph_iri`], used by the reverse (orphan) sweep.
pub(crate) fn edoal_path_for_graph_iri(iri: &str) -> Option<String> {
    let stem = iri.strip_prefix(&format!("{GRAPH_NS}projections/"))?;
    if !stem.ends_with(".edoal") {
        return None;
    }
    Some(format!("generated/projections/{stem}.ttl"))
}

/// Every distinct RDF-reconstruction graph IRI in the bundle: the per-file EDOAL
/// projection graphs (`…/graph/projections/…`) and the RDF-fanout graphs
/// (`…/graph/fanout/…`). For the reverse orphan sweep.
fn reconstruction_graph_iris(dataset: &RdfDataset) -> BTreeSet<String> {
    let projections = format!("{GRAPH_NS}projections/");
    let mut out = BTreeSet::new();
    for quad in dataset.owned_quads() {
        if let Some(purrdf::RdfTerm::Iri(iri)) = &quad.graph_name
            && (iri.starts_with(&projections) || iri.starts_with(RDF_FANOUT_NS))
        {
            out.insert(iri.clone());
        }
    }
    out
}

/// Run the superset gate over `gts_bytes` (the emitted `gmeow.gts`) against every
/// committed file under `<root>/generated/`.
pub fn check_superset(root: &Path, gts_bytes: &[u8]) -> Result<SupersetReport, gmeow_errors::Diag> {
    // The single reconstruction authority: every committed path the bundle carries,
    // reconstructed from the shipped bytes alone. The gate compares it to disk; the
    // fanout phase writes it. One code path, no second reconstruction.
    let projection = project_bundle(gts_bytes)?;
    sweep_against_committed(&projection, root)
}

/// Sweep a reconstructed [`BundleProjection`] against the committed `generated/` tree
/// under `root`: forward (missing / mismatch) and reverse (orphan). A pure function of
/// the projection and the on-disk tree — no bundle parsing — so the sweep verdicts are
/// unit-testable with an injected projection.
fn sweep_against_committed(
    projection: &BundleProjection,
    root: &Path,
) -> Result<SupersetReport, gmeow_errors::Diag> {
    let committed = committed_generated_paths(root)?;
    let committed_set: BTreeSet<&str> = committed.iter().map(String::as_str).collect();

    let mut missing = Vec::new();
    let mut mismatch = Vec::new();

    // ── Forward sweep: every committed path must reconstruct from the bundle. ──
    for path in &committed {
        if EXCLUDED.contains(&path.as_str()) {
            continue;
        }
        match projection.files.get(path) {
            None => missing.push(path.clone()),
            Some(reconstructed) => {
                let committed_bytes = std::fs::read(root.join(path))
                    .map_err(|e| stage_err(&format!("read committed {path}: {e}")))?;
                if *reconstructed != committed_bytes {
                    mismatch.push(path.clone());
                }
            }
        }
    }

    // ── Reverse sweep: every reconstructed `generated/` path must back a committed
    // file. The bundle is a *superset* of `generated/` (§5): it also carries source
    // archives (`dsl/`, `slices/` shapes/cells/tests) and the rendered docs site for
    // self-sufficiency — but `project_bundle` already filtered those out (it emits
    // only `generated/`-targeting reps). An orphan is thus a STALE reconstruction rep:
    // a carried `generated/` path with no committed file. ──
    let mut orphan = Vec::new();
    for path in projection.files.keys() {
        if !committed_set.contains(path.as_str()) {
            orphan.push(path.clone());
        }
    }

    missing.sort();
    mismatch.sort();
    orphan.sort();
    Ok(SupersetReport {
        missing,
        mismatch,
        orphan,
    })
}

/// Reconstruct one named graph's committed bytes from the bundle dataset, or
/// `None` if the graph carries no quads (no representative present). The fold is
/// the canonical-Turtle render of the projected graph (no graph label), the same
/// serializer the producing stage emits the committed file with.
fn reconstruct_graph(dataset: &RdfDataset, rep: &GraphRep) -> Option<Vec<u8>> {
    let projected = dataset.project_named_graph(&rep.iri);
    if projected.quad_count() == 0 {
        return None;
    }
    match rep.form {
        GraphForm::Turtle => {
            Some(purrdf::turtle_normalize::render(&projected, &rdf_prefixes()).into_bytes())
        }
        GraphForm::NTriples => canonical_ntriples(&projected).ok(),
        GraphForm::NQuads(label) => {
            // `project_named_graph` drops the graph label; restamp to the embedded
            // label so the RDFC-canonical N-Quads 4th column matches the committed file.
            let rooted = crate::stages::carrier::rooted_in_graph(&projected, label).ok()?;
            canonical_ntriples(&rooted).ok()
        }
        GraphForm::NQuadsSelf => {
            // The committed 4th column is the fanout IRI itself — restamp back to it.
            let rooted = crate::stages::carrier::rooted_in_graph(&projected, &rep.iri).ok()?;
            canonical_ntriples(&rooted).ok()
        }
    }
}

/// The project's single prefix authority for the canonical Turtle renderer — shared
/// by the gate (folding a carried named graph) and every producing stage that emits
/// an RDF file as `canonical_turtle(body, rdf_prefixes())`, so `file == fold` holds
/// by construction (identical prefix selection on both legs).
pub(crate) fn rdf_prefixes() -> Vec<(String, String)> {
    gmeow_logic_compile::ingest::prefixes::registry_pairs()
}

/// The RDFC-1.0 canonical N-Quads document for `dataset` (blank labels canonicalized,
/// lines bytewise-sorted). A default-graph dataset folds to N-Triples lines; a
/// graph-labelled dataset folds to N-Quads lines. Shared by the gate (folding a
/// `.nt`/`.nq` graph) and the producing stage (emitting the committed file), so
/// `file == fold` holds by construction — idempotent even with blank nodes.
pub(crate) fn canonical_ntriples(dataset: &RdfDataset) -> gmeow_errors::Result<Vec<u8>> {
    // Native RDFC-1.0 over the FLATTENED carrier: the statement overlay is
    // re-materialized to plain `rdf:reifies`/annotation triples before canonicalizing.
    // Format-adaptive: a default-graph dataset yields N-Triples lines, a graph-labelled
    // one N-Quads — byte-identical to the prior oxigraph-flat path.
    purrdf::canonical_flat_nquads(dataset)
        .map(String::into_bytes)
        .map_err(|e| stage_err(&format!("RDFC-1.0 canonicalize: {e}")))
}

/// Parse the emitted bundle back into a native dataset (closes serialize -> parse).
fn read_dataset(gts_bytes: &[u8]) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    let bundle = purrdf::import_gts_events(gts_bytes)
        .map_err(|e| stage_err(&format!("re-import gmeow.gts: {e}")))?;
    Ok(bundle.dataset)
}

/// Unpack every inline archive blob into a single `committed-path -> bytes` map. The
/// blob payloads are read from the GTS fold (digest -> bytes) joined with the
/// blob lookaside (digest -> representation); each archive is a deterministic
/// USTAR whose members are the committed files. Each member is keyed by its full
/// committed repo-relative path via
/// [`crate::stages::carrier::committed_path_for_archive_member`] (the inverse of the
/// rep's member-naming convention), so the caller resolves a member to its
/// `generated/` path with no basename guessing.
fn read_blob_members(gts_bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let graph = purrdf::gts::read_graph(gts_bytes, true)
        .map_err(|e| stage_err(&format!("read gmeow.gts blobs: {e}")))?;
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for record in &lookaside.blobs {
        // Only archive blobs unpack to member files; non-archive blobs (reports,
        // guides, docs) are not committed `generated/` reconstruction targets and
        // are skipped — their committed projections, if any, ride other reps.
        if record.media_type.as_deref() != Some("application/x-tar") {
            continue;
        }
        // Decode ONLY the archives that can carry a committed `generated/` file. The
        // source archives (cells/tests) and the large docs/okf payloads back no
        // `generated/` path — and the docs/okf archives are large enough to trip the
        // zstd decode safety bound, so decoding them would be both wasteful and fatal.
        let rep = record.representation.as_deref().unwrap_or_default();
        if !crate::stages::carrier::archive_rep_carries_generated(rep) {
            continue;
        }
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == &record.digest) else {
            continue;
        };
        let bytes = entry
            .decoded_vec()
            .map_err(|e| stage_err(&format!("decode blob {}: {e:?}", record.digest)))?;
        for (name, member_bytes) in purrdf::ustar::read_archive(&bytes)
            .map_err(|e| stage_err(&format!("unpack archive {}: {e}", record.digest)))?
        {
            let Some(committed) =
                crate::stages::carrier::committed_path_for_archive_member(rep, &name)
            else {
                // A rep that passed `archive_rep_carries_generated` but resolves no
                // committed path is a wiring contradiction — fail closed, never drop.
                return Err(stage_err(&format!(
                    "archive rep {rep} carries member {name} with no committed-path mapping"
                )));
            };
            out.insert(committed, member_bytes);
        }
    }
    Ok(out)
}

/// Every committed file under `<root>/generated/`, repo-relative (`generated/...`),
/// sorted. Walks the tree directly (the gate enumerates the on-disk committed set,
/// not a stage product).
fn committed_generated_paths(root: &Path) -> Result<Vec<String>, gmeow_errors::Diag> {
    // GENERATED-READ-OK: the superset gate VERIFIES the shipped bundle is a superset of the
    // committed generated/ tree, so it must enumerate that on-disk tree — a verification read,
    // not a produce-stage fold of a stale projection. Nothing here folds into gmeow.gts; this
    // read is the oracle the gate checks the freshly-projected bundle against.
    let base = root.join("generated");
    let mut out = Vec::new();
    walk(&base, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<(), gmeow_errors::Diag> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| stage_err(&format!("read dir {dir:?}: {e}")))?;
    for entry in entries {
        let entry = entry.map_err(|e| stage_err(&format!("dir entry in {dir:?}: {e}")))?;
        let path = entry.path();
        // Skip hidden (dot) directories: they are runtime, never committed — e.g.
        // `generated/.pipeline-cache/` (gitignored persistent stage cache). The gate
        // reconstructs only committed artifacts.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|e| stage_err(&format!("file type {path:?}: {e}")))?;
        if file_type.is_dir() {
            walk(&path, root, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| stage_err(&format!("strip prefix {path:?}: {e}")))?;
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-superset-gate".to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdfstar_closure_folds_byte_identically() {
        // The reasoning closure is RDF-1.2 with thousands of ANONYMOUS reifiers.
        // With the parse (anon-reifier collapse), `rdf:reifies` interning, render
        // (side-table emission) and content-stable Triple-signature fixes, a per-file
        // carrier fold must reproduce the canonical bytes exactly.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let committed =
            std::fs::read(root.join("generated/logic/inferred-closure.rdf12.ttl")).unwrap();
        let prefixes = rdf_prefixes();
        // canonical_turtle must be idempotent on RDF-1.2 reifiers.
        let c1 = purrdf::turtle_normalize::canonical_turtle(&committed, &prefixes).unwrap();
        let c2 = purrdf::turtle_normalize::canonical_turtle(c1.as_bytes(), &prefixes).unwrap();
        assert_eq!(
            c1, c2,
            "canonical_turtle must be idempotent on the RDF-1.2 closure"
        );
        // Full carrier fold: attach in a named graph, project (keeping reifiers by
        // reified-statement subject), render — must reproduce the canonical bytes.
        let ds = purrdf::parse_dataset(c1.as_bytes(), "text/turtle", None).unwrap();
        assert!(
            ds.annotations().count() > 0,
            "anonymous-reifier annotations must fold (not become base quads)"
        );
        let iri =
            "https://blackcatinformatics.ca/gmeow/graph/fanout/logic/inferred-closure.rdf12.ttl";
        let rooted = crate::stages::carrier::rooted_in_graph(&ds, iri).unwrap();
        let folded =
            purrdf::turtle_normalize::render(&rooted.project_named_graph_full(iri), &prefixes);
        assert_eq!(
            folded, c1,
            "the RDF-star carrier fold must reproduce the canonical bytes"
        );
    }

    #[test]
    fn excluded_holds_exactly_the_two_terminal_bundles() {
        assert_eq!(EXCLUDED.len(), 2);
        assert!(EXCLUDED.contains(&"generated/dist/gmeow.gts"));
        assert!(EXCLUDED.contains(&"generated/dist/gmeow-full.gts"));
    }

    #[test]
    fn archive_member_committed_path_restores_directory_for_basename_reps() {
        use crate::stages::carrier::committed_path_for_archive_member;
        // Basename-keyed reps get their directory prefix restored.
        assert_eq!(
            committed_path_for_archive_member("mappings-archive", "foaf.sssom.tsv").as_deref(),
            Some("generated/mappings/foaf.sssom.tsv")
        );
        assert_eq!(
            committed_path_for_archive_member("queries-archive", "bare.rq").as_deref(),
            Some("generated/queries/bare.rq")
        );
        assert_eq!(
            committed_path_for_archive_member("schemas-archive", "gmeow.schema.json").as_deref(),
            Some("generated/schemas/gmeow.schema.json")
        );
        // Repo-relative reps pass through unchanged.
        assert_eq!(
            committed_path_for_archive_member("generated-opaque-archive", "generated/n3/gmeow.n3")
                .as_deref(),
            Some("generated/n3/gmeow.n3")
        );
        assert_eq!(
            committed_path_for_archive_member("axioms-archive", "generated/owl/gmeow-dl.ttl")
                .as_deref(),
            Some("generated/owl/gmeow-dl.ttl")
        );
        // A non-generated rep resolves nothing.
        assert_eq!(
            committed_path_for_archive_member("cells-archive", "dsl/mappings/x.ttl"),
            None
        );
    }

    /// The authored `gmeow:fanoutExtracts` rows, read from the pipeline slice module.ttl
    /// (the same data the shipped bundle carries) — the real map the gate reads.
    fn authored_fanout_rules() -> Vec<FanoutRule> {
        let root = repo_root();
        let ttl = std::fs::read(root.join("slices/core/pipeline/module.ttl")).unwrap();
        let ds = purrdf::parse_dataset(&ttl, "text/turtle", None).unwrap();
        read_fanout_rules(&ds).unwrap()
    }

    #[test]
    fn byte_decorated_rdf_paths_fall_through_to_blob_members() {
        let rules = authored_fanout_rules();
        for path in [
            "generated/logic/inferred-closure.rdf12.ttl",
            "generated/logic/reasoning-explanations.rdf12.ttl",
            "generated/logic/dl-el-crosscheck-report.ttl",
            "generated/logic/perf-ledger.ttl",
            "generated/metadata/void.ttl",
            "generated/metadata/dcat.ttl",
            "generated/statements/gmeow-statements.owl.ttl",
            "generated/statements/gmeow.rdf12.ttl",
        ] {
            assert!(
                graph_rep_for_path(&rules, path).is_none(),
                "{path} has generated comments / section markers and must reconstruct from REP_GENERATED"
            );
        }
    }

    #[test]
    fn reconstruct_graph_folds_turtle_without_the_graph_label() {
        use purrdf::RdfDatasetBuilder;

        const G: &str = "https://blackcatinformatics.ca/gmeow/graph/projections/sample.edoal";
        const S: &str = "https://blackcatinformatics.ca/gmeow/projections/sample";
        const P: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const O: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Alignment";

        let mut b = RdfDatasetBuilder::new();
        let g = b.intern_iri(G);
        let s = b.intern_iri(S);
        let p = b.intern_iri(P);
        let o = b.intern_iri(O);
        b.push_quad(s, p, o, Some(g));
        let dataset = b.freeze().expect("freeze");

        let turtle = reconstruct_graph(
            &dataset,
            &GraphRep {
                iri: G.to_string(),
                form: GraphForm::Turtle,
            },
        )
        .expect("turtle reconstruction");
        let turtle = String::from_utf8(turtle).expect("utf8");
        assert!(turtle.contains("align:Alignment") || turtle.contains(O));
        assert!(
            !turtle.contains(G),
            "turtle fold must not carry the graph label"
        );

        // A graph IRI with no quads yields no representative.
        assert!(
            reconstruct_graph(
                &dataset,
                &GraphRep {
                    iri: "https://blackcatinformatics.ca/gmeow/graph/absent".to_string(),
                    form: GraphForm::Turtle,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn quality_assessment_nt_folds_as_ntriples_via_its_own_fanout_graph() {
        // The G2 quality-assessment `.nt` is a registered RDF-fanout class that folds as
        // plain N-Triples (default graph, no label) — the form the fanout writer emits and
        // the superset gate reconstructs, so `file == fold` holds by construction.
        const PATH: &str = "generated/quality/gmeow.quality-assessment.nt";
        assert!(is_rdf_fanout_class(PATH));
        let rules = authored_fanout_rules();
        let rep =
            graph_rep_for_path(&rules, PATH).expect("quality-assessment path resolves a graph rep");
        assert_eq!(rep.form, GraphForm::NTriples);
        assert_eq!(
            rep.iri,
            "https://blackcatinformatics.ca/gmeow/graph/fanout/quality/gmeow.quality-assessment.nt"
        );
    }

    #[test]
    fn edoal_graph_iri_convention_is_identity_in_both_directions() {
        assert_eq!(
            edoal_projection_graph_iri("generated/projections/foaf.edoal.ttl").as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/graph/projections/foaf.edoal")
        );
        // Non-EDOAL projections (template-emitted) are NOT named-graph carried yet.
        assert!(edoal_projection_graph_iri("generated/projections/core-prefixes.ttl").is_none());
        assert!(edoal_projection_graph_iri("generated/projections/functions.fno.ttl").is_none());
        assert!(edoal_projection_graph_iri("generated/mappings/foaf.sssom.tsv").is_none());
    }

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn project_bundle_reconstructs_the_committed_tree_and_gate_is_clean() {
        // Single-authority proof: project_bundle reconstructs every committed
        // generated/ file from the shipped gmeow.gts alone, and the refactored
        // forward+reverse sweep is clean against the committed tree.
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).unwrap();
        let proj = project_bundle(&gts).unwrap();
        assert!(
            proj.files.len() > 50,
            "projection unexpectedly small ({}); reconstruction likely dropped reps",
            proj.files.len()
        );
        // A byte-decorated RDF file rides a blob member; a plain RDF file a named-graph
        // fold — both must be present in the one projection.
        assert!(
            proj.files
                .contains_key("generated/logic/inferred-closure.rdf12.ttl"),
            "byte-decorated closure must reconstruct from a blob member"
        );
        assert!(
            proj.files
                .keys()
                .any(|p| p.starts_with("generated/profiles/")),
            "a profiles/*.ttl named-graph fold must reconstruct"
        );
        // Every reconstructed path is under generated/ (source archives filtered out).
        for path in proj.files.keys() {
            assert!(
                path.starts_with("generated/"),
                "projection leaked a non-generated path: {path}"
            );
        }
        let report = check_superset(&root, &gts).unwrap();
        assert!(
            report.is_clean(),
            "superset gate not clean after the seam refactor: {report:?}"
        );
    }

    #[test]
    fn sweep_detects_missing_mismatch_and_orphan() {
        use std::io::Write;
        // A temp committed tree: two files under generated/.
        let dir = std::env::temp_dir().join(format!("gmeow-superset-sweep-{}", std::process::id()));
        let gen_dir = dir.join("generated/x");
        std::fs::create_dir_all(&gen_dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let mut f = std::fs::File::create(gen_dir.join(name)).unwrap();
            f.write_all(bytes).unwrap();
        };
        write("kept.ttl", b"KEEP");
        write("drift.ttl", b"DISK-BYTES");
        write("absent.ttl", b"NO-REP");

        // A projection that: matches kept, drifts on drift, has NO rep for absent
        // (missing), and carries a stale extra path (orphan).
        let mut files = BTreeMap::new();
        files.insert("generated/x/kept.ttl".to_string(), b"KEEP".to_vec());
        files.insert(
            "generated/x/drift.ttl".to_string(),
            b"BUNDLE-BYTES".to_vec(),
        );
        files.insert("generated/x/stale.ttl".to_string(), b"ORPHAN".to_vec());
        let projection = BundleProjection { files };

        let report = sweep_against_committed(&projection, &dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.missing, vec!["generated/x/absent.ttl".to_string()]);
        assert_eq!(report.mismatch, vec!["generated/x/drift.ttl".to_string()]);
        assert_eq!(report.orphan, vec!["generated/x/stale.ttl".to_string()]);
        assert!(!report.is_clean());
    }

    #[test]
    fn fanout_rules_drive_reconstruction_and_bijection() {
        // The path↔representative map read as DATA (the promoted gmeow:fanoutExtracts rows):
        // parse a small row set, prove the form/family/graph-IRI resolution is data-driven,
        // and prove the bijection HARD-fail fires on an unmapped path AND on a stale row.
        let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:r2 gmeow:extractsPath "generated/logic/gmeow.correspondence.nt" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "ntriples" .
gmeow:r3 gmeow:extractsPath "generated/diagnostics/shacl.nq" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "nquads-diagnostics" .
gmeow:r4 gmeow:extractsPath "generated/profiles/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:r5 gmeow:extractsPath "generated/projections/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsSuffix ".edoal.ttl" ; gmeow:extractsGraphFamily "edoal" ; gmeow:extractsForm "turtle" .
"#;
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        let rules = read_fanout_rules(&ds).unwrap();
        assert_eq!(rules.len(), 5);

        // Form + graph-IRI resolution is driven entirely by the data rows.
        let rep = graph_rep_for_path(&rules, "generated/evals/scores.ttl").unwrap();
        assert_eq!(rep.form, GraphForm::Turtle);
        assert_eq!(
            rep.iri,
            "https://blackcatinformatics.ca/gmeow/graph/fanout/evals/scores.ttl"
        );
        assert_eq!(
            graph_rep_for_path(&rules, "generated/logic/gmeow.correspondence.nt")
                .unwrap()
                .form,
            GraphForm::NTriples
        );
        assert_eq!(
            graph_rep_for_path(&rules, "generated/diagnostics/shacl.nq")
                .unwrap()
                .form,
            GraphForm::NQuads(GRAPH_DIAGNOSTICS_IRI)
        );
        // The EDOAL prefix+suffix rule resolves the edoal graph family.
        assert_eq!(
            graph_rep_for_path(&rules, "generated/projections/foaf.edoal.ttl")
                .unwrap()
                .iri,
            "https://blackcatinformatics.ca/gmeow/graph/projections/foaf.edoal"
        );
        // A profiles/ file rides the rdf-fanout prefix rule.
        assert_eq!(
            graph_rep_for_path(&rules, "generated/profiles/full.ttl")
                .unwrap()
                .iri,
            "https://blackcatinformatics.ca/gmeow/graph/fanout/profiles/full.ttl"
        );
        // A non-EDOAL projection under the same directory does NOT match the suffix-filtered
        // edoal rule (and no other rule claims it) — no representative.
        assert!(graph_rep_for_path(&rules, "generated/projections/core-prefixes.ttl").is_none());

        // Bijection holds for a path set each rule claims exactly.
        let paths: BTreeSet<String> = [
            "generated/evals/scores.ttl",
            "generated/logic/gmeow.correspondence.nt",
            "generated/diagnostics/shacl.nq",
            "generated/profiles/full.ttl",
            "generated/projections/foaf.edoal.ttl",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        check_fanout_bijection(&rules, &paths).expect("bijection holds over the covered paths");

        // Deliberately-missing row: a reconstructed path no rule matches HARD-fails.
        let mut unmapped = paths.clone();
        unmapped.insert("generated/quality/gmeow.quality-assessment.nt".to_string());
        let err = check_fanout_bijection(&rules, &unmapped).unwrap_err();
        assert_eq!(err.code(), crate::error::FanoutBijection::register());

        // Stale row: a rule matching no reconstructed path HARD-fails (drop r2's path).
        let mut stale = paths.clone();
        stale.remove("generated/logic/gmeow.correspondence.nt");
        let err2 = check_fanout_bijection(&rules, &stale).unwrap_err();
        assert_eq!(err2.code(), crate::error::FanoutBijection::register());
    }

    #[test]
    fn clean_report_requires_all_three_sweeps_empty() {
        let clean = SupersetReport {
            missing: vec![],
            mismatch: vec![],
            orphan: vec![],
        };
        assert!(clean.is_clean());
        let dirty = SupersetReport {
            missing: vec!["x".into()],
            mismatch: vec![],
            orphan: vec![],
        };
        assert!(!dirty.is_clean());
    }
}
