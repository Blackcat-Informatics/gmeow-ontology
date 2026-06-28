// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-stage content-addressed cache (#861 P2, #1132 C4-cache / 2b).
//!
//! The cache key folds `stage.id ++ impl_version ++ sorted(upstream output
//! digests) ++ source_file_digest[SourceLoad only]` into a [`content_digest`],
//! and `generated/.pipeline-cache/<version>/` (gitignored) maps key → a serialized
//! [`CachedBundle`], backed by the kernel `ContentStore`. It is self-verifying: a
//! digest recheck on load HARD-fails on mismatch and never silently repairs
//! (no-optionality).
//!
//! # The C4-cache: a canonical-projection / structural-reconstitution cache
//!
//! C4 swapped [`StageProduct`]'s carrier from a byte-map to a structured
//! [`PipelineBundle<PipelineHandle>`](crate::bundle::PipelineHandle) — and the
//! kernel bundle deliberately has NO serde (the oxigraph-/PyO3-free ring-fence).
//! The cache therefore persists the bundle's **canonical byte projection + a
//! per-lane manifest + the handle backing graphs**, NOT the live IR, and on a hit
//! **reconstitutes** a digest- and structure-equal bundle by parsing the dataset
//! ONCE at the cache boundary (the single sanctioned re-parse). Each lane:
//!
//! * **dataset** — its canonical N-Quads bytes via the production
//!   [`serialize_dataset`] egress; on load `parse_dataset` round-trips them back to
//!   an `Arc<RdfDataset>` (serialize/parse are inverses for the star-capable N-Quads
//!   codec, so the canonical hash is preserved). This is the ONLY re-parse.
//! * **lookaside** — a serde mirror of the kernel [`RdfLookaside`] (which has no
//!   serde): every resource and blob record the byte-artifact lane and later lanes
//!   rely on, reconstructed field-for-field on load. The kernel records carry no
//!   serde, so the mirror lives here in the pipeline crate.
//! * **blobs** — the [`ContentStore`] contents (digest hex → bytes), rebuilt with
//!   [`ContentStore::insert_checked`] so a corrupt blob HARD-fails on load.
//! * **provenance** — the S0.5-safe PUBLIC projection only (unit names+kinds,
//!   artifact paths, locations). Runtime `UnitId`/`ArtifactId`/`OriginSetId` are
//!   NEVER persisted; on load we re-register units/artifacts/occurrences from the
//!   persisted public rows so the reconstituted prov's `public_projection()` equals
//!   the persisted one (and thus the bundle digest is preserved). Exact
//!   occurrence→quad bindings are NOT reconstructed — by C9 the output-relevant
//!   provenance is projected into the dataset (round-trips via the dataset bytes);
//!   the sidecar is a runtime accumulator and only its public projection feeds the
//!   digest.
//! * **handles** — each `(graph_iri, HandleEntry)` persists its backing named-graph
//!   canonical bytes plus a tag for the [`PipelineHandle`] arm. On load each handle
//!   is re-derived from its backing graph and re-attached via `pin_handle`, so the
//!   digest-pin invariant is re-checked; a handle that fails to re-pin HARD-fails.
//!
//! # GREENFIELD cache version
//!
//! [`CACHE_VERSION`] is folded into BOTH the on-disk subdirectory and the manifest.
//! A version bump makes every prior cache (including the C4-spine byte-only
//! stand-in) a clean MISS — there is no migration path (greenfield).
//!
//! # On-disk layout
//!
//! `generated/.pipeline-cache/<version>/` holds an `index.json` mapping each stage
//! key to a blob digest, and `blobs/<digest>` holds the serialized [`CachedBundle`].
//! On load the blob is re-hashed and compared to the indexed digest — a mismatch is
//! a HARD failure, never a silent repair (no-optionality).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_rdf::provenance::{DatasetProvenance, OriginKind};
use gmeow_rdf::{
    canonicalize, parse_dataset, serialize_dataset, ContentDigest, ContentStore, QuadHandle,
    RdfBlobOrigin, RdfBlobRecord, RdfLocation, RdfLookaside, RdfLookasideKind,
    RdfLookasideResource, RdfMetadataValue, SerializeGraph,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bundle::PipelineHandle;
use crate::error::PipelineError;
use crate::node::StageProduct;

/// The GREENFIELD on-disk cache-shape revision. Folded into BOTH the cache
/// subdirectory and the [`CachedBundle`] manifest so a stale cache (e.g. the C4-spine
/// byte-only stand-in, version-less or an older rev) is treated as a clean MISS, not
/// mis-decoded. Bump on ANY change to the persisted shape (no migration path).
pub const CACHE_VERSION: u32 = 2;

/// The media type of the dataset's canonical byte projection. N-Quads is
/// star-capable (carries the full RDF-1.2 statement layer) and `serialize_dataset` /
/// `parse_dataset` are inverses for it, so the round-tripped dataset is canonical-equal.
const DATASET_MEDIA_TYPE: &str = "application/n-quads";

/// Compute a hex SHA-256 over a sequence of byte fields, each length-free but
/// unit-separated, so the digest is unambiguous and order-sensitive.
pub fn content_digest(fields: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field);
        hasher.update(b"\x1f");
    }
    let bytes = hasher.finalize();
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The per-stage cache key: stage id + impl version + the sorted upstream output
/// digests (Merkle composition). `source_file_digest` is folded only for
/// `SourceLoad` stages (their inputs are files, not upstream products).
pub fn stage_key(
    stage_id: &str,
    impl_version: &str,
    upstream_digests_sorted: &[String],
    source_file_digest: Option<&str>,
) -> String {
    let mut fields: Vec<&[u8]> = vec![stage_id.as_bytes(), impl_version.as_bytes()];
    for d in upstream_digests_sorted {
        fields.push(d.as_bytes());
    }
    if let Some(src) = source_file_digest {
        fields.push(b"source");
        fields.push(src.as_bytes());
    }
    content_digest(&fields)
}

