// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The superset gate: `gmeow.gts` is a superset of `generated/`.
//!
//! The **authority is the projection** — the set of committed `generated/` paths the
//! shipped bundle reconstructs ([`project_bundle`]). The gate proves that projection
//! equals the materialized `generated/` tree on disk, in both directions:
//!
//! * an RDF output with a canonical graph fold is reconstructed from one named graph
//!   (Turtle via the wasm-clean renderer; N-Quads via a graph-rooted serialization),
//! * a byte-decorated output (including generated RDF reports whose committed files
//!   contain comments / section markers) is a member of one inline content-addressed
//!   archive blob, and
//! * a trained zstd dictionary is reconstructed from the segment header's in-band
//!   `"dct"` map — its ONE canonical home. Routing the `.zdict` bytes through the
//!   generated-opaque archive instead would carry them twice (Constitution §18
//!   forbids re-folding a blob the snapshot already carries) and would feed
//!   high-entropy bytes to a compressor, inflating that archive.
//!
//! The forward sweep drives off the projection: every projection key MUST exist on
//! disk with byte-matching content (`missing` = a projection key with no file on
//! disk; `mismatch` = present but drifted). The reverse sweep drives off the disk:
//! every materialized `generated/` file MUST be a projection key (`orphan` = an
//! undeclared / stale on-disk file). Because the projection is the must-exist set,
//! an empty or absent `generated/` tree can never pass vacuously — every projection
//! key becomes `missing`. Any non-empty sweep is a hard failure — no skips, no
//! optional coverage, no degraded pass.
//!
//! Reconstruction reads the bundle back through [`purrdf::import_gts_events`]
//! and the GTS blob reader, closing the serialize -> parse loop, so it proves
//! byte-reconstructibility from the emitted bundle rather than from in-memory
//! carrier state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use purrdf::RdfDataset;

/// The one terminal bundle that cannot byte-contain itself: it is the only
/// on-disk `generated/` path the reverse (orphan) sweep excludes, because a bundle
/// is not a projection of itself and so is never a projection key.
pub const EXCLUDED: [&str; 1] = ["generated/dist/gmeow.gts"];

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

/// One verified decode of the two bundle surfaces the projection proof needs.
///
/// The RDF dataset deliberately comes from the authoritative segment-aware event
/// importer, while the folded GTS graph retains the raw blob payloads and metadata
/// that are outside RDF. Keeping both in one opaque value prevents callers from
/// accidentally pairing a dataset and blob graph from different bundle bytes, and
/// lets whole-bundle proofs reuse the expensive decode/index work across assertions.
pub struct DecodedProjectionSource {
    dataset: Arc<RdfDataset>,
    graph: purrdf::gts::model::Graph,
    lookaside: purrdf::RdfLookaside,
    header_dicts: BTreeMap<String, Vec<u8>>,
}

impl DecodedProjectionSource {
    /// The graph-preserving RDF dataset produced by the authoritative GTS event importer.
    #[must_use]
    pub fn dataset(&self) -> &RdfDataset {
        self.dataset.as_ref()
    }

    /// The raw folded GTS graph, including inline blob payloads.
    #[must_use]
    pub fn graph(&self) -> &purrdf::gts::model::Graph {
        &self.graph
    }

    /// The blob/header companion index derived from [`Self::graph`].
    #[must_use]
    pub fn lookaside(&self) -> &purrdf::RdfLookaside {
        &self.lookaside
    }
}

/// Decode the RDF, blob and header surfaces of one emitted bundle exactly once.
///
/// This is intentionally not a weaker graph-only import: the segment-aware event
/// importer remains the RDF authority, and the raw reader remains the independent
/// authority for blob/header semantics. Every error is a hard failure.
pub fn decode_projection_source(
    gts_bytes: &[u8],
) -> Result<DecodedProjectionSource, gmeow_errors::Diag> {
    let dataset = read_dataset(gts_bytes)?;
    let graph = purrdf::gts::read_graph(gts_bytes, true)
        .map_err(|e| stage_err(&format!("read gmeow.gts blobs: {e}")))?;
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);
    let header_dicts = read_header_dicts(gts_bytes)?;
    Ok(DecodedProjectionSource {
        dataset,
        graph,
        lookaside,
        header_dicts,
    })
}

/// Reconstruct every committed `generated/` file the bundle carries, keyed by its
/// committed repo-relative path (PIPELINE_SPINE §5/§6). Drives off the *bundle's*
/// representatives, never the on-disk tree, so it reconstructs from `gmeow.gts`
/// alone — the property the fanout phase depends on. Two rep classes:
///
/// * **named-graph folds** — each EDOAL projection graph (`…/graph/projections/…`)
///   and RDF-fanout graph (`…/graph/fanout/…`) folds to its committed RDF bytes via
///   [`reconstruct_graph`];
/// * **inline blob members** — every archive member resolved to its committed
///   `generated/` path by [`read_blob_members`]; and
/// * **header dictionaries** — every entry of the segment header's in-band `"dct"`
///   map, resolved to `generated/medium/<dict-id>.zdict` by [`read_header_dicts`].
///
/// The three rep classes are disjoint by construction (RDF travels as a named graph,
/// opaque/byte-decorated output as a blob member, a trained zstd dictionary as a
/// header `"dct"` entry), so no path is produced twice.
pub fn project_bundle(gts_bytes: &[u8]) -> Result<BundleProjection, gmeow_errors::Diag> {
    let source = decode_projection_source(gts_bytes)?;
    project_decoded_bundle(&source)
}

