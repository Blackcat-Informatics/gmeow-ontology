// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The superset gate: `gmeow.gts` is a superset of `generated/`.
//!
//! For every committed path under `generated/`, this gate resolves the path's
//! carrier representative from the **shipped** bundle and reconstructs the bytes:
//!
//! * an RDF output is the canonical fold of one named graph (Turtle via the
//!   wasm-clean renderer; N-Quads via a graph-rooted serialization), and
//! * an opaque output is a member of one inline content-addressed archive blob.
//!
//! A committed path with no representative, a representative whose reconstruction
//! does not match the committed bytes, or a carried representative with no
//! committed counterpart is a hard failure — no skips, no optional coverage, no
//! degraded pass. The gate proves equality of the carried set, not merely a
//! one-directional superset: it sweeps both `generated/ -> bundle` (missing /
//! mismatch) and `bundle -> generated/` (orphan).
//!
//! Reconstruction reads the bundle back through [`gmeow_rdf::import_gts_events`]
//! and the GTS blob reader, closing the serialize -> parse loop, so it proves
//! byte-reconstructibility from the emitted bundle rather than from in-memory
//! carrier state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_rdf::RdfDataset;

use crate::error::PipelineError;

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

/// The serialization whose output equals one named graph's committed bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphForm {
    /// Canonical Turtle (single graph, no graph label).
    Turtle,
    /// N-Triples of the projected graph (no graph label).
    NTriples,
    /// N-Quads of the graph re-rooted into its IRI (graph label restamped).
    NQuads,
}

/// A committed path carried as the fold of one named graph: the backing graph IRI
/// and the serialization form whose output equals the committed bytes.
struct GraphRep {
    iri: String,
    form: GraphForm,
}

/// Resolve the named-graph representative for a committed `generated/` path. Every
/// RDF output (`.ttl`/`.nt`/`.nq`) is carried as a named graph (the locked
/// principle: RDF travels as RDF, the fold is the byte-truth); opaque outputs are
/// inline blob members. The serialization form is fixed by the file extension and
/// matches the form the producing stage emits the committed file with, so
/// `file == fold` holds by construction. A path whose graph is not (yet) carried
/// reconstructs to `None` and surfaces as `missing` — how the gate enumerates the
/// remaining gap.
fn graph_rep_for_path(path: &str) -> Option<GraphRep> {
    // EDOAL projections keep their dedicated per-file `graph/projections/<stem>`.
    if let Some(iri) = edoal_projection_graph_iri(path) {
        return Some(GraphRep {
            iri,
            form: GraphForm::Turtle,
        });
    }
    if !is_rdf_fanout_class(path) {
        return None;
    }
    let iri = rdf_fanout_graph_iri(path)?;
    let form = if path.ends_with(".nt") {
        GraphForm::NTriples
    } else if path.ends_with(".nq") {
        GraphForm::NQuads
    } else {
        GraphForm::Turtle
    };
    Some(GraphRep { iri, form })
}

/// Whether a committed RDF `generated/` path is carried as an RDF-fanout named graph
/// (vs. an older dedicated rep). Both the gate (claiming the rep) and the carrier
/// (attaching the graph) consult this single predicate, so the wired set is one
/// authority. Classes are added here as their producing stage starts emitting the
/// committed file as the canonical fold of its attached graph.
pub(crate) fn is_rdf_fanout_class(path: &str) -> bool {
    path.starts_with("generated/profiles/") || path.starts_with("generated/research-objects/")
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
        if let Some(gmeow_rdf::RdfTerm::Iri(iri)) = &quad.graph_name {
            if iri.starts_with(&projections) || iri.starts_with(RDF_FANOUT_NS) {
                out.insert(iri.clone());
            }
        }
    }
    out
}