// ── The serde bundle manifest ────────────────────────────────────────────────

/// The serde-able mirror of a [`PipelineBundle<PipelineHandle>`] sufficient to
/// reconstruct a digest- and structure-equal bundle. The kernel bundle has no serde
/// (ring-fence), so this pipeline-side manifest captures every lane the bundle uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBundle {
    /// The on-disk cache-shape revision; a mismatch is a clean miss (greenfield).
    version: u32,
    /// The producing stage id.
    stage_id: String,
    /// The cached cache-key digest (the `StageProduct::digest`, which for a real
    /// product equals `bundle.digest().to_hex()` but may be decoupled for abstract
    /// products — so it is persisted explicitly).
    digest: String,
    /// The dataset's canonical N-Quads byte projection (the ONLY re-parse on load).
    dataset_nquads: Vec<u8>,
    /// The lookaside mirror: resources + blob records (the byte-artifact lane and
    /// later typed sidecar lanes ride here).
    lookaside: CachedLookaside,
    /// The content store: blob digest hex → payload bytes (rebuilt via
    /// `insert_checked`, so a corrupt blob hard-fails on load).
    blobs: BTreeMap<String, Vec<u8>>,
    /// The S0.5 PUBLIC provenance projection rows `(unit, kind, artifact, location)`.
    /// NEVER the runtime numeric ids.
    provenance: Vec<CachedProvRow>,
    /// The typed-handle lane: each backing graph + its arm tag + canonical bytes.
    handles: Vec<CachedHandle>,
}

/// A serde mirror of [`RdfLookaside`]. Only the lanes the pipeline bundle populates
/// (resources, blobs) are mirrored; the remaining kernel lanes (metadata, segments,
/// suppressions, opaque nodes, signatures) are NOT used by any pipeline stage and
/// are asserted empty at persist time — a populated one HARD-fails rather than
/// silently dropping (no-optionality), signalling the mirror must grow first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedLookaside {
    resources: Vec<CachedResource>,
    blobs: Vec<CachedBlobRecord>,
}

/// A serde mirror of [`RdfLookasideResource`]. The byte-artifact lane sets
/// `kind`/`name`/`content_digest`; the remaining string fields are mirrored in full
/// so any resource the bundle carries round-trips field-for-field. The `metadata`
/// and `location` fields are not used by current lanes and are asserted empty at
/// persist time (hard-fail if populated).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedResource {
    kind: String,
    iri: Option<String>,
    name: Option<String>,
    graph_name: Option<String>,
    media_type: Option<String>,
    content_digest: Option<String>,
    path: Option<String>,
}

/// A serde mirror of [`RdfBlobRecord`] (the by-reference blob lane). `metadata` is
/// not used by current lanes and is asserted empty at persist time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedBlobRecord {
    digest: String,
    media_type: Option<String>,
    representation: Option<String>,
    decoded_len: Option<usize>,
    origin_segments: Option<Vec<String>>,
}

/// One public-projection provenance row `(unit_name, kind, artifact_path, location)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProvRow {
    unit: String,
    kind: String,
    artifact: String,
    location: Option<String>,
}