/// Reconstruct a bundle already decoded by [`decode_projection_source`].
///
/// This performs the complete projection, fanout-bijection and independently
/// authored expected-output checks. It only avoids re-reading the identical bytes;
/// no assertion or representation family is skipped.
pub fn project_decoded_bundle(
    source: &DecodedProjectionSource,
) -> Result<BundleProjection, gmeow_errors::Diag> {
    let dataset = source.dataset();
    let blob_members = read_blob_members(source.graph(), source.lookaside())?;
    // The path↔representative map as DATA: the gmeow:fanoutExtracts rows read back from the
    // bundle. The RDF-fanout / EDOAL rows are AUTHORED (pipeline slice, default graph); the
    // opaque rows are EMITTED by the carrier (one per generated-opaque archive member,
    // riding the meta-level fanout-manifest graph). The gate reads them as data instead of
    // branching in Rust (PIPELINE_SPINE §6/§7).
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
    for (path, bytes) in blob_members.files {
        if !path.starts_with("generated/") {
            continue;
        }
        if files.insert(path.clone(), bytes).is_some() {
            return Err(stage_err(&format!(
                "{path} is carried by two representatives (blob member collides with a named graph)"
            )));
        }
    }

    // Header-dict reps: every entry of the segment header's in-band `"dct"` map, keyed
    // by its committed `generated/medium/<dict-id>.zdict` path. The header is the
    // dictionary's ONE home — it is where a consumer priming its own store reads the
    // bytes from — so the projection reads them from there rather than carrying a
    // second copy through the archive lane.
    let mut header_dict_paths: BTreeSet<String> = BTreeSet::new();
    for (path, bytes) in &source.header_dicts {
        header_dict_paths.insert(path.clone());
        if files.insert(path.clone(), bytes.clone()).is_some() {
            return Err(stage_err(&format!(
                "{path} is carried by two representatives (header \"dct\" entry collides)"
            )));
        }
    }

    // Family-scope the three bijections so an opaque row can never claim a named-graph
    // path (or a header dictionary) and vice versa: the RDF-fanout / EDOAL rows are
    // proved against the reconstruction graphs, the opaque rows against the
    // generated-opaque archive members, and the header-dict rows against the header's
    // own `"dct"` map.
    let (header_dict_rules, rules): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
        .into_iter()
        .partition(|r| r.family == FanoutFamily::HeaderDict);
    let (opaque_rules, named_rules): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
        .into_iter()
        .partition(|r| r.family == FanoutFamily::Opaque);

    // Bijection completeness HARD-FAIL (named graphs): the authored gmeow:fanoutExtracts
    // rows must be a bijection over the reconstruction graphs — no path unmapped/ambiguous,
    // no stale row — so reading the map from data never silently drops a path from fanout.
    check_fanout_bijection(&named_rules, &reconstructed_paths)?;

    // Bijection completeness HARD-FAIL (opaque archive): every generated-opaque archive
    // member resolves to exactly one emitted opaque row and every opaque row is claimed by
    // exactly one member — so the opaque byte lane is a DECLARED, bijection-checked
    // inventory, not a silent hidden set the superset gate never sees.
    check_opaque_bijection(&opaque_rules, &blob_members.opaque_paths)?;

    // Bijection completeness HARD-FAIL (header dictionaries): every `"dct"` entry the
    // shipped segment header pins resolves to exactly one authored header-dict row and
    // every header-dict row is claimed by exactly one entry. Family-scoped for the same
    // reason the opaque bijection is: a dictionary row must never be able to satisfy a
    // named-graph path (or an archive member) it does not carry.
    check_header_dict_bijection(&header_dict_rules, &header_dict_paths)?;

    // The dictionary bytes are carried EXACTLY ONCE: a `generated/medium/*.zdict` path
    // that ALSO rode the generated-opaque archive would ship the same high-entropy bytes
    // twice (Constitution §18) and inflate that archive. The collision is impossible to
    // reach through `files` (the insert above would have hard-failed), so it is proved
    // here against the archive's own member set instead.
    for path in &blob_members.opaque_paths {
        if is_header_dict_path(path) {
            return Err(stage_err(&format!(
                "{path} rides the generated-opaque archive as well as the segment header's \
                 \"dct\" map — a trained dictionary's ONE home is the header, so carrying it \
                 twice re-folds a blob the snapshot already carries"
            )));
        }
    }

    // ── Independent completeness anchor (PIPELINE_SPINE §6/§7). ──
    // The expected-output inventory is AUTHORED TTL (the pipeline slice's
    // `gmeow:expectsGeneratedOutput` rows), read back from the bundle here — a DIFFERENT
    // source from the carrier's `files.keys()` (which the fanout rows are self-consistent
    // with). A carrier code change that stops emitting a declared output shrinks `files`
    // but NOT the authored inventory, so the ⊇ HARD FAIL below fires exactly where the
    // two-generation determinism gate is blind (a deterministic drop is byte-identical
    // across both runs). Running it inside `project_bundle` makes BOTH the fanout (Update)
    // and superset-gate (Check) paths enforce it.
    let expected = read_expected_outputs(dataset)?;
    check_expected_completeness(&files, &expected)?;
    // Prefix-family robustness: the two families whose members are cleanly DERIVABLE at the
    // gate (from the carrier's reconstruction-graph IRIs, independent of the authored TTL)
    // must have their authored members EXACTLY equal the derived members — so adding or
    // dropping a producing individual without its expected path HARD-fails.
    check_derivable_families(&expected, &reconstructed_paths, &header_dict_paths)?;

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
    /// No named-graph fold: the committed path rides as a byte-exact member of the
    /// inline generated-opaque archive blob (the `"opaque"` family). Never produces a
    /// [`GraphRep`] — [`graph_rep_for_path`] skips opaque rules — so it is never handed
    /// to [`reconstruct_graph`]; the enum arm exists only so [`read_fanout_rules`] can
    /// round-trip the `"blob"` form string of an opaque row.
    Blob,
    /// No named-graph fold: the committed path is the verbatim bytes of one entry of
    /// the GTS segment header's in-band `"dct"` map (the `"header-dict"` family).
    /// Never produces a [`GraphRep`] — [`graph_rep_for_path`] skips header-dict rules
    /// exactly as it skips opaque ones — so it is never handed to
    /// [`reconstruct_graph`]; the enum arm exists only so [`read_fanout_rules`] can
    /// round-trip the `"header-dict"` form string of a dictionary row.
    HeaderDict,
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
    /// No reconstruction graph: the committed path is a byte-exact member of the inline
    /// generated-opaque archive blob (`REP_GENERATED`). These rows are EMITTED by the
    /// carrier (one per archive member), never authored, and are bijection-checked
    /// against the blob members by [`check_opaque_bijection`] — family-scoped so they
    /// never cross-contaminate the RDF-fanout / EDOAL named-graph bijection.
    Opaque,
    /// No reconstruction graph and no archive member: the committed path
    /// `generated/medium/<dict-id>.zdict` is the verbatim bytes of the `<dict-id>`
    /// entry of the shipped segment header's in-band `"dct"` map (GTS spec §5), which
    /// is a trained zstd dictionary's ONE canonical home. These rows are AUTHORED (the
    /// declared dictionary set is stable, hand-written data — unlike the carrier-emitted
    /// opaque rows) and are bijection-checked against the header's own `"dct"` map by
    /// [`check_header_dict_bijection`] — family-scoped so they never cross-contaminate
    /// the named-graph or opaque-archive bijections.
    HeaderDict,
}

/// One parsed `gmeow:FanoutExtraction` row — the DATA the superset gate reads in place
/// of the former hard-coded path↔representative branches (`graph_rep_for_path` /
/// `is_rdf_fanout_class` form selection).
#[derive(Debug, Clone)]
pub(crate) struct FanoutRule {
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
    /// Whether this is an `opaque`-family row (a byte-exact archive member, not a
    /// named-graph fold). Crate-visible so the carrier's emit-side tests can assert the
    /// gate's OWN reader recovered the opaque rows it emitted.
    #[cfg(test)]
    pub(crate) fn is_opaque(&self) -> bool {
        self.family == FanoutFamily::Opaque
    }

    /// The committed path (exact) or directory prefix this rule matches.
    #[cfg(test)]
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

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
pub(crate) fn read_fanout_rules(
    dataset: &RdfDataset,
) -> Result<Vec<FanoutRule>, gmeow_errors::Diag> {
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
                "opaque" => FanoutFamily::Opaque,
                "header-dict" => FanoutFamily::HeaderDict,
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
            "blob" => GraphForm::Blob,
            "header-dict" => GraphForm::HeaderDict,
            other => {
                return Err(stage_err(&format!(
                    "fanout row {row} has unknown gmeow:extractsForm {other:?}"
                )));
            }
        };
        // The `opaque`/`blob` pairing is total: an opaque family MUST carry the blob form
        // (and an exact match — an opaque member is one byte-exact archive entry, never a
        // prefix family) and the blob form MUST be opaque. Any other pairing is a
        // malformed row — HARD FAIL (no-optionality), so a hand-authored opaque row that
        // forgets the pairing never silently degrades to a named-graph fold.
        let is_opaque = family == FanoutFamily::Opaque;
        let is_blob = form == GraphForm::Blob;
        if is_opaque != is_blob {
            return Err(stage_err(&format!(
                "fanout row {row} pairs gmeow:extractsGraphFamily/{family:?} with \
                 gmeow:extractsForm/{form:?} (the \"opaque\" family and \"blob\" form are \
                 mutually required)"
            )));
        }
        if is_opaque && match_prefix {
            return Err(stage_err(&format!(
                "opaque fanout row {row} must use gmeow:extractsMatch \"exact\" (an opaque \
                 archive member is one byte-exact entry, never a prefix family)"
            )));
        }
        // The `header-dict` family and form are mutually required in exactly the same
        // way, for exactly the same reason: a dictionary row that lost its pairing would
        // silently degrade to a named-graph fold of a graph that does not exist.
        let is_header_dict = family == FanoutFamily::HeaderDict;
        if is_header_dict != (form == GraphForm::HeaderDict) {
            return Err(stage_err(&format!(
                "fanout row {row} pairs gmeow:extractsGraphFamily/{family:?} with \
                 gmeow:extractsForm/{form:?} (the \"header-dict\" family and form are \
                 mutually required)"
            )));
        }
        if is_header_dict {
            if match_prefix {
                return Err(stage_err(&format!(
                    "header-dict fanout row {row} must use gmeow:extractsMatch \"exact\" (a \
                     header \"dct\" entry is one byte-exact dictionary, never a prefix family)"
                )));
            }
            if !is_header_dict_path(&path) {
                return Err(stage_err(&format!(
                    "header-dict fanout row {row} declares gmeow:extractsPath {path:?}, which is \
                     not a {MEDIUM_DICT_PREFIX}<dict-id>{MEDIUM_DICT_SUFFIX} path — the header \
                     \"dct\" key IS the committed file's stem, so any other path would name a \
                     dictionary the header cannot resolve"
                )));
            }
        }
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
    // Opaque and header-dict rows carry no named-graph rep (they reconstruct from the
    // archive blob and from the segment header's `"dct"` map respectively), so they are
    // skipped here — a byte-decorated RDF path (`.ttl`) that now has an opaque row still
    // falls through to its blob member, never a phantom named-graph fold.
    let rule = rules.iter().find(|r| {
        !matches!(r.family, FanoutFamily::Opaque | FanoutFamily::HeaderDict) && r.matches(path)
    })?;
    let iri = match rule.family {
        FanoutFamily::Edoal => edoal_projection_graph_iri(path)?,
        FanoutFamily::RdfFanout => rdf_fanout_graph_iri(path)?,
        FanoutFamily::Opaque | FanoutFamily::HeaderDict => return None,
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

/// The completeness HARD-FAIL for the OPAQUE half of the fanout map: assert the emitted
/// `gmeow:extractsGraphFamily "opaque"` rows are a BIJECTION against the generated-opaque
/// archive members (`REP_GENERATED`). Every opaque archive member path resolves to exactly
/// one opaque row (no undeclared member, no ambiguity), and every opaque row is claimed by
/// exactly one member (no stale row). This is the guard that closes the former hole where
/// the opaque blob members were declared NOWHERE and never bijection-checked — the exact
/// symmetric property [`check_fanout_bijection`] gives the RDF-fanout / EDOAL named graphs.
/// Opaque rows are always `exact` matches, so `rule.matches(path)` is `path == rule.path`.
fn check_opaque_bijection(
    opaque_rules: &[FanoutRule],
    member_paths: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    // Forward: every opaque archive member is claimed by exactly one opaque row.
    for path in member_paths {
        let n = opaque_rules.iter().filter(|r| r.matches(path)).count();
        if n != 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "generated-opaque archive member {path} matches {n} opaque \
                     gmeow:fanoutExtracts rows (want exactly 1)"
                ),
            }));
        }
    }
    // Reverse: every opaque row is claimed by exactly one archive member (no stale/duplicate).
    for rule in opaque_rules {
        let n = member_paths.iter().filter(|p| rule.matches(p)).count();
        if n != 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "opaque gmeow:fanoutExtracts row for {:?} claims {n} generated-opaque \
                     archive members (want exactly 1)",
                    rule.path
                ),
            }));
        }
    }
    Ok(())
}