/// Run the superset gate over `gts_bytes` (the emitted `gmeow.gts`) against every
/// committed file under `<root>/generated/`.
pub fn check_superset(root: &Path, gts_bytes: &[u8]) -> Result<SupersetReport, PipelineError> {
    let dataset = read_dataset(gts_bytes)?;
    let dataset = dataset.as_ref();
    let blob_members = read_blob_members(gts_bytes)?;
    let committed = committed_generated_paths(root)?;

    let mut missing = Vec::new();
    let mut mismatch = Vec::new();

    // Track which blob members a committed path consumed, for the reverse sweep.
    let mut used_blob_members: BTreeSet<String> = BTreeSet::new();

    // ── Forward sweep: every committed path must reconstruct from the bundle. ──
    for path in &committed {
        if EXCLUDED.contains(&path.as_str()) {
            continue;
        }
        let committed_bytes = std::fs::read(root.join(path))
            .map_err(|e| stage_err(&format!("read committed {path}: {e}")))?;

        if let Some(rep) = graph_rep_for_path(path) {
            match reconstruct_graph(dataset, &rep) {
                Some(folded) => {
                    if folded == committed_bytes {
                        continue;
                    }
                    mismatch.push(path.clone());
                }
                None => missing.push(path.clone()),
            }
            continue;
        }

        // Opaque: the path is an archive-blob member, keyed by repo-relative path
        // or by bare basename (the existing reps use one of the two).
        if let Some(key) = match_blob_member(path, &blob_members) {
            used_blob_members.insert(key.clone());
            if blob_members[&key] == committed_bytes {
                continue;
            }
            mismatch.push(path.clone());
            continue;
        }

        missing.push(path.clone());
    }

    // ── Reverse sweep: every `generated/`-TARGETING representative must map to a
    // committed `generated/` path. The bundle is a *superset* of `generated/`
    // (§5): it also carries source archives (`dsl/`, `slices/` shapes/cells/tests)
    // and the rendered docs site for self-sufficiency — those are legitimate extra
    // payload, NOT orphans. An orphan is a STALE `generated/` reconstruction rep: a
    // carried member whose key denotes a `generated/` path with no committed file. ──
    let committed_set: BTreeSet<&str> = committed.iter().map(String::as_str).collect();
    let mut orphan = Vec::new();
    for member in blob_members.keys() {
        if member.starts_with("generated/")
            && !used_blob_members.contains(member)
            && !committed_set.contains(member.as_str())
        {
            orphan.push(format!("blob-member:{member}"));
        }
    }
    // Named-graph orphans: every RDF-reconstruction graph in the bundle (an EDOAL
    // projection graph or an `…/graph/fanout/…` RDF-fanout graph) must back a
    // committed file. Other carrier graphs (statements, imports, reasoning, …) are
    // not `generated/`-reconstruction reps and are out of scope.
    for iri in reconstruction_graph_iris(dataset) {
        let path = edoal_path_for_graph_iri(&iri).or_else(|| rdf_fanout_path_for_graph_iri(&iri));
        if let Some(path) = path {
            if !committed_set.contains(path.as_str()) {
                orphan.push(format!("named-graph:{iri}"));
            }
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
            Some(gmeow_rdf::turtle_normalize::render(&projected, &rdf_prefixes()).into_bytes())
        }
        GraphForm::NTriples => gmeow_rdf::serialize_dataset_to_format(
            &projected,
            gmeow_rdf::NativeRdfFormat::NTriples,
            None,
        )
        .ok()
        .map(|outcome| outcome.bytes),
        GraphForm::NQuads => {
            // `project_named_graph` drops the graph label; restamp it so the N-Quads
            // 4th column matches the committed file, then serialize.
            let rooted = crate::stages::carrier::rooted_in_graph(&projected, &rep.iri).ok()?;
            gmeow_rdf::serialize_dataset_to_format(
                &rooted,
                gmeow_rdf::NativeRdfFormat::NQuads,
                None,
            )
            .ok()
            .map(|outcome| outcome.bytes)
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

/// Match a committed path against the unpacked archive-blob member keys: first by
/// repo-relative path (the preferred, collision-free key), then by bare basename
/// (the legacy reps key SSSOM / queries / schemas by basename, unique within their
/// directory). Returns the matched member key.
fn match_blob_member(path: &str, members: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    if members.contains_key(path) {
        return Some(path.to_string());
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    if members.contains_key(base) {
        return Some(base.to_string());
    }
    None
}

/// Parse the emitted bundle back into a native dataset (closes serialize -> parse).
fn read_dataset(gts_bytes: &[u8]) -> Result<std::sync::Arc<RdfDataset>, PipelineError> {
    let bundle = gmeow_rdf::import_gts_events(gts_bytes)
        .map_err(|e| stage_err(&format!("re-import gmeow.gts: {e}")))?;
    Ok(bundle.dataset)
}

/// Unpack every inline archive blob into a single `member-key -> bytes` map. The
/// blob payloads are read from the GTS fold (digest -> bytes) joined with the
/// blob lookaside (digest -> representation); each archive is a deterministic
/// USTAR whose members are the committed files.
fn read_blob_members(gts_bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let graph = gmeow_rdf::gts::read_graph(gts_bytes, true)
        .map_err(|e| stage_err(&format!("read gmeow.gts blobs: {e}")))?;
    let lookaside = gmeow_rdf::gts::lookaside_from_graph(&graph);

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
        for (name, member_bytes) in gmeow_rdf::ustar::read_archive(&bytes)
            .map_err(|e| stage_err(&format!("unpack archive {}: {e}", record.digest)))?
        {
            out.insert(name, member_bytes);
        }
    }
    Ok(out)
}

/// Every committed file under `<root>/generated/`, repo-relative (`generated/...`),
/// sorted. Walks the tree directly (the gate enumerates the on-disk committed set,
/// not a stage product).
fn committed_generated_paths(root: &Path) -> Result<Vec<String>, PipelineError> {
    let base = root.join("generated");
    let mut out = Vec::new();
    walk(&base, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<(), PipelineError> {
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

fn stage_err(message: &str) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-superset-gate".to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_holds_exactly_the_two_terminal_bundles() {
        assert_eq!(EXCLUDED.len(), 2);
        assert!(EXCLUDED.contains(&"generated/dist/gmeow.gts"));
        assert!(EXCLUDED.contains(&"generated/dist/gmeow-full.gts"));
    }

    #[test]
    fn match_blob_member_prefers_repo_relative_then_basename() {
        let mut members = BTreeMap::new();
        members.insert("generated/mappings/foaf.sssom.tsv".to_string(), vec![1u8]);
        members.insert("bare.rq".to_string(), vec![2u8]);
        assert_eq!(
            match_blob_member("generated/mappings/foaf.sssom.tsv", &members).as_deref(),
            Some("generated/mappings/foaf.sssom.tsv")
        );
        assert_eq!(
            match_blob_member("generated/queries/bare.rq", &members).as_deref(),
            Some("bare.rq")
        );
        assert_eq!(match_blob_member("generated/x/none.txt", &members), None);
    }

    #[test]
    fn reconstruct_graph_folds_turtle_without_the_graph_label() {
        use gmeow_rdf::RdfDatasetBuilder;

        const G: &str = "https://blackcatinformatics.ca/gmeow/graph/projections/sample.edoal";
        const S: &str = "https://blackcatinformatics.ca/gmeow/projections/sample";
        const P: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const O: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Alignment";

        let mut b = RdfDatasetBuilder::new();
        let g = b.intern_iri(G.to_string());
        let s = b.intern_iri(S.to_string());
        let p = b.intern_iri(P.to_string());
        let o = b.intern_iri(O.to_string());
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
        assert!(reconstruct_graph(
            &dataset,
            &GraphRep {
                iri: "https://blackcatinformatics.ca/gmeow/graph/absent".to_string(),
                form: GraphForm::Turtle,
            },
        )
        .is_none());
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