/// A persisted typed handle: the backing graph IRI, the [`PipelineHandle`] arm tag,
/// and the backing named-graph canonical bytes the payload is re-derived from.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHandle {
    /// The named-graph IRI the handle backs (the [`HandleKey`](gmeow_rdf::HandleKey)).
    graph: String,
    /// The [`PipelineHandle`] arm tag (see [`handle_arm_tag`]).
    arm: String,
    /// The canonical N-Quads bytes of the backing named graph (the sub-dataset the
    /// placeholder arms wrap). On load the payload is re-derived from these and
    /// `pin_handle` re-checks the pinned digest against the live dataset.
    graph_nquads: Vec<u8>,
}

/// The stable arm tag for a [`PipelineHandle`] variant (persisted with each handle).
fn handle_arm_tag(handle: &PipelineHandle) -> &'static str {
    match handle {
        PipelineHandle::Logic(_) => "logic",
        PipelineHandle::Reasoning(_) => "reasoning",
        PipelineHandle::RelationalCore(_) => "relational-core",
        PipelineHandle::Correspondence(_) => "correspondence",
    }
}

/// Re-derive a [`PipelineHandle`] of the given arm tag from its backing sub-dataset.
///
/// The C4 placeholder arms (`reasoning` / `relational-core` / `correspondence`) wrap
/// the backing `Arc<RdfDataset>` directly. The `logic` arm carries the REAL typed IR
/// (#1132 C6): its backing graph is the canonical RDF-1.2 projection of a
/// [`LogicProgram`], so on a cache hit the program is RE-DERIVED from that backing
/// graph via the reverse parser ([`parse_logic_dataset`]) — the consumer never
/// re-parses the logic graph itself, the cache boundary does it ONCE here. A parse
/// failure HARD-fails (no-optionality): a `logic` handle whose backing graph no longer
/// parses to a program is a corrupt cache, never a silently-dropped handle.
fn rebuild_handle(
    arm: &str,
    graph: Arc<gmeow_rdf::RdfDataset>,
) -> Result<PipelineHandle, PipelineError> {
    Ok(match arm {
        "logic" => {
            let (program, _diags) =
                gmeow_logic_compile::frontend::parse_logic_dataset(graph.as_ref(), None).map_err(
                    |e| {
                        PipelineError::Decode(format!(
                            "cache: re-derive Logic handle program from backing graph/logic: {}",
                            e.0
                        ))
                    },
                )?;
            PipelineHandle::Logic(Arc::new(program))
        }
        "reasoning" => PipelineHandle::Reasoning(graph),
        "relational-core" => PipelineHandle::RelationalCore(graph),
        "correspondence" => PipelineHandle::Correspondence(graph),
        other => {
            return Err(PipelineError::Decode(format!(
                "cached handle has unknown PipelineHandle arm tag {other:?}"
            )))
        }
    })
}

/// Map an [`OriginKind`] public string back to the kind. Greenfield: an unknown
/// string HARD-fails — the public projection only emits the closed set, and a
/// "unknown-kind" marker means a forged provenance the cache must not reconstruct.
fn origin_kind_from_str(kind: &str) -> Result<OriginKind, PipelineError> {
    Ok(match kind {
        "source" => OriginKind::Source,
        "root-ontology" => OriginKind::RootOntology,
        "import" => OriginKind::Import,
        "generated" => OriginKind::Generated,
        "runtime-input" => OriginKind::RuntimeInput,
        other => {
            return Err(PipelineError::Decode(format!(
                "cached provenance row carries an unrepresentable origin kind {other:?}"
            )))
        }
    })
}

impl CachedLookaside {
    /// Mirror a kernel [`RdfLookaside`], HARD-failing if it carries a lane this
    /// mirror does not yet model (no silent loss).
    fn from_lookaside(la: &RdfLookaside) -> Result<Self, PipelineError> {
        if !la.metadata.is_empty()
            || !la.segments.is_empty()
            || !la.suppressions.is_empty()
            || !la.opaque_nodes.is_empty()
            || !la.signatures.is_empty()
        {
            return Err(PipelineError::Decode(
                "pipeline bundle lookaside carries a lane (metadata/segments/suppressions/\
                 opaque-nodes/signatures) the C4 cache mirror does not yet model — grow the \
                 mirror before persisting it (no silent loss)"
                    .to_string(),
            ));
        }
        let resources = la
            .resources
            .iter()
            .map(CachedResource::from_resource)
            .collect::<Result<Vec<_>, _>>()?;
        let blobs = la
            .blobs
            .iter()
            .map(CachedBlobRecord::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { resources, blobs })
    }