/// The committed-path family a trained zstd dictionary is projected onto. Mirrors
/// [`crate::medium::MEDIUM_GENERATED_PREFIX`]; the compile-time assertion below turns a
/// drift between the two into a build failure rather than an orphan sweep.
pub(crate) const MEDIUM_DICT_PREFIX: &str = "generated/medium/";

/// The extension every projected dictionary file carries.
pub(crate) const MEDIUM_DICT_SUFFIX: &str = ".zdict";

const _: () = assert!(
    crate::medium::MEDIUM_GENERATED_PREFIX.len() == MEDIUM_DICT_PREFIX.len(),
    "the medium axis's reserved path family and the gate's projection family must agree"
);

/// The committed path a header `"dct"` key projects onto.
pub(crate) fn header_dict_path(dict_id: &str) -> String {
    format!("{MEDIUM_DICT_PREFIX}{dict_id}{MEDIUM_DICT_SUFFIX}")
}

/// The header `"dct"` key a committed dictionary path names — the inverse of
/// [`header_dict_path`], and `None` for any path outside the family.
pub(crate) fn header_dict_id_for_path(path: &str) -> Option<&str> {
    path.strip_prefix(MEDIUM_DICT_PREFIX)?
        .strip_suffix(MEDIUM_DICT_SUFFIX)
}

/// Whether a committed path belongs to the header-dictionary family.
pub(crate) fn is_header_dict_path(path: &str) -> bool {
    header_dict_id_for_path(path).is_some()
}

/// Every dictionary the shipped segment header pins in band, keyed by its committed
/// `generated/medium/<dict-id>.zdict` path.
///
/// The header's `"dct"` map is the dictionary's ONE home: it is where a consumer priming
/// its own runtime store reads the bytes from, and it is the only channel that keeps the
/// pack self-decoding. Reading the projection from there — rather than tarring a second
/// copy into `REP_GENERATED` — is what makes "the bytes exist exactly once" a structural
/// property of the bundle instead of a convention.
///
/// An empty `"dct"` map is legal and yields no paths: a deliberately unprimed bundle (a
/// minimal fixture, a `convert --to gts` exit) declares no dictionaries, and the
/// bijection below then proves it authors no header-dict rows either.
fn read_header_dicts(gts_bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let dicts = gmeow_gts_profile::segment_dictionaries(gts_bytes)
        .map_err(|e| stage_err(&format!("read gmeow.gts header dictionaries: {e}")))?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (id, bytes) in dicts {
        if bytes.is_empty() {
            return Err(stage_err(&format!(
                "the segment header pins dictionary {id:?} with zero bytes — a named-but-empty \
                 dictionary primes nothing while still raising the reader contract"
            )));
        }
        out.insert(header_dict_path(&id), bytes);
    }
    Ok(out)
}

/// The completeness HARD-FAIL for the HEADER-DICT family of the fanout map: assert the
/// authored `gmeow:extractsGraphFamily "header-dict"` rows are a BIJECTION against the
/// entries of the shipped segment header's in-band `"dct"` map. Every pinned dictionary
/// resolves to exactly one row (no undeclared dictionary), and every row is claimed by
/// exactly one pinned dictionary (no stale row naming a dictionary the pack dropped).
/// Family-scoped, exactly as [`check_opaque_bijection`] is, so a dictionary row can never
/// satisfy a named-graph path or an archive member it does not carry. Header-dict rows are
/// always `exact` matches, so `rule.matches(path)` is `path == rule.path`.
fn check_header_dict_bijection(
    header_dict_rules: &[FanoutRule],
    dict_paths: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    // Forward: every pinned header dictionary is claimed by exactly one header-dict row.
    for path in dict_paths {
        let n = header_dict_rules.iter().filter(|r| r.matches(path)).count();
        if n != 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "header \"dct\" entry {path} matches {n} header-dict \
                     gmeow:fanoutExtracts rows (want exactly 1)"
                ),
            }));
        }
    }
    // Reverse: every header-dict row is claimed by exactly one pinned dictionary.
    for rule in header_dict_rules {
        let n = dict_paths.iter().filter(|p| rule.matches(p)).count();
        if n != 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::FanoutBijection {
                message: format!(
                    "header-dict gmeow:fanoutExtracts row for {:?} claims {n} header \"dct\" \
                     entries (want exactly 1)",
                    rule.path
                ),
            }));
        }
    }
    Ok(())
}

/// The predicate carrying one authored expected-output path on the pipeline individual.
const EXPECTED_OUTPUT_PRED: &str = "https://blackcatinformatics.ca/gmeow/expectsGeneratedOutput";

/// Read the AUTHORED expected-output inventory from the bundle dataset: every
/// `gmeow:expectsGeneratedOutput "generated/…"` literal, deduplicated and sorted. This is
/// hand-written TTL in the pipeline slice (NOT carrier-emitted like the opaque fanout rows),
/// so it is an INDEPENDENT completeness oracle — when a carrier change silently drops an
/// output, `project_bundle`'s reconstructed set shrinks but this authored set does not, and
/// [`check_expected_completeness`] HARD-fails. A non-literal object, a path not under
/// `generated/`, a duplicate value, or an empty inventory (the rows never reached the bundle)
/// is a HARD FAIL (no-optionality), never a silent pass.
pub(crate) fn read_expected_outputs(
    dataset: &RdfDataset,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.owned_quads() {
        if quad.predicate != EXPECTED_OUTPUT_PRED {
            continue;
        }
        let purrdf::RdfTerm::Literal(lit) = &quad.object else {
            return Err(expected_err(&format!(
                "gmeow:expectsGeneratedOutput on {:?} has a non-literal object (want a \"generated/…\" path literal)",
                quad.subject
            )));
        };
        let path = &lit.lexical_form;
        if !path.starts_with("generated/") {
            return Err(expected_err(&format!(
                "gmeow:expectsGeneratedOutput value {path:?} is not under generated/"
            )));
        }
        if EXCLUDED.contains(&path.as_str()) {
            return Err(expected_err(&format!(
                "gmeow:expectsGeneratedOutput must not list the terminal bundle {path:?} (a bundle is not a projection of itself)"
            )));
        }
        if !out.insert(path.clone()) {
            return Err(expected_err(&format!(
                "gmeow:expectsGeneratedOutput lists {path:?} more than once (the inventory is a set)"
            )));
        }
    }
    if out.is_empty() {
        return Err(expected_err(
            "no gmeow:expectsGeneratedOutput rows in the bundle — the authored expected-output inventory did not reach gmeow.gts",
        ));
    }
    Ok(out)
}

