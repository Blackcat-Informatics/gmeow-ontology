// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-free, bundle-backed access to the folded ontology surface and its
//! transforms — the native port of the Python `gmeow_tools.bundle` consumer.
//!
//! The `gmeow` deliverable ships ONE artifact — `generated/dist/gmeow.gts` — that
//! folds the complete useful ontology surface AND its transforms: the SSSOM lift
//! maps, the compiled projection queries, the equivalence/projection cells, the
//! test-DSL specs, the reasoning reports, the OKF export, the ontology-docs site,
//! the SHACL shape surface, the compiled logic/DL axioms, the JSON/OpenAPI
//! schemas, and the JSON-LD-star / YAML-LD-star serializations. Each rides as a
//! deterministic tar blob keyed by a representation label (the fold `rep`); the
//! `transform:denied` blob is a raw JSON payload rather than a tar.
//!
//! This module reads those blobs back **from the snapshot bytes alone, with no
//! repo checkout** — the CLI razor: `gmeow` does not need a repo, `gmeow-dev`
//! does. Read side only; the pipeline `carrier` / `gts-sink` stages build the
//! blobs. GTS is exit-only, so this operates purely on the `&[u8]` snapshot the
//! consumer already holds (`include_bytes!` of the embedded bundle); it never
//! reads the repo tree or disk.
//!
//! The rep-label strings MUST match the producer (`crate::stages::carrier`) and
//! the retired Python `REP_*` constants EXACTLY — a drifted label silently
//! resolves to an empty archive, shipping the bundle without that surface.

use std::collections::BTreeMap;
use std::path::Path;

use purrdf::gts::reader::read;
use purrdf::gts_view::GtsFoldView;

use crate::gmeow_ns::GMEOW_NS;

/// tar of `generated/mappings/*.sssom.tsv` (the SSSOM lift maps).
pub const REP_MAPPINGS: &str = "mappings-archive";
/// tar of `generated/queries/*.rq` (the compiled projection queries).
pub const REP_QUERIES: &str = "queries-archive";
/// tar of the cell/projection TTL sources, keyed by repo-relative path.
pub const REP_CELLS: &str = "cells-archive";
/// tar of the slice test-DSL specs, keyed by repo-relative path.
pub const REP_TESTS: &str = "tests-archive";
/// tar of the native reasoner's report artifacts (explanations + DL/EL ledger).
pub const REP_REASONING: &str = "reasoning-archive";
/// tar of the OKF (Open Knowledge Format) bundle.
pub const REP_OKF: &str = "okf-export";
/// tar of the Rust-rendered ontology-docs static site (per-language member paths).
pub const REP_ONTOLOGY_DOCS: &str = "ontology-docs";
/// tar of `gmeow.schema.json` + `gmeow.openapi.json`.
pub const REP_SCHEMAS: &str = "schemas-archive";
/// tar of `gmeow.jsonld` + `gmeow.yamlld` (the RDF 1.2-star serializations).
pub const REP_YAMLLD: &str = "yaml-ld-archive";
/// tar of the FULL SHACL shape surface, keyed by repo-relative path.
pub const REP_SHAPES: &str = "shapes-archive";
/// tar of the compiled logic/DL projection surface, keyed by repo-relative path.
pub const REP_AXIOMS: &str = "axioms-archive";
/// JSON (NOT a tar) of the saturation refusal set (the alignment-lint ERROR rows).
pub const REP_DENIED: &str = "transform:denied";
/// JSON (NOT a tar) of a diagnostics producer's FORWARD-projected
/// `Vec<gmeow_errors::DiagNode>` — the pre-lowered run-ledger nodes. On the pipeline
/// CARRIER this rides `stage-validate` / `stage-compile-logic`'s product bundle (the
/// run ledger's single source); on the SHIPPED gts it is absent (the run ledger is a
/// build-time projection, not a folded gts surface), so `diag_nodes` resolves to an
/// empty set — the wheel-only contract. This is the SINGLE definition of the label; the
/// producer side re-exports it as [`crate::stages::carrier::REP_DIAG_NODES`], so no drift
/// is possible (a drifted label would silently read empty).
pub const REP_DIAG_NODES: &str = "diagnostics:nodes";
/// JSON (NOT a tar) of `stage-source-load`'s authored subject→source-position
/// [`SpanIndex`](crate::ingest::SpanIndex): the source spans the diagnostics consumers
/// lift onto their findings. It rides ONLY the source-load product's blob lane and is
/// dropped before the carrier assembles, so it never folds into a shipped `gmeow.gts`
/// surface. This is the SINGLE definition of the label; the producer side re-exports it
/// as [`crate::stages::carrier::REP_SPAN_TABLE`], so no drift is possible (a drifted
/// label would silently read empty).
pub const REP_SPAN_TABLE: &str = "spans:source-table";
/// tar of the mdbook `src/` source tree (`book.toml` + `SUMMARY.md` + `src/<page>/index.md`
/// chapters), every member prefixed with the English internal tag (`x-gmeow-english/…`).
/// This is the SINGLE definition of the label; the producer side re-exports it as
/// [`crate::stages::carrier::REP_DOCS_BOOK`], so no drift is possible (a drifted label would
/// silently read back an empty archive).
pub const REP_DOCS_BOOK: &str = "docs-book";
/// tar of the print documentation projection: the byte-reproducible `gmeow.pdf` and its
/// deterministic `gmeow.typ` Typst source, both prefixed with the English internal tag
/// (`x-gmeow-english/gmeow.pdf`, `x-gmeow-english/gmeow.typ`). This is the SINGLE definition
/// of the label; the producer side re-exports it as [`crate::stages::carrier::REP_DOCS_PRINT`],
/// so no drift is possible (a drifted label would silently read back an empty archive).
pub const REP_DOCS_PRINT: &str = "docs-print";