    /// Reconstruct the exact kernel [`RdfLookaside`] from this mirror.
    fn into_lookaside(self) -> RdfLookaside {
        RdfLookaside {
            resources: self
                .resources
                .into_iter()
                .map(CachedResource::into_resource)
                .collect(),
            blobs: self
                .blobs
                .into_iter()
                .map(CachedBlobRecord::into_record)
                .collect(),
            ..RdfLookaside::default()
        }
    }
}

impl CachedResource {
    fn from_resource(r: &RdfLookasideResource) -> Result<Self, PipelineError> {
        if !r.metadata.is_empty() || r.location.is_some() {
            return Err(PipelineError::Decode(
                "pipeline lookaside resource carries metadata/location the C4 cache mirror \
                 does not yet model — grow the mirror before persisting it (no silent loss)"
                    .to_string(),
            ));
        }
        Ok(Self {
            kind: r.kind.as_str().to_string(),
            iri: r.iri.clone(),
            name: r.name.clone(),
            graph_name: r.graph_name.clone(),
            media_type: r.media_type.clone(),
            content_digest: r.content_digest.clone(),
            path: r.path.clone(),
        })
    }

    fn into_resource(self) -> RdfLookasideResource {
        // `from_hint` resolves the canonical kind string (incl. `Other(_)` for an
        // unknown domain) so the kind round-trips exactly.
        RdfLookasideResource {
            kind: RdfLookasideKind::from_hint(&self.kind),
            iri: self.iri,
            name: self.name,
            graph_name: self.graph_name,
            media_type: self.media_type,
            content_digest: self.content_digest,
            path: self.path,
            location: None::<RdfLocation>,
            metadata: BTreeMap::<String, RdfMetadataValue>::new(),
        }
    }
}

impl CachedBlobRecord {
    fn from_record(r: &RdfBlobRecord) -> Result<Self, PipelineError> {
        if !r.metadata.is_empty() {
            return Err(PipelineError::Decode(
                "pipeline lookaside blob record carries metadata the C4 cache mirror does not \
                 yet model — grow the mirror before persisting it (no silent loss)"
                    .to_string(),
            ));
        }
        Ok(Self {
            digest: r.digest.clone(),
            media_type: r.media_type.clone(),
            representation: r.representation.clone(),
            decoded_len: r.decoded_len,
            origin_segments: r.origin.as_ref().map(|o| o.source_segments.clone()),
        })
    }

    fn into_record(self) -> RdfBlobRecord {
        RdfBlobRecord {
            digest: self.digest,
            media_type: self.media_type,
            representation: self.representation,
            decoded_len: self.decoded_len,
            metadata: BTreeMap::new(),
            origin: self
                .origin_segments
                .map(|source_segments| RdfBlobOrigin { source_segments }),
        }
    }
}

impl CachedBundle {
    /// Project a [`StageProduct`] into its serde manifest (every lane captured).
    fn from_product(product: &StageProduct) -> Result<Self, PipelineError> {
        let bundle = product.bundle();

        // dataset → canonical N-Quads bytes (the production egress; the load-time
        // re-parse is the sole sanctioned re-parse).
        let dataset_nquads = serialize_dataset(
            bundle.dataset(),
            DATASET_MEDIA_TYPE,
            SerializeGraph::Dataset,
        )
        .map_err(|e| PipelineError::Decode(format!("cache: serialize bundle dataset: {e}")))?;

        let lookaside = CachedLookaside::from_lookaside(bundle.lookaside())?;

        // blobs → digest hex → bytes.
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (digest, bytes) in bundle.blobs().iter() {
            blobs.insert(digest.to_hex(), bytes.clone());
        }

        // provenance → PUBLIC projection rows only.
        let provenance = bundle
            .provenance()
            .public_projection()
            .into_iter()
            .map(|(unit, kind, artifact, location)| CachedProvRow {
                unit,
                kind,
                artifact,
                location,
            })
            .collect();

        // handles → backing-graph canonical bytes + arm tag, sorted by graph IRI
        // (BTreeMap iteration is already sorted, so the manifest is deterministic).
        let mut handles = Vec::with_capacity(bundle.handles.len());
        for (graph, entry) in &bundle.handles {
            let subgraph = project_named_graph(bundle.dataset(), graph);
            let graph_nquads =
                serialize_dataset(&subgraph, DATASET_MEDIA_TYPE, SerializeGraph::Dataset).map_err(
                    |e| {
                        PipelineError::Decode(format!("cache: serialize handle backing graph: {e}"))
                    },
                )?;
            handles.push(CachedHandle {
                graph: graph.clone(),
                arm: handle_arm_tag(&entry.payload).to_string(),
                graph_nquads,
            });
        }

        Ok(Self {
            version: CACHE_VERSION,
            stage_id: product.stage_id.clone(),
            digest: product.digest.clone(),
            dataset_nquads,
            lookaside,
            blobs,
            provenance,
            handles,
        })
    }