/// The independent-oracle completeness HARD-FAIL: every AUTHORED expected path must be
/// PRODUCED by the reconstructed bundle (`expected ⊆ files.keys()`). The message names EVERY
/// missing path. This is the anchor that survives the Task-3 disk-walk inversion: it is the
/// "every declared output is produced" direction, proved against the bundle rather than the
/// on-disk tree, so a clean clone that no longer emits a consumed output cannot pass silently.
fn check_expected_completeness(
    files: &BTreeMap<String, Vec<u8>>,
    expected: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    let missing: Vec<&str> = expected
        .iter()
        .filter(|p| !files.contains_key(p.as_str()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::ExpectedOutputMissing {
                message: format!(
                    "{} authored generated/ output(s) not produced by the bundle: {}",
                    missing.len(),
                    missing.join(", ")
                ),
            },
        ));
    }
    Ok(())
}

/// A derivable prefix family: an authored expected-output family whose members can be
/// INDEPENDENTLY re-derived at the gate from the carrier's reconstruction-graph IRIs (an
/// emitted-Rust source, distinct from the authored TTL). Only families all of whose members
/// travel as named-graph folds qualify (the mixed RDF/opaque families — research-objects,
/// lang projections — are authored-only, guarded by a count-consistency test instead).
struct DerivableFamily {
    /// The committed-path prefix (e.g. `generated/profiles/`).
    prefix: &'static str,
    /// An optional suffix filter isolating the family within a shared directory (the EDOAL
    /// `.edoal.ttl` case, which shares `generated/projections/` with plain RDF projections).
    suffix: Option<&'static str>,
    /// Which independently-derived path set this family's membership is proved against.
    source: DerivedSource,
}

/// The gate-side oracle a [`DerivableFamily`]'s membership is re-derived from — always a
/// source DISTINCT from the authored inventory it is checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DerivedSource {
    /// The carrier's reconstruction-graph IRIs (`graph/fanout/…`, `graph/projections/…`).
    ReconstructionGraphs,
    /// The shipped segment header's in-band `"dct"` map — a WIRE source, read straight off
    /// the emitted bytes rather than out of any graph, so it is independent of both the
    /// authored inventory and the carrier's named graphs.
    HeaderDicts,
}

impl DerivableFamily {
    fn contains(&self, path: &str) -> bool {
        path.starts_with(self.prefix) && self.suffix.is_none_or(|s| path.ends_with(s))
    }
}

/// The prefix families whose membership is DERIVED (not merely authored): the RDF-fanout
/// `generated/profiles/*` set and the EDOAL `generated/projections/*.edoal.ttl` set, both of
/// which travel entirely as reconstruction named graphs; and the
/// `generated/medium/*.zdict` set, whose membership is the shipped segment header's own
/// `"dct"` map. Each yields its exact membership independently of the authored inventory, so
/// a dictionary added to (or retired from) the medium axis without its expected path
/// HARD-fails instead of silently shrinking the projection.
const DERIVABLE_FAMILIES: [DerivableFamily; 3] = [
    DerivableFamily {
        prefix: "generated/profiles/",
        suffix: None,
        source: DerivedSource::ReconstructionGraphs,
    },
    DerivableFamily {
        prefix: "generated/projections/",
        suffix: Some(".edoal.ttl"),
        source: DerivedSource::ReconstructionGraphs,
    },
    DerivableFamily {
        prefix: MEDIUM_DICT_PREFIX,
        suffix: Some(MEDIUM_DICT_SUFFIX),
        source: DerivedSource::HeaderDicts,
    },
];

/// The derivation cross-check: for each [`DERIVABLE_FAMILIES`] member, the AUTHORED family
/// (from the expected-output inventory) must EXACTLY equal the family DERIVED from the
/// carrier's reconstructed named-graph paths. A source individual added without its expected
/// path (derived ⊋ authored) or a stale authored path (authored ⊋ derived) HARD-fails, so
/// the dynamic families cannot silently drift out of the authored inventory.
fn check_derivable_families(
    expected: &BTreeSet<String>,
    reconstructed: &BTreeSet<String>,
    header_dicts: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    for family in &DERIVABLE_FAMILIES {
        let oracle = match family.source {
            DerivedSource::ReconstructionGraphs => reconstructed,
            DerivedSource::HeaderDicts => header_dicts,
        };
        let authored: BTreeSet<&str> = expected
            .iter()
            .filter(|p| family.contains(p))
            .map(String::as_str)
            .collect();
        let derived: BTreeSet<&str> = oracle
            .iter()
            .filter(|p| family.contains(p))
            .map(String::as_str)
            .collect();
        if authored != derived {
            let authored_only: Vec<&str> = authored.difference(&derived).copied().collect();
            let derived_only: Vec<&str> = derived.difference(&authored).copied().collect();
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::ExpectedOutputMissing {
                    message: format!(
                        "derivable family {}{} authored≠derived: authored-only [{}], derived-only [{}]",
                        family.prefix,
                        family.suffix.unwrap_or(""),
                        authored_only.join(", "),
                        derived_only.join(", ")
                    ),
                },
            ));
        }
    }
    Ok(())
}

fn expected_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::ExpectedOutputMissing {
        message: message.to_string(),
    })
}

/// The embedded graph label of the committed diagnostics `.nq` files (mirrors
/// `carrier::GRAPH_DIAGNOSTICS`).
pub(crate) const GRAPH_DIAGNOSTICS_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// The build-time authority for "which committed RDF `generated/` path is attached as an
/// RDF-fanout named graph", DERIVED from the AUTHORED `gmeow:fanoutExtracts` rows (family
/// `"rdf-fanout"`) of the loaded pipeline-slice source — the SAME rows the superset gate
/// reads back from the shipped bundle. This replaces a former hand-maintained `||` chain
/// that duplicated the authored inventory as a third copy; the set is now a single
/// source-of-truth read from data, so the carrier's attach set and the gate's claim set
/// cannot silently drift.
///
/// Built once per assemble (from the `stage-source-load` product, which already holds the
/// pipeline `module.ttl` in memory — a source INPUT, not the bundle being produced, so
/// there is no bootstrapping cycle) and consulted per committed path.
pub(crate) struct RdfFanoutClasses {
    rules: Vec<FanoutRule>,
}

impl RdfFanoutClasses {
    /// Parse the authored `gmeow:fanoutExtracts` rows from a pipeline-slice `source`
    /// dataset, keeping only the RDF-fanout family (EDOAL keeps its own
    /// `graph/projections/` family; opaque rows are carrier-emitted, not a build-time
    /// attach class). A malformed row HARD-fails through [`read_fanout_rules`].
    pub(crate) fn from_source(source: &RdfDataset) -> Result<Self, gmeow_errors::Diag> {
        let rules = read_fanout_rules(source)?
            .into_iter()
            .filter(|r| r.family == FanoutFamily::RdfFanout)
            .collect();
        Ok(Self { rules })
    }

    /// Whether a committed RDF `generated/` path is attached as an RDF-fanout named graph,
    /// per the authored rows (exact or directory-prefix match).
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.rules.iter().any(|r| r.matches(path))
    }
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
    sweep_against_materialized(&projection, root)
}