/// The saturation refusal set: one `(subject, predicate, object)` ERROR row per
/// denied alignment cell, as recovered from the [`REP_DENIED`] JSON payload.
pub type DeniedCells = Vec<(String, String, String)>;

/// The `gmeow:guideBlob` predicate, filtered out of the reconstructed merged graph
/// (it references the per-slice guide content blobs, not ontology assertions).
fn guide_blob_iri() -> String {
    format!("{GMEOW_NS}guideBlob")
}

/// The `gmeow:graph/imports` named-graph IRI whose quads the merged-graph
/// reconstruction folds in alongside the default graph when `include_imports`.
fn graph_imports_iri() -> String {
    format!("{GMEOW_NS}graph/imports")
}

// A failure resolving a bundle blob: a malformed snapshot, an undecodable blob,
// a corrupt tar frame, or invalid `transform:denied` JSON. A missing rep is NOT
// an error — it resolves to an empty archive (the wheel-only-install contract).
use gmeow_errors::{FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// The snapshot bytes did not fold into a usable GTS graph.
    pub struct BundleParse { message: String }
    code = "pipeline.bundle.parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "bundle snapshot parse: {}", message;
}

define_diag_kind! {
    /// A blob's transformed wire bytes could not be decoded to its payload.
    pub struct BundleDecode { message: String }
    code = "pipeline.bundle.decode";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "bundle blob decode: {}", message;
}

define_diag_kind! {
    /// A blob's tar frame was malformed.
    pub struct BundleUntar { message: String }
    code = "pipeline.bundle.untar";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "bundle archive untar: {}", message;
}

define_diag_kind! {
    /// The `transform:denied` JSON payload was malformed.
    pub struct BundleJson { message: String }
    code = "pipeline.bundle.json";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "bundle denied-cells JSON: {}", message;
}

/// True when the canonical ontology source tree is on disk (a dev checkout) at
/// `ontology_file`. A repo-free consumer uses this to decide whether to read the
/// repo (fast dev path) or fall back to the bundle (a wheel-only install). A pure
/// path-existence probe — it never reads file content.
pub fn repo_sources_present(ontology_file: &Path) -> bool {
    ontology_file.exists()
}

/// A parsed `gmeow.gts` snapshot, the parse-once handle behind every accessor.
///
/// Folding the committed ~40 MB `gmeow.gts` snapshot is not free, so a consumer that reads several
/// archives should parse ONCE with [`Bundle::from_snapshot`] and call the methods;
/// the free `bundled_*` functions parse per call for the one-shot case.
pub struct Bundle {
    view: GtsFoldView,
}

