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

/// A committed path carried as the fold of one named graph: the backing graph IRI
/// whose canonical-Turtle fold equals the committed bytes. (The N-Quads-rooted form
/// for `.nq` classes lands with the diagnostics graphs.)
struct GraphRep {
    iri: String,
}

/// Resolve the named-graph representative for a committed `generated/` path, or
/// `None` if the path is not carried as a named graph (it is then expected to be
/// an inline blob member). This is the single graph-IRI <-> path convention,
/// shared with the producing stages so the mapping is identity in both directions.
fn graph_rep_for_path(path: &str) -> Option<GraphRep> {
    // Classes are wired here as their producing stage starts attaching the named
    // graph (and emitting `file == fold`). Until a class is wired, its committed
    // paths fall through to the blob match and — if not yet carried — surface as
    // `missing`, which is exactly how this gate enumerates the remaining gap.
    edoal_projection_graph_iri(path).map(|iri| GraphRep { iri })
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

/// Every distinct projection graph IRI present in the bundle dataset (the named
/// graphs under `…/graph/projections/`), for the reverse orphan sweep.
fn projection_graph_iris(dataset: &RdfDataset) -> BTreeSet<String> {
    let prefix = format!("{GRAPH_NS}projections/");
    let mut out = BTreeSet::new();
    for quad in dataset.owned_quads() {
        if let Some(gmeow_rdf::RdfTerm::Iri(iri)) = &quad.graph_name {
            if iri.starts_with(&prefix) {
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
    // Named-graph orphans: every projection graph in the bundle must back a
    // committed EDOAL file (the only named-graph reconstruction class wired so far).
    for iri in projection_graph_iris(dataset) {
        if let Some(path) = edoal_path_for_graph_iri(&iri) {
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
    Some(gmeow_rdf::turtle_normalize::render(&projected, &registry_prefixes()).into_bytes())
}

/// The project's single prefix authority, for the canonical Turtle renderer.
fn registry_prefixes() -> Vec<(String, String)> {
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

        let turtle = reconstruct_graph(&dataset, &GraphRep { iri: G.to_string() })
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