    /// Reconstitute a digest- and structure-equal [`StageProduct`] from the manifest.
    fn into_product(self) -> Result<StageProduct, PipelineError> {
        if self.version != CACHE_VERSION {
            // A version-mismatched manifest is a clean miss handled by the caller; a
            // mismatch reaching here means a tampered/forged blob — hard-fail.
            return Err(PipelineError::Decode(format!(
                "cached bundle version {} != expected {CACHE_VERSION}",
                self.version
            )));
        }

        // dataset: the ONE sanctioned re-parse.
        let dataset = parse_dataset(&self.dataset_nquads, DATASET_MEDIA_TYPE, None)
            .map_err(|e| PipelineError::Parse(format!("cache: re-parse bundle dataset: {e}")))?;

        let lookaside = self.lookaside.into_lookaside();

        // blobs: rebuild via insert_checked so a corrupt blob HARD-fails.
        let mut store = ContentStore::new();
        for (hex, bytes) in self.blobs {
            let digest = ContentDigest::from_hex(&hex).ok_or_else(|| {
                PipelineError::Decode(format!("cache: malformed blob digest hex {hex:?}"))
            })?;
            store
                .insert_checked(digest, bytes)
                .map_err(|e| PipelineError::CacheMismatch {
                    expected: hex.clone(),
                    actual: format!("{e}"),
                })?;
        }

        // provenance: re-register units/artifacts/occurrences so the reconstituted
        // public projection equals the persisted one. Quad bindings are NOT
        // reconstructed (only the public projection feeds the digest); each occurrence
        // is bound to a placeholder quad handle, which never enters the projection.
        let mut provenance = DatasetProvenance::new();
        for row in &self.provenance {
            let kind = origin_kind_from_str(&row.kind)?;
            let unit = provenance.register_unit(row.unit.clone(), kind);
            let artifact = provenance.register_artifact(row.artifact.clone());
            provenance.record_occurrence(
                QuadHandle::from_index(0),
                unit,
                artifact,
                row.location.clone(),
            );
        }

        // Assemble the bundle, then re-pin every handle (re-checks the digest invariant).
        let mut bundle = PipelineBundleAlias::new(dataset, lookaside, Arc::new(store), provenance);
        for h in self.handles {
            let subgraph =
                parse_dataset(&h.graph_nquads, DATASET_MEDIA_TYPE, None).map_err(|e| {
                    PipelineError::Parse(format!("cache: re-parse handle backing graph: {e}"))
                })?;
            // Pin against the canonical digest of the PERSISTED backing bytes (the
            // sub-dataset just re-parsed). `pin_handle` then checks this against the
            // LIVE named graph of the reconstituted dataset — if the persisted backing
            // bytes were tampered (so the handle would project a graph that disagrees
            // with the dataset it rides), the re-pin HARD-fails rather than silently
            // attaching a stale handle (no-optionality).
            let pinned = ContentDigest::of(canonicalize(&subgraph).nquads.as_bytes());
            let payload = rebuild_handle(&h.arm, subgraph)?;
            bundle
                .pin_handle(h.graph.clone(), payload, pinned)
                .map_err(|e| {
                    PipelineError::Decode(format!(
                        "cache: re-pin handle for <{}> failed: {e}",
                        h.graph
                    ))
                })?;
        }

        let mut product = StageProduct::from_bundle(self.stage_id, Arc::new(bundle));
        // Restore the explicit cached digest (abstract/test products carry a digest
        // decoupled from the carrier; a real product's bundle.digest() equals it).
        product.digest = self.digest;
        Ok(product)
    }
}

/// The pipeline bundle alias the cache reconstitutes (`PipelineBundle<PipelineHandle>`).
type PipelineBundleAlias = gmeow_rdf::PipelineBundle<PipelineHandle>;