impl Bundle {
    /// Fold `snapshot` (the `gmeow.gts` bytes) into a queryable bundle.
    ///
    /// GTS is exit-only: this consumes the terminal package the same way every
    /// external reader does, never an internal pipeline transport.
    pub fn from_snapshot(snapshot: &[u8]) -> Result<Self, gmeow_errors::Diag> {
        let graph = read(snapshot, true, None);
        if graph.terms.is_empty() && graph.blobs.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(
                crate::bundle_blobs::BundleParse {
                    message: "snapshot folded to an empty graph with no blobs (not a gmeow.gts?)"
                        .to_owned(),
                },
            ));
        }
        Ok(Self {
            view: GtsFoldView::new(graph),
        })
    }

    /// The decoded payload of the single blob carrying `rep`, or `None` when no
    /// blob declares it. First-match, mirroring the Python `_blob_by_rep`; every
    /// archive rep is carried by exactly one blob.
    pub fn blob_by_rep(&self, rep: &str) -> Result<Option<Vec<u8>>, gmeow_errors::Diag> {
        let graph = self.view.graph();
        let Some(digest) = graph
            .blob_meta
            .iter()
            .find(|(_, meta)| blob_meta_rep(meta).as_deref() == Some(rep))
            .map(|(digest, _)| digest.as_str())
        else {
            return Ok(None);
        };
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == digest) else {
            return Ok(None);
        };
        entry.decoded_vec().map(Some).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::bundle_blobs::BundleDecode {
                message: format!("blob {digest}: {e}"),
            })
        })
    }

    /// Untar the blob carrying `rep` into `{member-name: bytes}` (regular files
    /// only). An absent rep yields an empty map — the wheel-only-install contract.
    pub fn archive(&self, rep: &str) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        let Some(raw) = self.blob_by_rep(rep)? else {
            return Ok(BTreeMap::new());
        };
        let members = purrdf::ustar::read_archive(&raw).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::bundle_blobs::BundleUntar {
                message: format!("{rep}: {e}"),
            })
        })?;
        Ok(members.into_iter().collect())
    }

    /// Every folded SSSOM file as `{filename: tsv-bytes}` ([`REP_MAPPINGS`]).
    pub fn sssom(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_MAPPINGS)
    }

    /// Every folded projection query as `{"<profile>.rq": query-bytes}`
    /// ([`REP_QUERIES`]).
    pub fn queries(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_QUERIES)
    }

    /// Every folded cell/projection TTL as `{repo-relative-path: ttl-bytes}`
    /// ([`REP_CELLS`]). Keys preserve the repo-relative path so a loader routes
    /// each file to exactly the directory it reads in repo mode.
    pub fn cells(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_CELLS)
    }

    /// Every folded slice test-DSL spec as `{repo-relative-path: ttl-bytes}`
    /// ([`REP_TESTS`]).
    pub fn tests(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_TESTS)
    }

    /// The native reasoner's report artifacts as `{member: ttl-bytes}`
    /// ([`REP_REASONING`]): the entailment explanations + the DL/EL cross-check
    /// ledger over the bundle's reasoned closure.
    pub fn reasoning(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_REASONING)
    }

    /// Every folded OKF document as `{bundle-relative-path: md-bytes}`
    /// ([`REP_OKF`]).
    pub fn okf(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_OKF)
    }

    /// The full ontology-docs site as `{member-path: bytes}` ([`REP_ONTOLOGY_DOCS`]).
    /// Member paths are prefixed with the internal language tag
    /// (`x-gmeow-english/index.html`, …); `gmeow export-docs --format site` selects one language.
    pub fn ontology_docs(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_ONTOLOGY_DOCS)
    }

    /// The mdbook `src/` source tree as `{member-path: bytes}` ([`REP_DOCS_BOOK`]).
    /// Member paths are prefixed with the English internal language tag
    /// (`x-gmeow-english/book.toml`, `x-gmeow-english/src/SUMMARY.md`, …).
    pub fn docs_book(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_DOCS_BOOK)
    }

    /// The print documentation projection as `{member-path: bytes}` ([`REP_DOCS_PRINT`]):
    /// the byte-reproducible `x-gmeow-english/gmeow.pdf` and its deterministic
    /// `x-gmeow-english/gmeow.typ` Typst source.
    pub fn docs_print(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_DOCS_PRINT)
    }

    /// Every folded SHACL shape as `{repo-relative-path: ttl-bytes}` ([`REP_SHAPES`]):
    /// the FULL shape surface so a repo-free `gmeow validate` can reassemble both
    /// the data-graph validator union AND the separate DSL phases.
    pub fn shapes(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_SHAPES)
    }

    /// Every folded compiled logic/DL projection as `{repo-path: bytes}`
    /// ([`REP_AXIOMS`]): the small, committed projection surface a repo-free
    /// consumer needs (the big reasoning OUTPUTS ride other channels).
    pub fn axioms(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_AXIOMS)
    }

    /// The folded SHACL-derived schemas as `{filename: bytes}` ([`REP_SCHEMAS`]):
    /// `gmeow.schema.json` (JSON Schema) + `gmeow.openapi.json` (OpenAPI).
    pub fn schemas(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_SCHEMAS)
    }

    /// The bundled SHACL-derived JSON Schema (`gmeow.schema.json`), or `None` if
    /// absent.
    pub fn schema(&self) -> Result<Option<Vec<u8>>, gmeow_errors::Diag> {
        Ok(self.schemas()?.remove("gmeow.schema.json"))
    }

    /// The folded JSON-LD-star + YAML-LD-star serializations ([`REP_YAMLLD`]):
    /// `gmeow.jsonld` + `gmeow.yamlld`.
    pub fn yaml_ld(&self) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        self.archive(REP_YAMLLD)
    }

    /// The bundled JSON-LD-star serialization (`gmeow.jsonld`), or `None` if absent.
    pub fn jsonld_star(&self) -> Result<Option<Vec<u8>>, gmeow_errors::Diag> {
        Ok(self.yaml_ld()?.remove("gmeow.jsonld"))
    }

    /// Folded cell TTLs under repo-relative `prefix` PLUS every slice mappings file.
    ///
    /// Mirrors the two repo loaders exactly: the equivalences loader reads
    /// `dsl/mappings/equivalences/` + slice mappings; the projection loader reads
    /// `dsl/mappings/projections/` + slice mappings. Pass the directory prefix; the
    /// slice mappings (`slices/<g>/<n>/mappings/<file>.ttl`) are always included.
    pub fn cells_under(
        &self,
        prefix: &str,
    ) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        Ok(self
            .cells()?
            .into_iter()
            .filter(|(rel, _)| rel.starts_with(prefix) || is_slice_mapping(rel))
            .collect())
    }

    /// Reconstruct the merged-ontology N-Triples from the bundled named graphs: the
    /// default graph, plus `gmeow:graph/imports` when `include_imports`, with the
    /// `gmeow:guideBlob` reference triples filtered out. Deterministic (sorted).
    pub fn merged_ttl(&self, include_imports: bool) -> Result<Vec<u8>, gmeow_errors::Diag> {
        let graph = self.view.graph();
        let imports = graph_imports_iri();
        let guide = guide_blob_iri();
        let mut lines: Vec<String> = Vec::new();
        for (s, p, o, graph_id) in &graph.quads {
            let scope: Option<&str> = graph_id
                .map(|g| graph.terms[g].value.as_deref().unwrap_or(""))
                .filter(|v| !v.is_empty());
            let in_scope = match scope {
                None => true,
                Some(name) => include_imports && name == imports,
            };
            if !in_scope {
                continue;
            }
            if graph.terms[*p].value.as_deref() == Some(guide.as_str()) {
                continue;
            }
            lines.push(format!(
                "{} {} {} .",
                self.view.nq_token(*s),
                self.view.nq_token(*p),
                self.view.nq_token(*o)
            ));
        }
        lines.sort();
        let mut out = lines.join("\n");
        out.push('\n');
        Ok(out.into_bytes())
    }

    /// The FORWARD-projected diagnostics nodes ([`REP_DIAG_NODES`]) folded on a bundle,
    /// or an EMPTY vec when the rep is absent (the shipped gts carries no run ledger —
    /// the run ledger is a build-time projection, not a folded gts surface). Absent is
    /// NOT an error (wheel-only contract); malformed JSON IS a hard fail (no-optionality).
    pub fn diag_nodes(&self) -> Result<Vec<gmeow_errors::DiagNode>, gmeow_errors::Diag> {
        let Some(raw) = self.blob_by_rep(REP_DIAG_NODES)? else {
            return Ok(Vec::new());
        };
        serde_json::from_slice(&raw).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::bundle_blobs::BundleJson {
                message: format!("diagnostics nodes JSON: {e}"),
            })
        })
    }

    /// The precomputed saturation refusal set (the alignment-lint ERROR rows) as
    /// `(subject, predicate, object)` triples, or `None` when unbundled. Folded at
    /// build time so the consumer need not re-run the alignment lint.
    pub fn denied_cells(&self) -> Result<Option<DeniedCells>, gmeow_errors::Diag> {
        let Some(raw) = self.blob_by_rep(REP_DENIED)? else {
            return Ok(None);
        };
        let rows: Vec<Vec<String>> = serde_json::from_slice(&raw).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::bundle_blobs::BundleJson {
                message: e.to_string(),
            })
        })?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let [a, b, c] = <[String; 3]>::try_from(row).map_err(|bad| {
                gmeow_errors::Diag::of_kind(crate::bundle_blobs::BundleJson {
                    message: format!("denied-cells row is not a 3-tuple: {bad:?}"),
                })
            })?;
            out.push((a, b, c));
        }
        Ok(Some(out))
    }
}