/// Sweep a reconstructed [`BundleProjection`] against the materialized `generated/`
/// tree under `root`. The **projection is the authority**: the forward sweep drives
/// off the projection keys (the must-exist set), the reverse sweep off the on-disk
/// tree (the orphan oracle). A pure function of the projection and the on-disk tree —
/// no bundle parsing — so the sweep verdicts are unit-testable with an injected
/// projection.
fn sweep_against_materialized(
    projection: &BundleProjection,
    root: &Path,
) -> Result<SupersetReport, gmeow_errors::Diag> {
    let mut missing = Vec::new();
    let mut mismatch = Vec::new();

    // ── Forward sweep: every projection key MUST be materialized on disk with
    // byte-matching content. A key with no file on disk is `missing`; a key present
    // on disk whose bytes differ from the reconstruction is `mismatch`. Because the
    // projection (not the disk walk) is the must-exist set, an empty or absent
    // `generated/` tree flags EVERY key as missing — no vacuous pass on a fresh
    // clone. A read error other than "not found" is a hard failure, never a skip. ──
    for (path, reconstructed) in &projection.files {
        match std::fs::read(root.join(path)) {
            Ok(disk_bytes) => {
                if *reconstructed != disk_bytes {
                    mismatch.push(path.clone());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing.push(path.clone()),
            Err(e) => return Err(stage_err(&format!("read materialized {path}: {e}"))),
        }
    }

    // ── Reverse sweep: every materialized `generated/` file MUST be a projection key.
    // The bundle is a *superset* of `generated/` (§5): it also carries source archives
    // (`dsl/`, `slices/` shapes/cells/tests) and the rendered docs site for
    // self-sufficiency — but `project_bundle` already filtered those out (it emits only
    // `generated/`-targeting reps). An orphan is thus an UNDECLARED / STALE on-disk
    // file: a materialized `generated/` path that is not a projection key. The terminal
    // bundle is a materialized `generated/` file that is not a projection of itself, so
    // it is excluded here rather than reported as an orphan. ──
    let materialized = materialized_generated_paths(root)?;
    let mut orphan = Vec::new();
    for path in &materialized {
        if EXCLUDED.contains(&path.as_str()) {
            continue;
        }
        if !projection.files.contains_key(path) {
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
        // Unreachable in practice: an opaque or header-dict row never yields a `GraphRep`
        // (`graph_rep_for_path` skips both families), so neither the `Blob` nor the
        // `HeaderDict` form is ever handed to a graph fold. Fail closed rather than panic
        // if the invariant is ever violated.
        GraphForm::Blob | GraphForm::HeaderDict => None,
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
/// The blob-member reconstruction map plus the opaque-archive member set: [`files`]
/// is every archive member keyed by committed path (all `generated/`-carrying archives),
/// and [`opaque_paths`] is the subset that rode the `REP_GENERATED` generated-opaque
/// archive — the exact set the carrier declares as `gmeow:extractsGraphFamily "opaque"`
/// rows, which [`check_opaque_bijection`] proves is a bijection against those rows.
struct BlobMembers {
    files: BTreeMap<String, Vec<u8>>,
    opaque_paths: BTreeSet<String>,
}

fn read_blob_members(
    graph: &purrdf::gts::model::Graph,
    lookaside: &purrdf::RdfLookaside,
) -> Result<BlobMembers, gmeow_errors::Diag> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut opaque_paths: BTreeSet<String> = BTreeSet::new();
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
            // The generated-opaque archive is the byte lane the carrier declares as
            // `opaque` fanout rows; record its members (all under `generated/`) as the
            // set the opaque bijection proves against those rows.
            if rep == crate::stages::carrier::REP_GENERATED && committed.starts_with("generated/") {
                opaque_paths.insert(committed.clone());
            }
            out.insert(committed, member_bytes);
        }
    }
    Ok(BlobMembers {
        files: out,
        opaque_paths,
    })
}

/// Every materialized file under `<root>/generated/`, repo-relative (`generated/...`),
/// sorted. Walks the tree directly.
fn materialized_generated_paths(root: &Path) -> Result<Vec<String>, gmeow_errors::Diag> {
    // GENERATED-READ-OK: this read is ONLY the reverse (orphan) oracle. After the
    // authority inversion the projection — not this disk walk — is the must-exist set,
    // so the walk no longer decides completeness; it only enumerates the materialized
    // tree so the reverse sweep can flag an on-disk file that is not a projection key.
    // Nothing here folds into gmeow.gts. An absent `generated/` tree is not a clean
    // pass: it yields zero orphans here while the forward sweep flags every projection
    // key as missing.
    let base = root.join("generated");
    let mut out = Vec::new();
    if base.exists() {
        walk(&base, root, &mut out)?;
    }
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
        // `.cache/gmeow-sync/pipeline/` (gitignored opt-in stage cache). The gate
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
    fn excluded_holds_exactly_the_one_terminal_bundle() {
        assert_eq!(EXCLUDED.len(), 1);
        assert!(EXCLUDED.contains(&"generated/dist/gmeow.gts"));
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
            // The statement layer's two: same reason, but they reconstruct from
            // REP_STATEMENTS rather than REP_GENERATED — a rep is the unit a dictionary
            // primes, and these are the claim corpus's byte frames.
            "generated/statements/gmeow-statements.owl.ttl",
            "generated/statements/gmeow.rdf12.ttl",
        ] {
            assert!(
                graph_rep_for_path(&rules, path).is_none(),
                "{path} has generated comments / section markers, so it cannot reconstruct \
                 from a canonical named-graph fold and must ride an archive member"
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
        let ttl = std::fs::read(repo_root().join("slices/core/pipeline/module.ttl")).unwrap();
        let source = purrdf::parse_dataset(&ttl, "text/turtle", None).unwrap();
        assert!(
            RdfFanoutClasses::from_source(&source)
                .unwrap()
                .contains(PATH)
        );
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

        // ── The inventory's OTHER direction. `project_bundle` proves authored ⊆ produced
        // (`check_expected_completeness`); nothing proved produced ⊆ authored, so an output
        // the pipeline emits could stay absent from the hand-authored oracle indefinitely —
        // and one did (`generated/mappings/gmeow-preference.sssom.tsv`, produced from the
        // preference slice's MappingSet but never declared). An undeclared output is a real
        // hole, not a harmless omission: the completeness anchor is the ONLY gate that catches
        // a stage silently ceasing to emit a file, so a path missing from the inventory is a
        // path that can vanish from a clean clone unnoticed. Closing the loop here makes the
        // inventory EXACTLY the produced set: authored ⊆ produced (project_bundle, above)
        // plus produced ⊆ authored (here) = equality. Free — the projection is already in hand.
        let authored = authored_expected();
        let undeclared: Vec<&str> = proj
            .files
            .keys()
            .map(String::as_str)
            .filter(|p| !EXCLUDED.contains(p) && !authored.contains(*p))
            .collect();
        assert!(
            undeclared.is_empty(),
            "the bundle produces generated/ output(s) absent from the authored \
             gmeow:expectsGeneratedOutput inventory in slices/core/pipeline/module.ttl, so the \
             completeness oracle would not notice them disappearing: {undeclared:?}"
        );
    }

    #[test]
    fn sweep_against_materialized_detects_missing_mismatch_and_orphan() {
        use std::io::Write;
        // Authority is the PROJECTION. Materialize a disk tree of three files under
        // generated/, then inject a projection whose keys diverge from it.
        // RAII: the materialized tree is removed when `tmp` drops, including on a
        // failed assertion below.
        let tmp = tempfile::tempdir().expect("create temp materialized root");
        let dir = tmp.path();
        let gen_dir = dir.join("generated/x");
        std::fs::create_dir_all(&gen_dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let mut f = std::fs::File::create(gen_dir.join(name)).unwrap();
            f.write_all(bytes).unwrap();
        };
        write("match.ttl", b"SAME");
        write("drift.ttl", b"DISK-BYTES");
        write("orphan.ttl", b"UNDECLARED");

        // Projection keys: match.ttl agrees with disk; drift.ttl reconstructs to
        // different bytes than disk (mismatch); missing.ttl has NO disk file (missing).
        // orphan.ttl is on disk but is NOT a projection key (orphan).
        let mut files = BTreeMap::new();
        files.insert("generated/x/match.ttl".to_string(), b"SAME".to_vec());
        files.insert(
            "generated/x/drift.ttl".to_string(),
            b"BUNDLE-BYTES".to_vec(),
        );
        files.insert("generated/x/missing.ttl".to_string(), b"GONE".to_vec());
        let projection = BundleProjection { files };

        let report = sweep_against_materialized(&projection, dir).unwrap();

        assert_eq!(report.missing, vec!["generated/x/missing.ttl".to_string()]);
        assert_eq!(report.mismatch, vec!["generated/x/drift.ttl".to_string()]);
        assert_eq!(report.orphan, vec!["generated/x/orphan.ttl".to_string()]);
        assert!(!report.is_clean());
    }

    #[test]
    fn superset_empty_materialized_tree_hard_fails() {
        // The vacuous-pass guard: with the projection as the authority, an EMPTY
        // (or absent) generated/ tree can never pass clean — every projection key is
        // flagged missing. Prove it for both an empty generated/ dir and a wholly
        // absent one.
        let mut files = BTreeMap::new();
        files.insert("generated/x/a.ttl".to_string(), b"A".to_vec());
        files.insert("generated/y/b.ttl".to_string(), b"B".to_vec());
        let projection = BundleProjection { files };

        // (a) An empty-but-present generated/ directory. RAII: removed when
        // `empty_tmp` drops, including on a failed assertion below.
        let empty_tmp = tempfile::tempdir().expect("create empty materialized root");
        let empty_dir = empty_tmp.path();
        std::fs::create_dir_all(empty_dir.join("generated")).unwrap();
        let report = sweep_against_materialized(&projection, empty_dir).unwrap();
        assert_eq!(
            report.missing,
            vec![
                "generated/x/a.ttl".to_string(),
                "generated/y/b.ttl".to_string()
            ]
        );
        assert!(report.orphan.is_empty());
        assert!(
            !report.is_clean(),
            "an empty generated/ tree must HARD-fail, never pass vacuously"
        );

        // (b) A wholly absent generated/ tree (fresh clone) is equally not clean.
        // The root exists but carries no `generated/` child at all.
        let absent_tmp = tempfile::tempdir().expect("create absent-generated root");
        let absent_dir = absent_tmp.path();
        let report = sweep_against_materialized(&projection, absent_dir).unwrap();
        assert_eq!(report.missing.len(), 2);
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
    fn opaque_rows_parse_and_bijection_checks_the_archive_members() {
        // The opaque family: exact/opaque/blob rows carrier-emitted per REP_GENERATED
        // member. Prove they parse, resolve NO named-graph rep (they ride the blob lane),
        // and that check_opaque_bijection HARD-fails on an undeclared member AND a stale row.
        let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:o1 gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
gmeow:o2 gmeow:extractsPath "generated/logic/inferred-closure.rdf12.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
"#;
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        let rules = read_fanout_rules(&ds).unwrap();
        let (opaque, named): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
            .into_iter()
            .partition(|r| r.family == FanoutFamily::Opaque);
        assert_eq!(opaque.len(), 2);
        assert_eq!(named.len(), 1);

        // Opaque rows never resolve a named-graph rep — even the byte-decorated `.ttl` one.
        assert!(graph_rep_for_path(&opaque, "generated/n3/gmeow.n3").is_none());
        assert!(
            graph_rep_for_path(&opaque, "generated/logic/inferred-closure.rdf12.ttl").is_none()
        );

        // Bijection holds when the member set equals the opaque-row path set exactly.
        let members: BTreeSet<String> = [
            "generated/n3/gmeow.n3",
            "generated/logic/inferred-closure.rdf12.ttl",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        check_opaque_bijection(&opaque, &members).expect("opaque bijection holds");

        // Undeclared member (a blob member with no opaque row) HARD-fails.
        let mut undeclared = members.clone();
        undeclared.insert("generated/cl/gmeow.clif".to_string());
        let err = check_opaque_bijection(&opaque, &undeclared).unwrap_err();
        assert_eq!(err.code(), crate::error::FanoutBijection::register());

        // Stale opaque row (a row claiming no archive member) HARD-fails.
        let mut stale = members.clone();
        stale.remove("generated/n3/gmeow.n3");
        let err2 = check_opaque_bijection(&opaque, &stale).unwrap_err();
        assert_eq!(err2.code(), crate::error::FanoutBijection::register());
    }

    #[test]
    fn opaque_family_and_blob_form_are_mutually_required() {
        // A row that pairs opaque family with a non-blob form is malformed → HARD FAIL.
        let bad_form = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "turtle" .
"#;
        let ds = purrdf::parse_dataset(bad_form.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_fanout_rules(&ds).is_err());

        // A blob form paired with a non-opaque family is equally malformed.
        let bad_family = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "blob" .
"#;
        let ds2 = purrdf::parse_dataset(bad_family.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_fanout_rules(&ds2).is_err());

        // An opaque row using a prefix match is malformed (opaque members are exact).
        let bad_prefix = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:x gmeow:extractsPath "generated/n3/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
"#;
        let ds3 = purrdf::parse_dataset(bad_prefix.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_fanout_rules(&ds3).is_err());
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

    /// The authored expected-output inventory, read from the pipeline slice module.ttl
    /// (the same data the shipped bundle carries) — the real independent oracle.
    fn authored_expected() -> BTreeSet<String> {
        let ttl = std::fs::read(repo_root().join("slices/core/pipeline/module.ttl")).unwrap();
        let ds = purrdf::parse_dataset(&ttl, "text/turtle", None).unwrap();
        read_expected_outputs(&ds).unwrap()
    }

    #[test]
    fn expected_output_inventory_round_trips_from_the_authored_module_ttl() {
        // The authored gmeow:expectsGeneratedOutput rows round-trip through the gate's OWN
        // reader: the complete non-terminal generated/ tree, deduplicated, every path under
        // generated/, and neither terminal bundle present.
        let expected = authored_expected();
        assert_eq!(
            expected.len(),
            416,
            "the authored inventory must hold every non-terminal generated/ path"
        );
        for p in &expected {
            assert!(
                p.starts_with("generated/"),
                "non-generated inventory path: {p}"
            );
            assert!(
                !EXCLUDED.contains(&p.as_str()),
                "terminal bundle leaked in: {p}"
            );
        }
        // The known runtime-consumed catalog files (crates/docs/src/model.rs) are present.
        assert!(expected.contains("generated/catalog/constraint-catalog.nq"));
        assert!(expected.contains("generated/catalog/term-content-manifest.nq"));
    }

    #[test]
    fn read_expected_outputs_rejects_malformed_rows() {
        // Non-literal object.
        let bad_obj = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput gmeow:not-a-literal ."#;
        let ds = purrdf::parse_dataset(bad_obj.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_expected_outputs(&ds).is_err());
        // A path not under generated/.
        let outside = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "docs/x.md" ."#;
        let ds = purrdf::parse_dataset(outside.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_expected_outputs(&ds).is_err());
        // A terminal bundle listed.
        let terminal = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "generated/dist/gmeow.gts" ."#;
        let ds = purrdf::parse_dataset(terminal.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_expected_outputs(&ds).is_err());
        // The same path declared by two subjects (identical triples collapse under RDF set
        // semantics, so a genuine duplicate needs distinct subjects).
        let dup = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build gmeow:expectsGeneratedOutput "generated/a.ttl" .
gmeow:other gmeow:expectsGeneratedOutput "generated/a.ttl" ."#;
        let ds = purrdf::parse_dataset(dup.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_expected_outputs(&ds).is_err());
        // No rows at all — the inventory did not reach the bundle.
        let empty = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build a gmeow:Pipeline ."#;
        let ds = purrdf::parse_dataset(empty.as_bytes(), "text/turtle", None).unwrap();
        assert!(read_expected_outputs(&ds).is_err());
    }

    #[test]
    fn completeness_hard_fails_naming_every_dropped_output() {
        // The completeness anchor: dropping ONE declared path from the produced set fires the
        // ExpectedOutputMissing HARD FAIL, and the message names the missing path — exactly the
        // deterministic-drop case the two-generation determinism gate is blind to.
        let expected = authored_expected();
        // A produced set that reconstructs every declared path passes.
        let full: BTreeMap<String, Vec<u8>> =
            expected.iter().map(|p| (p.clone(), Vec::new())).collect();
        check_expected_completeness(&full, &expected).expect("full production is complete");
        // Drop one declared output (simulate a carrier code change that stops emitting it).
        let dropped = "generated/catalog/constraint-catalog.nq";
        let mut partial = full.clone();
        partial.remove(dropped);
        let err = check_expected_completeness(&partial, &expected).unwrap_err();
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
        assert!(
            err.to_string().contains(dropped),
            "the HARD FAIL must name the dropped path, got: {err}"
        );
    }

    #[test]
    fn project_bundle_hard_fails_when_a_declared_output_is_never_produced() {
        // NEVER-PRODUCED — the regression this task guards: a producing stage change stops
        // emitting a declared output. The bundle then carries NO representative for it, so the
        // bytes are IDENTICAL across two cold runs — the two-generation determinism gate is
        // blind. Only the completeness oracle catches it, and it must bite through the REAL
        // `project_bundle` path (not only the hand-built projection map
        // `completeness_hard_fails_naming_every_dropped_output` exercises), proving the
        // `check_expected_completeness` call at the top of `project_bundle` is wired.
        //
        // Build a minimal-but-valid gmeow.gts through the production terminal
        // (`emit_gmeow_gts`): the ontology header the importer requires, plus two authored
        // `gmeow:expectsGeneratedOutput` rows for paths the bundle does NOT produce — one
        // OPAQUE-family member (`generated/n3/gmeow.n3`, normally an inline archive member) and
        // one PREFIX-family member (`generated/profiles/full.ttl`, normally a named-graph fold).
        // With no fanout rows, no reconstruction graphs, and no opaque archive, the
        // reconstructed `files` set is empty, so BOTH declared paths are "never produced".
        use purrdf::gts_compose::SnapshotBuilder;

        const OPAQUE_MEMBER: &str = "generated/n3/gmeow.n3";
        const PREFIX_MEMBER: &str = "generated/profiles/full.ttl";
        let doc = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix dcterms: <http://purl.org/dc/terms/> .\n\
             <https://blackcatinformatics.ca/gmeow> a owl:Ontology ;\n\
                 dcterms:title \"GMEOW\" ;\n\
                 owl:versionInfo \"test\" .\n\
             gmeow:pipeline-build gmeow:expectsGeneratedOutput \"{OPAQUE_MEMBER}\" , \"{PREFIX_MEMBER}\" .\n"
        );
        let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).unwrap();
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(ds.as_ref()).expect("add_dataset");
        let gts =
            gmeow_gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
                .expect("emit minimal expected-output bundle");

        // The reusable path performs the complete proof over one tied decode.
        let decoded = decode_projection_source(&gts).expect("decode projection source");
        let decoded_err = project_decoded_bundle(&decoded).expect_err(
            "decoded projection must HARD-fail: two declared outputs were never produced",
        );
        assert_eq!(
            decoded_err.code(),
            crate::error::ExpectedOutputMissing::register()
        );

        // The public one-shot path is exactly the decode + projection composition.
        let err = project_bundle(&gts)
            .expect_err("project_bundle must HARD-fail: two declared outputs were never produced");
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
        let msg = err.to_string();
        assert_eq!(decoded_err.to_string(), msg);
        assert!(
            msg.contains(OPAQUE_MEMBER),
            "the HARD FAIL must name the never-produced opaque-family path, got: {msg}"
        );
        assert!(
            msg.contains(PREFIX_MEMBER),
            "the HARD FAIL must name the never-produced prefix-family path, got: {msg}"
        );
    }
    #[test]
    fn derivable_families_cross_check_catches_authored_derived_drift() {
        // The two DERIVED families (profiles, edoal): authored must EXACTLY equal the set the
        // carrier's reconstruction graphs yield. The real authored counts are pinned so a
        // silent family-count change trips the count-consistency guard.
        let expected = authored_expected();
        let profiles: BTreeSet<&str> = expected
            .iter()
            .filter(|p| p.starts_with("generated/profiles/"))
            .map(String::as_str)
            .collect();
        let edoal: BTreeSet<&str> = expected
            .iter()
            .filter(|p| p.starts_with("generated/projections/") && p.ends_with(".edoal.ttl"))
            .map(String::as_str)
            .collect();
        let dicts: BTreeSet<String> = expected
            .iter()
            .filter(|p| is_header_dict_path(p))
            .cloned()
            .collect();
        assert_eq!(profiles.len(), 8, "profiles family membership drifted");
        assert_eq!(edoal.len(), 47, "edoal family membership drifted");
        assert_eq!(dicts.len(), 6, "header-dict family membership drifted");

        // Equal authored/derived over the derivable families passes.
        let reconstructed: BTreeSet<String> = expected
            .iter()
            .filter(|p| {
                p.starts_with("generated/profiles/")
                    || (p.starts_with("generated/projections/") && p.ends_with(".edoal.ttl"))
            })
            .cloned()
            .collect();
        check_derivable_families(&expected, &reconstructed, &dicts).expect("authored == derived");

        // A source individual added without its expected path (derived ⊋ authored) HARD-fails.
        let mut extra = reconstructed.clone();
        extra.insert("generated/profiles/newprofile.ttl".to_string());
        let err = check_derivable_families(&expected, &extra, &dicts).unwrap_err();
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());

        // A stale authored path (authored ⊋ derived) HARD-fails too.
        let mut short = reconstructed.clone();
        assert!(short.remove("generated/profiles/full.ttl"));
        let err = check_derivable_families(&expected, &short, &dicts).unwrap_err();
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());

        // The header-dict family is derived from the WIRE (the header's own "dct" map),
        // so both drift directions bite there too: a dictionary the medium axis pins but
        // the inventory never declared, and an inventory entry the header dropped.
        let mut extra_dict = dicts.clone();
        extra_dict.insert(header_dict_path("gmeow-invented-v1"));
        let err = check_derivable_families(&expected, &reconstructed, &extra_dict).unwrap_err();
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
        let mut short_dict = dicts.clone();
        assert!(short_dict.remove(&header_dict_path("gmeow-core-v1")));
        let err = check_derivable_families(&expected, &reconstructed, &short_dict).unwrap_err();
        assert_eq!(err.code(), crate::error::ExpectedOutputMissing::register());
    }

    /// The header-dict family: rows parse, resolve NO named-graph rep, and the
    /// family-scoped bijection HARD-fails on an undeclared pinned dictionary AND on a
    /// stale row naming a dictionary the pack no longer carries.
    #[test]
    fn header_dict_rows_parse_and_bijection_checks_the_pinned_dictionaries() {
        let ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:d1 gmeow:extractsPath "generated/medium/gmeow-core-v1.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" .
gmeow:d2 gmeow:extractsPath "generated/medium/gmeow-logic-v1.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" .
gmeow:r1 gmeow:extractsPath "generated/evals/scores.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "turtle" .
gmeow:o1 gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "opaque" ; gmeow:extractsForm "blob" .
"#;
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        let rules = read_fanout_rules(&ds).unwrap();
        let (header_dict, rest): (Vec<FanoutRule>, Vec<FanoutRule>) = rules
            .into_iter()
            .partition(|r| r.family == FanoutFamily::HeaderDict);
        assert_eq!(header_dict.len(), 2);
        assert_eq!(rest.len(), 2);

        // A header-dict row never resolves a named-graph rep — it rides the header lane.
        for rule_set in [&header_dict, &rest] {
            assert!(
                graph_rep_for_path(rule_set, "generated/medium/gmeow-core-v1.zdict").is_none(),
                "a .zdict path must never resolve a phantom named-graph fold"
            );
        }

        let pinned: BTreeSet<String> = ["gmeow-core-v1", "gmeow-logic-v1"]
            .iter()
            .map(|id| header_dict_path(id))
            .collect();
        check_header_dict_bijection(&header_dict, &pinned).expect("header-dict bijection holds");

        // An undeclared pinned dictionary (the medium axis grew a third) HARD-fails.
        let mut undeclared = pinned.clone();
        undeclared.insert(header_dict_path("gmeow-unrowed-v1"));
        let err = check_header_dict_bijection(&header_dict, &undeclared).unwrap_err();
        assert_eq!(err.code(), crate::error::FanoutBijection::register());

        // A stale row (the pack stopped pinning that dictionary) HARD-fails too.
        let mut stale = pinned.clone();
        assert!(stale.remove(&header_dict_path("gmeow-core-v1")));
        let err = check_header_dict_bijection(&header_dict, &stale).unwrap_err();
        assert_eq!(err.code(), crate::error::FanoutBijection::register());
    }

    /// The `header-dict` family and form are mutually required, the match is always
    /// `exact`, and the path must be a `generated/medium/<id>.zdict` one — otherwise the
    /// row would name a dictionary the header cannot resolve.
    #[test]
    fn header_dict_family_form_match_and_path_shape_are_mutually_required() {
        let cases = [
            // family header-dict with a graph form.
            r#"gmeow:x gmeow:extractsPath "generated/medium/a.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "turtle" ."#,
            // form header-dict with a graph family.
            r#"gmeow:x gmeow:extractsPath "generated/medium/a.zdict" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "header-dict" ."#,
            // header-dict with a prefix match.
            r#"gmeow:x gmeow:extractsPath "generated/medium/" ; gmeow:extractsMatch "prefix" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" ."#,
            // header-dict on a path outside the family.
            r#"gmeow:x gmeow:extractsPath "generated/n3/gmeow.n3" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dict" ; gmeow:extractsForm "header-dict" ."#,
        ];
        for case in cases {
            let ttl = format!("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{case}\n");
            let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
            assert!(
                read_fanout_rules(&ds).is_err(),
                "malformed header-dict row accepted: {case}"
            );
        }
    }

    /// An unknown family / form string is still a HARD FAIL — the enums stay closed even
    /// though a fourth family was added.
    #[test]
    fn an_unknown_fanout_family_or_form_still_hard_fails() {
        for case in [
            r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "header-dictionary" ; gmeow:extractsForm "turtle" ."#,
            r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "zdict" ."#,
            r#"gmeow:x gmeow:extractsPath "generated/a.ttl" ; gmeow:extractsMatch "exact" ; gmeow:extractsGraphFamily "invented" ; gmeow:extractsForm "turtle" ."#,
        ] {
            let ttl = format!("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n{case}\n");
            let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
            assert!(
                read_fanout_rules(&ds).is_err(),
                "unknown family/form accepted: {case}"
            );
        }
    }

    /// The one enumerated value set, read from BOTH sides: the `skos:definition`s of
    /// `gmeow:extractsGraphFamily` / `gmeow:extractsForm` name exactly the strings the
    /// Rust match arms accept. Without this the ontology could keep enumerating three
    /// families while the code accepted four — a Principle 4 second source of truth in
    /// which the shipped definition contradicts the shipped behaviour.
    #[test]
    fn the_rust_family_and_form_arms_equal_the_ontology_declared_value_sets() {
        let ttl = std::fs::read_to_string(repo_root().join("slices/core/pipeline/module.ttl"))
            .expect("the pipeline slice is readable");
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();

        // Every family/form string the Rust reader accepts, proved by round-tripping a
        // one-row document per value rather than by re-listing the arms (a second copy of
        // the match would drift exactly as the prose did).
        let accepted = |predicate: &str, value: &str| -> bool {
            let (family, form) = match predicate {
                "extractsGraphFamily" => (value, form_for_family(value)),
                _ => (family_for_form(value), value),
            };
            let path = match family {
                "header-dict" => "generated/medium/probe-v1.zdict",
                _ => "generated/probe.ttl",
            };
            let doc = format!(
                "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                 gmeow:probe gmeow:extractsPath {path:?} ; gmeow:extractsMatch \"exact\" ; \
                 gmeow:extractsGraphFamily {family:?} ; gmeow:extractsForm {form:?} .\n"
            );
            let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).unwrap();
            read_fanout_rules(&ds).is_ok()
        };

        for (predicate, rust_values) in [
            (
                "extractsGraphFamily",
                vec!["rdf-fanout", "edoal", "opaque", "header-dict"],
            ),
            (
                "extractsForm",
                vec![
                    "turtle",
                    "ntriples",
                    "nquads-self",
                    "nquads-diagnostics",
                    "blob",
                    "header-dict",
                ],
            ),
        ] {
            // The Rust side really does accept every value claimed, and nothing else it
            // was not told about.
            for value in &rust_values {
                assert!(
                    accepted(predicate, value),
                    "gmeow:{predicate} value {value:?} is claimed but the Rust reader rejects it"
                );
            }
            assert!(
                !accepted(predicate, "not-a-declared-value"),
                "gmeow:{predicate} accepts an undeclared value"
            );
            let declared = declared_value_set(&ds, predicate);
            assert_eq!(
                declared,
                rust_values.iter().map(|v| (*v).to_string()).collect(),
                "the gmeow:{predicate} skos:definition and the Rust match arms disagree"
            );
        }
    }

    /// The form a family is REQUIRED to pair with, for the round-trip probe above.
    fn form_for_family(family: &str) -> &'static str {
        match family {
            "opaque" => "blob",
            "header-dict" => "header-dict",
            _ => "turtle",
        }
    }

    /// The family a form is REQUIRED to pair with, for the round-trip probe above.
    fn family_for_form(form: &str) -> &'static str {
        match form {
            "blob" => "opaque",
            "header-dict" => "header-dict",
            _ => "rdf-fanout",
        }
    }

    /// The value set a property's `skos:definition` DECLARES, read out of the normative
    /// prose: the `"a" | "b" | …` list after the `are exactly:` marker. The enumeration
    /// lives in the definition (not in a second machine-only vocabulary) so the shipped
    /// English and the shipped behaviour cannot say different things.
    fn declared_value_set(ds: &RdfDataset, local: &str) -> BTreeSet<String> {
        const MARKER: &str = "are exactly:";
        let subject = format!("https://blackcatinformatics.ca/gmeow/{local}");
        let definition = ds
            .owned_quads()
            .filter(|q| {
                q.subject == purrdf::RdfTerm::iri(&subject)
                    && q.predicate == "http://www.w3.org/2004/02/skos/core#definition"
            })
            .find_map(|q| match q.object {
                purrdf::RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            })
            .unwrap_or_else(|| panic!("gmeow:{local} carries no skos:definition"));
        let tail = definition
            .split_once(MARKER)
            .unwrap_or_else(|| {
                panic!("gmeow:{local}'s skos:definition must enumerate its values after {MARKER:?}")
            })
            .1;
        let tail = tail.split_once('.').map_or(tail, |(head, _)| head);
        tail.split('|')
            .map(|part| {
                let part = part.trim();
                part.trim_matches('"').to_string()
            })
            .filter(|part| !part.is_empty())
            .collect()
    }

    #[test]
    fn authored_only_families_hold_their_count_consistency() {
        // The two prefix families whose producing individuals are NOT cleanly enumerable at
        // the gate — research-objects (a single research object with a mixed RDF / JSON / XML /
        // HTML sub-tree) and the heterogeneous lang projections (per-reading CoNLL-U, per-example
        // GMN1, per-sentence NIF/TEI/SEMAF, EBNF grammars) — are AUTHORED-ONLY. They cannot be
        // re-derived from reconstruction graphs (their non-RDF members ride opaque blobs), so a
        // count-consistency guard stands in for a derivation cross-check: a silent add/drop trips
        // the pinned membership. Both prefixes are also fully covered by the completeness ⊇ anchor.
        let expected = authored_expected();
        let research: Vec<&String> = expected
            .iter()
            .filter(|p| p.starts_with("generated/research-objects/"))
            .collect();
        let lang: Vec<&String> = expected
            .iter()
            .filter(|p| p.starts_with("generated/projections/lang/"))
            .collect();
        assert_eq!(
            research.len(),
            13,
            "research-objects family membership drifted"
        );
        assert_eq!(lang.len(), 35, "lang-projections family membership drifted");
        // These families are genuinely mixed (not all RDF), the reason they are authored-only.
        assert!(
            research.iter().any(|p| p.ends_with(".json"))
                && research.iter().any(|p| p.ends_with(".ttl")),
            "research-objects should carry both RDF and non-RDF members"
        );
        assert!(
            lang.iter().any(|p| p.ends_with(".conllu")) && lang.iter().any(|p| p.ends_with(".ttl")),
            "lang projections should carry both RDF and non-RDF members"
        );
    }

    /// Recursively collect the committed `generated/<file>` paths that shipped code names via
    /// the repo-root read idiom `root.join("generated/…")` under one directory, skipping
    /// integration-test trees (`…/tests/…`).
    fn collect_root_join_generated(dir: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                collect_root_join_generated(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let needle = "root.join(\"generated/";
                let mut rest = text.as_str();
                while let Some(i) = rest.find(needle) {
                    let after = &rest[i + "root.join(\"".len()..];
                    if let Some(end) = after.find('"') {
                        let p = &after[..end];
                        // Only file references (a dotted final segment), never bare dirs.
                        if p.rsplit('/').next().is_some_and(|seg| seg.contains('.')) {
                            out.insert(p.to_string());
                        }
                        rest = &after[end..];
                    } else {
                        break;
                    }
                }
            }
        }
    }

    #[test]
    fn every_runtime_generated_read_is_in_the_authored_inventory() {
        // Downstream-read guarantee: every committed generated/ file shipped code reads at
        // runtime (via root.join) must be in the authored inventory, so a clean clone cannot
        // silently lose a consumed output. The terminal bundle is legitimately excluded.
        let inventory = authored_expected();
        let mut refs = BTreeSet::new();
        collect_root_join_generated(&repo_root().join("crates"), &mut refs);
        // Sanity: the flagged docs consumer read is actually discovered by the scan.
        assert!(
            refs.contains("generated/catalog/constraint-catalog.nq"),
            "scan failed to discover the crates/docs/src/model.rs catalog read"
        );
        for path in &refs {
            if EXCLUDED.contains(&path.as_str()) {
                continue;
            }
            assert!(
                inventory.contains(path),
                "runtime read {path} is absent from the authored expected-output inventory — a \
                 clean clone would silently lose a consumed file"
            );
        }
    }
}