/// Project one named graph of `dataset` into a fresh default-graph dataset whose
/// canonical bytes are the handle's backing graph — mirrors the kernel
/// `PipelineBundle::graph_digest` projection so the persisted bytes re-derive the
/// same sub-dataset on load.
fn project_named_graph(dataset: &gmeow_rdf::RdfDataset, graph: &str) -> gmeow_rdf::RdfDataset {
    use gmeow_rdf::{RdfDatasetBuilder, RdfTerm};
    let mut builder = RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        let in_graph = matches!(&quad.graph_name, Some(RdfTerm::Iri(iri)) if iri == graph);
        if !in_graph {
            continue;
        }
        let mut projected = quad.clone();
        projected.graph_name = None;
        builder.push_owned_quad(&projected);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    Arc::try_unwrap(
        builder
            .freeze()
            .expect("a sub-projection of a valid dataset is valid"),
    )
    .unwrap_or_else(|arc| gmeow_rdf::RdfDataset::union(&[&*arc]))
}

// ── On-disk content-addressed cache ──────────────────────────────────────────

/// The persistent per-stage cache under `generated/.pipeline-cache/<version>/`
/// (gitignored).
///
/// `index.json` maps `stage_key → blob ContentDigest (hex)`; `blobs/<hex>` holds
/// the serialized [`CachedBundle`]. Reads re-hash the blob and HARD-fail on a
/// digest mismatch (self-verifying, no silent repair). The `<version>` segment makes
/// a prior cache-shape revision a clean miss (greenfield, no migration).
pub struct PipelineCache {
    dir: PathBuf,
    index: BTreeMap<String, String>,
}

impl PipelineCache {
    /// The conventional cache base directory under a repo root. [`open`](Self::open)
    /// appends the version segment, so this is the un-segmented base.
    pub fn default_dir(root: &Path) -> PathBuf {
        root.join("generated").join(".pipeline-cache")
    }

    /// Open (or create) the cache rooted at `dir`, loading its index. The on-disk
    /// store lives under a `v<CACHE_VERSION>` leaf of `dir` so a prior cache-shape
    /// rev is isolated — a shape bump makes every older cache a clean miss
    /// (greenfield, no migration).
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, PipelineError> {
        let dir = dir.into().join(format!("v{CACHE_VERSION}"));
        fs::create_dir_all(dir.join("blobs"))?;
        let index_path = dir.join("index.json");
        let index: BTreeMap<String, String> = if index_path.exists() {
            let bytes = fs::read(&index_path)?;
            serde_json::from_slice(&bytes)
                .map_err(|e| PipelineError::Decode(format!("corrupt pipeline cache index: {e}")))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { dir, index })
    }

    /// Look up a stage product by cache key. Returns `None` on a miss. HARD-fails
    /// (`CacheMismatch`) if the blob exists but its re-hashed digest disagrees
    /// with the index — the cache is never silently repaired.
    pub fn get(&self, stage_key: &str) -> Result<Option<StageProduct>, PipelineError> {
        let Some(digest_hex) = self.index.get(stage_key) else {
            return Ok(None);
        };
        let blob_path = self.dir.join("blobs").join(digest_hex);
        if !blob_path.exists() {
            // Index references a missing blob: a corrupt cache, not a clean miss.
            return Err(PipelineError::CacheMismatch {
                expected: digest_hex.clone(),
                actual: "<missing blob>".to_string(),
            });
        }
        let bytes = fs::read(&blob_path)?;
        let actual = ContentDigest::of(&bytes).to_hex();
        if &actual != digest_hex {
            return Err(PipelineError::CacheMismatch {
                expected: digest_hex.clone(),
                actual,
            });
        }
        let cached: CachedBundle = serde_json::from_slice(&bytes)
            .map_err(|e| PipelineError::Decode(format!("corrupt cached bundle: {e}")))?;
        // A version-mismatched manifest is treated as a clean MISS (greenfield): the
        // entry belongs to a prior shape rev and must not be mis-decoded.
        if cached.version != CACHE_VERSION {
            return Ok(None);
        }
        Ok(Some(cached.into_product()?))
    }

    /// Store a stage product under `stage_key`, persisting the blob and index.
    pub fn put(&mut self, stage_key: &str, product: &StageProduct) -> Result<(), PipelineError> {
        let manifest = CachedBundle::from_product(product)?;
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|e| PipelineError::Decode(format!("cannot serialize cached bundle: {e}")))?;
        let digest_hex = ContentDigest::of(&bytes).to_hex();
        write_atomic(&self.dir.join("blobs").join(&digest_hex), &bytes)?;
        self.index.insert(stage_key.to_string(), digest_hex);
        self.persist_index()?;
        Ok(())
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn persist_index(&self) -> Result<(), PipelineError> {
        // Deterministic: BTreeMap serializes in sorted key order.
        let bytes = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| PipelineError::Decode(format!("cannot serialize cache index: {e}")))?;
        write_atomic(&self.dir.join("index.json"), &bytes)?;
        Ok(())
    }
}