/// The blob-DAG integrity law over a folded bundle, computed by
/// [`Bundle::integrity_report`]: a content-addressed store's defining
/// invariant is that (a) every reference resolves to a stored blob, (b) every
/// stored blob is reachable from a reference or declares its own producer
/// `rep`, and (c) a stored key IS the hash of its decoded value.
///
/// Reused as production code by `gmeow verify`'s "blob integrity" table row
/// (not test-local), so a regression in the bundle writer, a mis-keyed digest,
/// or truncated blob is caught for every consumer of the shipped bundle, not
/// just the gate.
#[derive(Debug, Default, Clone)]
pub struct BundleIntegrityReport {
    /// Every referenced content-addressed digest, keyed by the referencing
    /// predicate IRI (every property whose local name ends in `Blob`, e.g.
    /// `gmeow:guideBlob`, `lang:surfaceBlob`).
    pub referenced: BTreeMap<String, Vec<String>>,
    /// The subset of each predicate's referenced digests with no matching key
    /// in the bundle's blob store. Empty (every value empty) = pass.
    pub dangling: BTreeMap<String, Vec<String>>,
    /// Stored blob digests reachable from no reference predicate above and
    /// carrying no producer-declared `rep` label (an undeclared, unreferenced
    /// blob nobody can find). Empty = pass.
    pub orphan_blobs: Vec<String>,
    /// `(stored digest, recomputed blake3 digest)` pairs where decoding the
    /// blob and rehashing it does not reproduce the stored key. Empty = pass.
    pub hash_mismatches: Vec<(String, String)>,
}

impl BundleIntegrityReport {
    /// True iff there is no dangling reference, no orphan blob, and no
    /// hash-integrity mismatch across the whole bundle.
    pub fn is_clean(&self) -> bool {
        self.dangling.values().all(Vec::is_empty)
            && self.orphan_blobs.is_empty()
            && self.hash_mismatches.is_empty()
    }

    /// A short, readable summary naming the first few offenders per category —
    /// sized for both a test-failure message and a `gmeow verify` table cell.
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return "clean: no dangling refs, no orphan blobs, no hash mismatches".to_owned();
        }
        let preview = |items: &[String]| -> String {
            items.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        };
        let mut lines = Vec::new();
        for (pred, digests) in &self.dangling {
            if digests.is_empty() {
                continue;
            }
            lines.push(format!(
                "dangling {pred}: {} ref(s), e.g. {}",
                digests.len(),
                preview(digests)
            ));
        }
        if !self.orphan_blobs.is_empty() {
            lines.push(format!(
                "orphan blobs: {} unreferenced, e.g. {}",
                self.orphan_blobs.len(),
                preview(&self.orphan_blobs)
            ));
        }
        if !self.hash_mismatches.is_empty() {
            let sample: Vec<String> = self
                .hash_mismatches
                .iter()
                .take(3)
                .map(|(digest, recomputed)| format!("{digest} != {recomputed}"))
                .collect();
            lines.push(format!(
                "hash mismatches: {} blob(s), e.g. {}",
                self.hash_mismatches.len(),
                sample.join(", ")
            ));
        }
        lines.join("\n")
    }
}

impl std::fmt::Display for BundleIntegrityReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.summary())
    }
}

impl Bundle {
    /// Compute the [`BundleIntegrityReport`] over this bundle's graph: the
    /// content-addressed blob-reference predicates (`gmeow:guideBlob`,
    /// `lang:surfaceBlob`, and any other predicate following the same
    /// naming/documentation convention), their dangling refs, the orphan
    /// blobs, and the hash-integrity mismatches.
    ///
    /// Modeled on the attestation-digest walk in
    /// [`crate::stages::release::verify_release_bundle`] — same idiom, applied
    /// to the whole blob DAG rather than just the attested subset.
    ///
    /// Reference predicates are found by the ontology's own naming
    /// convention — every property whose local name ends in `Blob`
    /// (`gmeow:guideBlob`, `lang:surfaceBlob`) is documented (`skos:definition`
    /// in `slices/core/kernel/module.ttl` / `slices/grounding/lang/module.ttl`)
    /// as "held in the bundle's content-addressed blob channel", i.e. an actual
    /// dereference contract against `graph().blobs`. This is deliberately
    /// narrower than "any predicate whose object literal is `blake3:`-shaped":
    /// the ontology also carries generic content-hash/fingerprint predicates
    /// (`gmeow:contentDigest`, domain-free identity-by-content;
    /// `gmeow:definitionDigest`, a term's citation permalink; SPDX
    /// `checksumValue`) that legitimately use the same `blake3:`/`sha256:`
    /// literal shape without ever promising a matching bundle blob — for
    /// `gmeow:contentDigest` specifically, that dereference contract holds only
    /// inside a fully folded release-attestation graph, which is
    /// [`crate::stages::release::verify_release_bundle`]'s own, separate
    /// concern. Scanning those generically here would flag thousands of
    /// legitimate fingerprints as "dangling", which is not what this law means.
    pub fn integrity_report(&self) -> Result<BundleIntegrityReport, gmeow_errors::Diag> {
        let graph = self.view.graph();

        // Referenced digests per predicate: every property whose local name
        // ends in `Blob` — the ontology's documented content-addressed
        // blob-reference convention (see the doc comment above).
        let mut referenced: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_s, p, o, _g) in &graph.quads {
            let Some(pred) = graph.terms.get(*p).and_then(|t| t.value.as_deref()) else {
                continue;
            };
            if !pred.ends_with("Blob") {
                continue;
            }
            let Some(text) = graph.terms.get(*o).and_then(|t| t.value.as_deref()) else {
                continue;
            };
            referenced
                .entry(pred.to_owned())
                .or_default()
                .push(text.to_owned());
        }
        for digests in referenced.values_mut() {
            digests.sort();
            digests.dedup();
        }

        // Dangling refs: referenced digests with no matching stored blob key.
        let stored: std::collections::BTreeSet<&str> =
            graph.blobs.iter().map(|(d, _)| d.as_str()).collect();
        let mut dangling: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (pred, digests) in &referenced {
            let missing: Vec<String> = digests
                .iter()
                .filter(|d| !stored.contains(d.as_str()))
                .cloned()
                .collect();
            dangling.insert(pred.clone(), missing);
        }