/// Write `bytes` to `target` atomically: stage them in a sibling temp file in the
/// SAME directory (so the final `rename` stays on one filesystem, where POSIX
/// guarantees atomicity), then rename over the target. An interrupted write can
/// only ever leave a stray temp file, never a half-written `target` — so the
/// cache is never bricked mid-write (no-optionality, #861 P2).
fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), PipelineError> {
    let dir = target.parent().ok_or_else(|| {
        PipelineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cache path {} has no parent directory", target.display()),
        ))
    })?;
    // A per-target temp name keeps concurrent writers from clobbering each other's
    // staging file; the final atomic rename still resolves the last-writer-wins.
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{file_name}.tmp.{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram};
    use gmeow_rdf::{PipelineBundle, RdfDatasetBuilder, RdfTerm, TermId};

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    const GRAPH_IRI: &str = "http://example.org/graph";

    /// A tiny but real [`LogicProgram`] whose canonical RDF-1.2 projection is an EXACT
    /// graph round-trip: only `rdf:type → logic:Class` axioms (the form the reverse
    /// parser re-extracts) and a `None` source (out-of-graph provenance the canonical
    /// graph does not carry). Its projection backs the cache's `Logic` handle, so the
    /// cache's re-derivation (`parse_logic_dataset(graph, None)`) reconstructs a
    /// canonical-key-equal program.
    fn sample_logic_program() -> LogicProgram {
        let ax = |s: &str, o: &str| {
            LogicAxiom::new(
                s,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                o,
                false,
                false,
                ContextualScope::default(),
            )
            .expect("valid axiom")
        };
        LogicProgram::new(
            vec![
                ax(
                    "https://blackcatinformatics.ca/gmeow/Animal",
                    "https://blackcatinformatics.ca/logic/Kind",
                ),
                ax(
                    "https://blackcatinformatics.ca/gmeow/Cat",
                    "https://blackcatinformatics.ca/logic/Subkind",
                ),
            ],
            vec![],
            vec![],
            None,
        )
    }

    /// A non-trivial dataset: one default-graph quad plus the canonical RDF-1.2
    /// projection of [`sample_logic_program`] folded into named graph [`GRAPH_IRI`]
    /// (so the attached `Logic` handle has a real, re-derivable backing graph).
    fn dataset_with_named_graph() -> Arc<gmeow_rdf::RdfDataset> {
        let arts = gmeow_logic_compile::projections::compile_program(&sample_logic_program())
            .expect("compile sample program");
        let logic_ds = parse_dataset(arts.canonical_rdf12.as_bytes(), "text/turtle", None)
            .expect("parse canonical rdf12");

        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None); // a default-graph quad
                                    // Fold every triple of the logic projection into the named graph GRAPH_IRI.
        let graph = RdfTerm::Iri(GRAPH_IRI.to_owned());
        for quad in logic_ds.owned_quads() {
            let mut routed = quad.clone();
            routed.graph_name = Some(graph.clone());
            b.push_owned_quad(&routed);
        }
        b.freeze().expect("valid")
    }

    /// Build a richly populated bundle: dataset (≥1 named graph), a lookaside Blob
    /// resource + matching blob, a populated provenance, and one attached handle.
    fn rich_bundle() -> PipelineBundle<PipelineHandle> {
        let dataset = dataset_with_named_graph();

        // A byte-artifact-lane Blob resource + matching content-store blob.
        let mut blobs = ContentStore::new();
        let blob_digest = blobs.insert(b"artifact-bytes".to_vec());
        let mut lookaside = RdfLookaside::default();
        lookaside.resources.push(
            RdfLookasideResource::new(RdfLookasideKind::Blob)
                .with_name("generated/x.ttl")
                .with_digest(blob_digest.to_hex()),
        );

        // A populated provenance with a registered unit + an occurrence.
        let mut prov = DatasetProvenance::new();
        let unit = prov.register_unit("slices/core/epistemics", OriginKind::Source);
        let artifact = prov.register_artifact("slices/core/epistemics/epistemics.ttl");
        prov.record_occurrence(
            QuadHandle::from_index(0),
            unit,
            artifact,
            Some("epistemics.ttl:1".to_owned()),
        );

        let mut bundle = PipelineBundle::new(dataset, lookaside, Arc::new(blobs), prov);

        // Attach the REAL typed Logic handle (#1132 C6) over the named graph: the
        // payload is the compiled program, pinned to the canonical digest of its
        // backing `graph/logic` projection.
        let program = Arc::new(sample_logic_program());
        let pinned = bundle.graph_digest(GRAPH_IRI);
        bundle
            .pin_handle(GRAPH_IRI, PipelineHandle::Logic(program), pinned)
            .expect("pin handle over the named graph");
        bundle
    }

    fn canon_hex(ds: &gmeow_rdf::RdfDataset) -> String {
        ContentDigest::of(canonicalize(ds).nquads.as_bytes()).to_hex()
    }

    #[test]
    fn cached_bundle_structural_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let original = rich_bundle();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(original.clone()));
        cache.put("k", &product).unwrap();

        let got = cache.get("k").unwrap().expect("cache hit");
        let recon = got.bundle();

        // dataset: canonical hash equal.
        assert_eq!(
            canon_hex(recon.dataset()),
            canon_hex(original.dataset()),
            "dataset canonical hash preserved"
        );

        // lookaside: every resource + blob record equal.
        assert_eq!(
            recon.lookaside(),
            original.lookaside(),
            "lookaside reconstructed field-for-field"
        );

        // blobs: every digest → bytes equal.
        assert_eq!(recon.blobs(), original.blobs(), "blob store preserved");

        // handles: re-derived, present, and still pin-valid.
        assert_eq!(recon.handles.len(), original.handles.len(), "handle count");
        let entry = recon.handle(GRAPH_IRI).expect("handle re-attached");
        let PipelineHandle::Logic(reconstituted) = &entry.payload else {
            panic!("handle arm preserved (Logic)");
        };
        // The REAL typed Logic handle (#1132 C6) re-derives from its backing
        // `graph/logic` projection to a canonical-key-equal program.
        assert_eq!(
            reconstituted.canonical_key(),
            sample_logic_program().canonical_key(),
            "the cache re-derived the Logic handle's program canonical-key-equal"
        );
        // The pinned digest matches the LIVE backing graph (pin_handle re-checked it).
        assert_eq!(
            entry.content_digest,
            recon.graph_digest(GRAPH_IRI),
            "handle pin matches the reconstituted graph"
        );

        // provenance: public projection equal.
        assert_eq!(
            recon.provenance().public_projection(),
            original.provenance().public_projection(),
            "public provenance projection preserved"
        );

        // digest: bundle content fold equal.
        assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");

        // byte-artifact lane: reproduced byte-for-byte.
        assert_eq!(
            got.artifacts(),
            product.artifacts(),
            "byte-artifact lane reproduced exactly"
        );
        // The product's cache-key digest is preserved too.
        assert_eq!(got.digest, product.digest, "stage-product digest preserved");
    }

    #[test]
    fn tampered_handle_backing_graph_hard_fails_on_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        cache.put("k", &product).unwrap();

        // Tamper the persisted manifest's handle backing graph so the re-derived
        // payload no longer matches the named graph the dataset carries: the handle
        // must FAIL to re-pin (drop/stale a handle = hard failure, never silent).
        let blobs_dir = cache.dir.join("blobs");
        let blob_path = std::fs::read_dir(&blobs_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = std::fs::read(&blob_path).unwrap();
        let mut manifest: CachedBundle = serde_json::from_slice(&bytes).unwrap();
        // Replace the handle's backing-graph bytes with a DIFFERENT graph's canon.
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "x"), iri(&mut b, "y"), iri(&mut b, "z"));
        b.push_quad(s, p, o, None);
        let other = b.freeze().unwrap();
        manifest.handles[0].graph_nquads =
            serialize_dataset(&other, DATASET_MEDIA_TYPE, SerializeGraph::Dataset).unwrap();
        // Re-serialize + re-file under the NEW content digest (and re-point the index),
        // so the blob still self-verifies and we exercise the handle re-pin path.
        let new_bytes = serde_json::to_vec(&manifest).unwrap();
        std::fs::remove_file(&blob_path).unwrap();
        let new_hex = ContentDigest::of(&new_bytes).to_hex();
        std::fs::write(blobs_dir.join(&new_hex), &new_bytes).unwrap();
        let mut reopened = PipelineCache::open(dir.path()).unwrap();
        reopened.index.insert("k".to_string(), new_hex);
        reopened.persist_index().unwrap();

        let err = reopened
            .get("k")
            .expect_err("a stale/dropped handle must hard-fail");
        assert!(
            matches!(err, PipelineError::Decode(_)),
            "tampered handle backing graph fails to re-pin (hard fail), got {err:?}"
        );
    }
}