        // Orphan blobs: stored digests reachable from no `*Blob` reference
        // predicate above AND carrying no producer-declared `rep` label at all.
        // A `blob_meta` "pub" map's `rep` field is the producer's own
        // declaration of why the blob exists (§12) — every archive/report/
        // export the pipeline folds sets one (see the `REP_*` catalogs spread
        // across `crate::stages::carrier` / `crate::stages::compile_logic` and
        // this module's own subset), so "carries a rep" is the complete,
        // forward-compatible test for "intentionally shipped", not a
        // hand-maintained enumeration of the reps this module happens to
        // expose a typed accessor for.
        let all_referenced: std::collections::BTreeSet<&str> =
            referenced.values().flatten().map(String::as_str).collect();
        let mut orphan_blobs: Vec<String> = Vec::new();
        for (digest, _entry) in &graph.blobs {
            if all_referenced.contains(digest.as_str()) {
                continue;
            }
            let has_declared_rep = graph
                .blob_meta
                .iter()
                .any(|(d, meta)| d == digest && blob_meta_rep(meta).is_some());
            if !has_declared_rep {
                orphan_blobs.push(digest.clone());
            }
        }
        orphan_blobs.sort();

        // Hash-integrity: the stored key must equal blake3(decoded bytes) — the
        // content-addressed store's actual defining law.
        let mut hash_mismatches: Vec<(String, String)> = Vec::new();
        for (digest, entry) in &graph.blobs {
            let decoded = entry.decoded_vec().map_err(|e| {
                gmeow_errors::Diag::of_kind(BundleDecode {
                    message: format!("blob {digest}: {e}"),
                })
            })?;
            let recomputed = format!("blake3:{}", blake3::hash(&decoded).to_hex());
            if &recomputed != digest {
                hash_mismatches.push((digest.clone(), recomputed));
            }
        }

        Ok(BundleIntegrityReport {
            referenced,
            dangling,
            orphan_blobs,
            hash_mismatches,
        })
    }
}

/// Extract the `rep` text from a blob's folded `pub` metadata map (CBOR). Mirrors
/// the release-fold recovery: a blob frame's `pub` map carries `mt` + `rep`.
fn blob_meta_rep(meta: &ciborium::value::Value) -> Option<String> {
    use ciborium::value::Value;
    let Value::Map(entries) = meta else {
        return None;
    };
    for (k, v) in entries {
        if let (Value::Text(key), Value::Text(val)) = (k, v)
            && key == "rep"
        {
            return Some(val.clone());
        }
    }
    None
}

/// True for a `slices/<group>/<name>/mappings/<file>.ttl` repo-relative path.
fn is_slice_mapping(relpath: &str) -> bool {
    let parts: Vec<&str> = relpath.split('/').collect();
    parts.len() == 5 && parts[0] == "slices" && parts[3] == "mappings" && relpath.ends_with(".ttl")
}

/// Every folded SSSOM file (one-shot parse of `snapshot`; see [`Bundle::sssom`]).
pub fn bundled_sssom(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.sssom()
}

/// Every folded projection query (one-shot; see [`Bundle::queries`]).
pub fn bundled_queries(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.queries()
}

/// Every folded cell/projection TTL (one-shot; see [`Bundle::cells`]).
pub fn bundled_cells(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.cells()
}

/// Every folded slice test-DSL spec (one-shot; see [`Bundle::tests`]).
pub fn bundled_tests(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.tests()
}

/// The reasoning report artifacts (one-shot; see [`Bundle::reasoning`]).
pub fn bundled_reasoning(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.reasoning()
}

/// Every folded OKF document (one-shot; see [`Bundle::okf`]).
pub fn bundled_okf(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.okf()
}

/// The full ontology-docs site (one-shot; see [`Bundle::ontology_docs`]).
pub fn bundled_ontology_docs(
    snapshot: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.ontology_docs()
}

/// The mdbook `src/` source tree (one-shot; see [`Bundle::docs_book`]).
pub fn bundled_docs_book(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.docs_book()
}

/// The print documentation projection (one-shot; see [`Bundle::docs_print`]).
pub fn bundled_docs_print(
    snapshot: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.docs_print()
}

/// Every folded SHACL shape (one-shot; see [`Bundle::shapes`]).
pub fn bundled_shapes(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.shapes()
}

/// Every folded compiled logic/DL projection (one-shot; see [`Bundle::axioms`]).
pub fn bundled_axioms(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.axioms()
}

/// The folded SHACL-derived schemas (one-shot; see [`Bundle::schemas`]).
pub fn bundled_schemas(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.schemas()
}

/// The bundled JSON Schema (one-shot; see [`Bundle::schema`]).
pub fn bundled_schema(snapshot: &[u8]) -> Result<Option<Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.schema()
}

/// The folded JSON-LD-star + YAML-LD-star serializations (one-shot; see
/// [`Bundle::yaml_ld`]).
pub fn bundled_yaml_ld(snapshot: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.yaml_ld()
}

/// The bundled JSON-LD-star serialization (one-shot; see [`Bundle::jsonld_star`]).
pub fn bundled_jsonld_star(snapshot: &[u8]) -> Result<Option<Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.jsonld_star()
}

/// Folded cell TTLs under `prefix` plus slice mappings (one-shot; see
/// [`Bundle::cells_under`]).
pub fn bundled_cells_under(
    snapshot: &[u8],
    prefix: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.cells_under(prefix)
}

/// Reconstruct merged-ontology N-Triples (one-shot; see [`Bundle::merged_ttl`]).
pub fn bundled_merged_ttl(
    snapshot: &[u8],
    include_imports: bool,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.merged_ttl(include_imports)
}

/// The precomputed saturation refusal set (one-shot; see [`Bundle::denied_cells`]).
pub fn bundled_denied_cells(snapshot: &[u8]) -> Result<Option<DeniedCells>, gmeow_errors::Diag> {
    Bundle::from_snapshot(snapshot)?.denied_cells()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The committed `generated/dist/gmeow.gts` snapshot bytes, read from the
    /// worktree (the CLI embeds these via `include_bytes!`; a test reads them off
    /// disk to exercise the real fold).
    fn committed_snapshot() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("generated/dist/gmeow.gts");
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// Mirrors `tests/test_bundle_blob_integrity.py`: the wheel-mode consumer
    /// archives are folded into gmeow.gts as blobs and resolve non-empty. This
    /// pins Rust↔producer rep-string agreement — a drifted label would silently
    /// resolve to `{}` and ship the bundle without that surface.
    #[test]
    fn bundle_carries_the_consumer_archives() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");
        assert!(
            !bundle.sssom().unwrap().is_empty(),
            "mappings-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.cells().unwrap().is_empty(),
            "cells-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.queries().unwrap().is_empty(),
            "queries-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.tests().unwrap().is_empty(),
            "tests-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.shapes().unwrap().is_empty(),
            "shapes-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.axioms().unwrap().is_empty(),
            "axioms-archive blob missing from gmeow.gts"
        );
        assert!(
            !bundle.reasoning().unwrap().is_empty(),
            "reasoning-archive blob missing from gmeow.gts"
        );
    }

    // (The former `span_table_rep_labels_agree` drift-pin test is gone: the producer
    // side now RE-EXPORTS `REP_SPAN_TABLE`/`REP_DIAG_NODES` from this module
    // (`crate::stages::carrier`), so producer and reader are one constant and the label
    // cannot drift structurally — a runtime assert_eq of a const against itself guards
    // nothing.)

    /// The two documentation-projection blobs (`docs-book`, `docs-print`) resolve
    /// NON-EMPTY off the committed bundle and carry their signature members. This
    /// pins Rust↔producer rep-string agreement for the print/book surfaces: a
    /// drifted `REP_DOCS_BOOK` / `REP_DOCS_PRINT` label would silently resolve to
    /// `{}` and ship the bundle without the print PDF or the mdbook source tree.
    #[test]
    fn docs_book_and_docs_print_resolve_non_empty() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");

        let book = bundle.docs_book().unwrap();
        assert!(!book.is_empty(), "docs-book blob missing from gmeow.gts");
        assert!(
            book.contains_key("x-gmeow-english/book.toml"),
            "docs-book carries the mdbook manifest (x-gmeow-english/book.toml)"
        );
        assert!(
            book.contains_key("x-gmeow-english/src/SUMMARY.md"),
            "docs-book carries the mdbook table of contents (x-gmeow-english/src/SUMMARY.md)"
        );

        let print = bundle.docs_print().unwrap();
        assert!(!print.is_empty(), "docs-print blob missing from gmeow.gts");
        let pdf = print
            .get("x-gmeow-english/gmeow.pdf")
            .expect("docs-print carries the print PDF (x-gmeow-english/gmeow.pdf)");
        assert!(
            pdf.starts_with(b"%PDF"),
            "docs-print gmeow.pdf begins with the %PDF magic (got {:?})",
            &pdf[..pdf.len().min(8)]
        );
        assert!(
            print.contains_key("x-gmeow-english/gmeow.typ"),
            "docs-print carries the deterministic Typst source (x-gmeow-english/gmeow.typ)"
        );
    }

    #[test]
    fn ontology_docs_and_schemas_resolve_non_empty() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");

        let docs = bundle.ontology_docs().unwrap();
        assert!(
            !docs.is_empty(),
            "ontology-docs blob missing from gmeow.gts"
        );
        assert!(
            docs.keys().any(|k| k.contains("x-gmeow-")),
            "ontology-docs members are language-tagged"
        );

        let schemas = bundle.schemas().unwrap();
        assert!(
            schemas.contains_key("gmeow.schema.json"),
            "schemas-archive carries the JSON Schema"
        );
        assert!(
            bundle.schema().unwrap().is_some(),
            "schema() resolves the JSON Schema payload"
        );
        assert!(
            bundle.jsonld_star().unwrap().is_some(),
            "yaml-ld-archive carries the JSON-LD-star serialization"
        );
    }

    #[test]
    fn merged_ttl_reconstructs_non_empty_ntriples() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");
        let base = bundle.merged_ttl(false).unwrap();
        assert!(!base.is_empty(), "merged N-Triples are non-empty");
        assert!(base.ends_with(b"\n"), "merged N-Triples end with a newline");
        // The guideBlob reference triples are filtered out of the merged graph —
        // i.e. no triple carries `gmeow:guideBlob` in PREDICATE position (the
        // property's own definitional triples, where it is a subject, survive, so
        // a bare substring check would be wrong).
        let text = String::from_utf8(base.clone()).expect("merged graph is UTF-8");
        let guide_predicate = format!(" <{}guideBlob> ", crate::gmeow_ns::GMEOW_NS);
        assert!(
            !text.lines().any(|line| line.contains(&guide_predicate)),
            "guideBlob reference triples are filtered from the merged graph"
        );
        // Including imports never drops assertions.
        let with_imports = bundle.merged_ttl(true).unwrap();
        assert!(with_imports.len() >= base.len());
    }

    #[test]
    fn absent_rep_resolves_to_empty_not_error() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");
        assert!(
            bundle.archive("no-such-rep-archive").unwrap().is_empty(),
            "an unknown rep resolves to an empty archive (wheel-only contract)"
        );
        assert!(bundle.blob_by_rep("no-such-rep").unwrap().is_none());
    }

    #[test]
    fn malformed_snapshot_is_a_hard_error() {
        assert!(Bundle::from_snapshot(b"not a valid gts snapshot").is_err());
    }

    #[test]
    fn is_slice_mapping_matches_five_segment_ttl() {
        assert!(is_slice_mapping("slices/core/inhabitation/mappings/x.ttl"));
        assert!(!is_slice_mapping("dsl/mappings/equivalences/x.ttl"));
        assert!(!is_slice_mapping("slices/core/inhabitation/shapes.ttl"));
        assert!(!is_slice_mapping("slices/a/b/mappings/x.rq"));
    }

    /// Regression pin for the scoping decision in [`Bundle::integrity_report`]:
    /// `gmeow:contentDigest` and `gmeow:definitionDigest` are domain-free
    /// content-hash/fingerprint predicates over the committed bundle (thousands
    /// of `gmeow:definitionDigest` triples alone), and legitimately carry
    /// `blake3:`-shaped literals with NO promise of a matching bundle blob —
    /// only properties whose local name ends in `Blob` are a dereference
    /// contract. Scanning by literal shape instead of by name previously
    /// flagged this committed bundle's real, correct data as thousands of
    /// false "dangling references".
    #[test]
    fn integrity_report_does_not_flag_fingerprint_predicates_as_references() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");
        let report = bundle.integrity_report().expect("integrity report");
        assert!(
            !report
                .referenced
                .contains_key(&format!("{GMEOW_NS}contentDigest")),
            "gmeow:contentDigest is a content-hash identity predicate, not a blob-store \
             reference — it must not appear in the referenced-digest map"
        );
        assert!(
            !report
                .referenced
                .contains_key(&format!("{GMEOW_NS}definitionDigest")),
            "gmeow:definitionDigest is a term-definition fingerprint, not a blob-store \
             reference — it must not appear in the referenced-digest map"
        );
    }

    /// Regression pin for the orphan-detection scoping: a stored blob that
    /// carries a producer-declared `rep` label (e.g. the SHACL findings/SARIF
    /// reports, the compiled shape surfaces) is a legitimate, intentionally
    /// shipped blob even when this module exposes no dedicated typed accessor
    /// for that particular rep, and even when no graph predicate references
    /// its digest by name. Only a blob with NEITHER a reference NOR a
    /// declared rep is a genuine orphan.
    #[test]
    fn integrity_report_does_not_flag_rep_labeled_blobs_as_orphans() {
        let snapshot = committed_snapshot();
        let bundle = Bundle::from_snapshot(&snapshot).expect("fold committed gmeow.gts");
        let report = bundle.integrity_report().expect("integrity report");
        let graph = bundle.view.graph();
        for orphan in &report.orphan_blobs {
            let has_rep = graph
                .blob_meta
                .iter()
                .any(|(d, meta)| d == orphan && blob_meta_rep(meta).is_some());
            assert!(
                !has_rep,
                "blob {orphan} carries a producer-declared rep label and must not be \
                 flagged as an orphan"
            );
        }
    }
}
