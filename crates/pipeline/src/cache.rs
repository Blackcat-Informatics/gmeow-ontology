// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-stage content-addressed cache (C4-cache).
//!
//! The cache key folds `stage.id ++ impl_version ++ sorted(upstream output
//! digests) ++ source_file_digest[SourceLoad only]` into a [`content_digest`],
//! and `.cache/gmeow-sync/pipeline/<version>/` (gitignored) maps key → a serialized
//! [`CachedBundle`], backed by the kernel `ContentStore`. It is self-verifying: a
//! digest recheck on load HARD-fails on mismatch and never silently repairs
//! (no-optionality).
//!
//! # The C4-cache: a canonical-projection / structural-reconstitution cache
//!
//! C4 swapped [`StageProduct`]'s carrier from a byte-map to a structured
//! [`PipelineBundle<PipelineHandle>`](crate::bundle::PipelineHandle) — and the
//! kernel bundle deliberately has NO serde (the oxigraph-/PyO3-free ring-fence).
//! The cache therefore persists the bundle's **packed IR + a per-lane manifest**
//! and on a hit **reconstitutes** a digest- and structure-equal bundle without an
//! RDF text serialization/parsing detour. Each lane:
//!
//! * **dataset** — a deterministic `PURRPCK1` image via [`PackBuilder`]; on load
//!   [`restore_pack`] reconstructs the complete indexed RDF 1.2 dataset (base
//!   quads, reifiers, and annotations) directly from the packed dictionary and
//!   side tables.
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
//!   the persisted one (and thus the bundle digest is preserved). Each occurrence's
//!   asserted-quad ordinal IS restored from its persisted public row so the
//!   reconstituted public projection — which carries that ordinal — matches exactly;
//!   the full quad content is not rebound (the output-relevant provenance is also
//!   projected into the dataset and round-trips via the dataset bytes). The sidecar
//!   is a runtime accumulator and only its public projection feeds the digest.
//! * **handles** — each `(graph_iri, HandleEntry)` persists only its graph IRI and
//!   [`PipelineHandle`] arm tag. On load the backing graph is projected from the
//!   restored dataset, eliminating the previous duplicate graph serialization;
//!   each handle is re-derived and re-attached via `pin_handle`, so the digest-pin
//!   invariant is re-checked and a handle that fails to re-pin HARD-fails.
//!
//! # GREENFIELD cache version
//!
//! [`CACHE_VERSION`] is folded into BOTH the on-disk subdirectory and the manifest.
//! A version bump makes every prior cache (including the C4-spine byte-only
//! stand-in) a clean MISS — there is no migration path (greenfield).
//!
//! # On-disk layout
//!
//! `.cache/gmeow-sync/pipeline/<version>/` holds an `index.json` mapping each stage
//! key to a blob digest, and `blobs/<digest>` holds the bincode-serialized
//! [`CachedBundle`]. The binary encoding keeps the manifest's large `Vec<u8>` lanes
//! byte-dense instead of expanding every byte into a JSON number. On load the blob
//! is re-hashed and compared to the indexed digest — a mismatch is a HARD failure,
//! never a silent repair (no-optionality).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::provenance::{DatasetProvenance, OriginKind};
use purrdf::{
    ContentDigest, ContentStore, PackBuilder, QuadHandle, RdfBlobOrigin, RdfBlobRecord,
    RdfLocation, RdfLookaside, RdfLookasideKind, RdfLookasideResource, RdfMetadataValue,
    SerializeGraph, canonicalize, restore_pack, serialize_dataset,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bundle::PipelineHandle;
use crate::node::StageProduct;

/// The GREENFIELD on-disk cache-shape revision. Folded into BOTH the cache
/// subdirectory and the [`CachedBundle`] manifest so a stale cache (e.g. the C4-spine
/// byte-only stand-in, version-less or an older rev) is treated as a clean MISS, not
/// mis-decoded. Bump on ANY change to the persisted shape (no migration path).
pub const CACHE_VERSION: u32 = 5;

/// The text projection used only by the Reasoning handle's legacy reverse parser.
/// Dataset persistence itself uses `PURRPCK1` and never passes through this codec.
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

/// The build fingerprint folded into every [`stage_key`]: a content hash of the whole
/// workspace Rust source + `Cargo.lock` + the `rustc` version, computed by `build.rs`.
/// Any code, dependency, or toolchain change changes it, so the persistent per-stage
/// cache invalidates fail-closed — there is no `impl_version` to forget to bump.
pub const BUILD_FINGERPRINT: &str = env!("GMEOW_BUILD_FINGERPRINT");

/// The per-stage cache key: build fingerprint + stage id + impl version + the sorted
/// upstream output digests (Merkle composition). `source_file_digest` is folded only
/// for `SourceLoad` stages (their inputs are files, not upstream products).
///
/// Folding [`BUILD_FINGERPRINT`] makes the key capture the producing CODE, not just
/// its declared `impl_version`: a stage whose Rust impl changed (here or in any
/// workspace crate it calls, e.g. `gmeow-logic`) gets a fresh key and recomputes,
/// so a persistent cache can never serve a stale pre-change product.
pub fn stage_key(
    stage_id: &str,
    impl_version: &str,
    upstream_digests_sorted: &[String],
    source_file_digest: Option<&str>,
) -> String {
    let mut fields: Vec<&[u8]> = vec![
        BUILD_FINGERPRINT.as_bytes(),
        stage_id.as_bytes(),
        impl_version.as_bytes(),
    ];
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
    /// The complete deterministic `PURRPCK1` dataset image.
    dataset_pack: Vec<u8>,
    /// The lookaside mirror: resources + blob records (the byte-artifact lane and
    /// later typed sidecar lanes ride here).
    lookaside: CachedLookaside,
    /// The content store: blob digest hex → payload bytes (rebuilt via
    /// `insert_checked`, so a corrupt blob hard-fails on load).
    blobs: BTreeMap<String, Vec<u8>>,
    /// The S0.5 PUBLIC provenance projection rows `(unit, kind, artifact, location)`.
    /// NEVER the runtime numeric ids.
    provenance: Vec<CachedProvRow>,
    /// The typed-handle lane: each backing graph IRI + its arm tag. The backing
    /// graph itself already lives in `dataset_pack` and is never duplicated here.
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

/// One public-projection provenance row
/// `(quad_index, unit_name, kind, artifact_path, location)`. `quad_index` is the
/// asserted quad's frozen ordinal, preserved so the reconstituted projection
/// carries the same per-occurrence quad identity (not collapsed to a placeholder).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProvRow {
    quad_index: usize,
    unit: String,
    kind: String,
    artifact: String,
    location: Option<String>,
}

/// A persisted typed handle: the backing graph IRI and [`PipelineHandle`] arm tag.
/// The payload is re-derived by projecting this graph from the restored dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHandle {
    /// The named-graph IRI the handle backs (the [`HandleKey`](purrdf::HandleKey)).
    graph: String,
    /// The [`PipelineHandle`] arm tag (see [`handle_arm_tag`]).
    arm: String,
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
/// (C6): its backing graph is the canonical RDF-1.2 projection of a
/// [`LogicProgram`], so on a cache hit the program is RE-DERIVED from that backing
/// graph via the reverse parser ([`parse_logic_dataset`]) — the consumer never
/// re-parses the logic graph itself, the cache boundary does it ONCE here. A parse
/// failure HARD-fails (no-optionality): a `logic` handle whose backing graph no longer
/// parses to a program is a corrupt cache, never a silently-dropped handle.
fn rebuild_handle(
    arm: &str,
    graph: Arc<purrdf::RdfDataset>,
) -> Result<PipelineHandle, gmeow_errors::Diag> {
    Ok(match arm {
        "logic" => {
            let (program, _diags) = gmeow_logic_compile::frontend::parse_logic_dataset(
                graph.as_ref(),
                None,
            )
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: re-derive Logic handle program from backing graph/logic: {}",
                        e.0
                    ),
                })
            })?;
            PipelineHandle::Logic(Arc::new(program))
        }
        "reasoning" => {
            // The REAL typed Reasoning handle (C7): its backing graph is the
            // deterministic `graph/reasoning` projection of a `ReasoningResult`, so on
            // a cache hit the verdict-and-provenance result is RE-DERIVED from that
            // graph via the reverse parser (the binding rows / closure quads are not
            // carried in this graph — they live in the bundle's default-graph dataset,
            // per the projection's documented round-trip contract). The consumer never
            // re-parses the reasoning graph; the cache boundary does it ONCE here. A
            // parse failure HARD-fails (no-optionality): a `reasoning` handle whose
            // backing graph no longer parses is a corrupt cache, never a dropped handle.
            // The backing sub-dataset is default-graph only (the cache's
            // `project_named_graph` strips the graph name), so its canonical N-Quads
            // lines are `s p o .` — exactly the N-Triples shape the projection's reverse
            // parser reads.
            let nt = serialize_dataset(graph.as_ref(), DATASET_MEDIA_TYPE, SerializeGraph::Dataset)
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: format!("cache: serialize Reasoning handle backing graph: {e}"),
                    })
                })?;
            let nt = String::from_utf8(nt).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: Reasoning backing graph not UTF-8: {e}"),
                })
            })?;
            let result = gmeow_logic::result_rdf::parse_reasoning_graph(&nt).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: re-derive Reasoning handle result from backing graph/reasoning: {e}"
                    ),
                })
            })?;
            PipelineHandle::Reasoning(Arc::new(result))
        }
        "relational-core" => {
            // The REAL typed RelationalCore handle (C8): its backing graph is the
            // deterministic projection of a `RelationalCoreProgram`, so on a cache hit the
            // typed dialect is RE-DERIVED from that graph via the reverse parser. The
            // consumer never re-lowers; the cache boundary re-derives it ONCE here. A parse
            // failure HARD-fails (no-optionality): a `relational-core` handle whose backing
            // graph no longer parses is a corrupt cache, never a silently-dropped handle.
            let program =
                gmeow_logic_compile::relational_core::parse_relational_core(graph.as_ref())
                    .map_err(|e| {
                        gmeow_errors::Diag::of_kind(crate::error::Decode {
                            message: format!(
                                "cache: re-derive RelationalCore handle from backing \
                             graph/relational-core: {e}"
                            ),
                        })
                    })?;
            PipelineHandle::RelationalCore(Arc::new(program))
        }
        "correspondence" => {
            // The REAL typed Correspondence handle (C10): its backing graph is the
            // deterministic `graph/correspondence` projection of a `CorrespondenceProgram`,
            // so on a cache hit the typed program is RE-DERIVED from that graph via the
            // reverse parser. The consumer never re-projects; the cache boundary re-derives
            // it ONCE here. A parse failure HARD-fails (no-optionality): a `correspondence`
            // handle whose backing graph no longer parses is a corrupt cache, never a
            // silently-dropped handle.
            let program = gmeow_logic_compile::projections::correspondence::parse_correspondence(
                graph.as_ref(),
            )
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: re-derive Correspondence handle from backing \
                         graph/correspondence: {e}"
                    ),
                })
            })?;
            PipelineHandle::Correspondence(Arc::new(program))
        }
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cached handle has unknown PipelineHandle arm tag {other:?}"),
            }));
        }
    })
}

/// Map an [`OriginKind`] public string back to the kind. Greenfield: an unknown
/// string HARD-fails — the public projection only emits the closed set, and a
/// "unknown-kind" marker means a forged provenance the cache must not reconstruct.
fn origin_kind_from_str(kind: &str) -> Result<OriginKind, gmeow_errors::Diag> {
    Ok(match kind {
        "source" => OriginKind::Source,
        "root-ontology" => OriginKind::RootOntology,
        "import" => OriginKind::Import,
        "generated" => OriginKind::Generated,
        "runtime-input" => OriginKind::RuntimeInput,
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached provenance row carries an unrepresentable origin kind {other:?}"
                ),
            }));
        }
    })
}

impl CachedLookaside {
    /// Mirror a kernel [`RdfLookaside`], HARD-failing if it carries a lane this
    /// mirror does not yet model (no silent loss).
    fn from_lookaside(la: &RdfLookaside) -> Result<Self, gmeow_errors::Diag> {
        if !la.metadata.is_empty()
            || !la.segments.is_empty()
            || !la.suppressions.is_empty()
            || !la.opaque_nodes.is_empty()
            || !la.signatures.is_empty()
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message:
                    "pipeline bundle lookaside carries a lane (metadata/segments/suppressions/\
                 opaque-nodes/signatures) the C4 cache mirror does not yet model — grow the \
                 mirror before persisting it (no silent loss)"
                        .to_string(),
            }));
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
    fn from_resource(r: &RdfLookasideResource) -> Result<Self, gmeow_errors::Diag> {
        if !r.metadata.is_empty() || r.location.is_some() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message:
                    "pipeline lookaside resource carries metadata/location the C4 cache mirror \
                 does not yet model — grow the mirror before persisting it (no silent loss)"
                        .to_string(),
            }));
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
    fn from_record(r: &RdfBlobRecord) -> Result<Self, gmeow_errors::Diag> {
        if !r.metadata.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message:
                    "pipeline lookaside blob record carries metadata the C4 cache mirror does not \
                 yet model — grow the mirror before persisting it (no silent loss)"
                        .to_string(),
            }));
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
    fn from_product(product: &StageProduct) -> Result<Self, gmeow_errors::Diag> {
        let bundle = product.bundle();

        // dataset → deterministic packed IR. This retains the complete RDF 1.2
        // value and its query indexes without serializing through RDF text.
        let dataset_pack = PackBuilder::build_bytes(bundle.dataset()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cache: pack bundle dataset: {e}"),
            })
        })?;

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
            .map(
                |(quad_index, unit, kind, artifact, location)| CachedProvRow {
                    quad_index,
                    unit,
                    kind,
                    artifact,
                    location,
                },
            )
            .collect();

        // handles → graph IRI + arm tag, sorted by graph IRI (BTreeMap iteration is
        // already sorted). The graph data itself is already present once in the
        // packed dataset and must not be duplicated in the manifest.
        let mut handles = Vec::with_capacity(bundle.handles().len());
        for (graph, entry) in bundle.handles() {
            handles.push(CachedHandle {
                graph: graph.clone(),
                arm: handle_arm_tag(&entry.payload).to_string(),
            });
        }

        Ok(Self {
            version: CACHE_VERSION,
            stage_id: product.stage_id.clone(),
            digest: product.digest.clone(),
            dataset_pack,
            lookaside,
            blobs,
            provenance,
            handles,
        })
    }

    /// Reconstitute a digest- and structure-equal [`StageProduct`] from the manifest.
    fn into_product(self) -> Result<StageProduct, gmeow_errors::Diag> {
        if self.version != CACHE_VERSION {
            // A version-mismatched manifest is a clean miss handled by the caller; a
            // mismatch reaching here means a tampered/forged blob — hard-fail.
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle version {} != expected {CACHE_VERSION}",
                    self.version
                ),
            }));
        }

        // dataset: reconstruct directly from the packed dictionary, indexes, and
        // RDF 1.2 side tables. The cache blob digest was verified by `get` before
        // this point, so the hot path does not repeat canonicalization.
        let dataset = restore_pack(&self.dataset_pack).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cache: restore packed bundle dataset: {e}"),
            })
        })?;

        let lookaside = self.lookaside.into_lookaside();

        // blobs: rebuild via insert_checked so a corrupt blob HARD-fails.
        let mut store = ContentStore::new();
        for (hex, bytes) in self.blobs {
            let digest = ContentDigest::from_hex(&hex).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: malformed blob digest hex {hex:?}"),
                })
            })?;
            store.insert_checked(digest, bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: hex.clone(),
                    actual: format!("{e}"),
                })
            })?;
        }

        // provenance: re-register units/artifacts/occurrences so the reconstituted
        // public projection equals the persisted one. Each occurrence is rebound to
        // its persisted asserted-quad ordinal so the projection's per-occurrence quad
        // identity round-trips (the public projection carries the quad index).
        let mut provenance = DatasetProvenance::new();
        for row in &self.provenance {
            let kind = origin_kind_from_str(&row.kind)?;
            let unit = provenance.register_unit(row.unit.clone(), kind);
            let artifact = provenance.register_artifact(row.artifact.clone());
            let quad_ordinal = u32::try_from(row.quad_index).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: provenance quad ordinal {} exceeds u32",
                        row.quad_index
                    ),
                })
            })?;
            provenance.record_occurrence(
                QuadHandle::from_index(quad_ordinal),
                unit,
                artifact,
                row.location.clone(),
            );
        }

        // Assemble the bundle, then re-pin every handle (re-checks the digest invariant).
        let mut bundle = PipelineBundleAlias::new(dataset, lookaside, Arc::new(store), provenance);
        for h in self.handles {
            let subgraph = Arc::new(bundle.dataset().project_named_graph(&h.graph));
            // Derive the pin and typed payload from the SAME live graph projection.
            // `pin_handle` independently checks that pin against the restored carrier,
            // preserving the hard-fail invariant without a duplicate persisted graph.
            let pinned = ContentDigest::of(canonicalize(&subgraph).nquads.as_bytes());
            let payload = rebuild_handle(&h.arm, subgraph)?;
            bundle
                .pin_handle(h.graph.clone(), payload, pinned)
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: format!("cache: re-pin handle for <{}> failed: {e}", h.graph),
                    })
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
type PipelineBundleAlias = purrdf::PipelineBundle<PipelineHandle>;

// ── On-disk content-addressed cache ──────────────────────────────────────────

/// The persistent per-stage cache under `.cache/gmeow-sync/pipeline/<version>/`
/// (gitignored and worktree-local).
///
/// `index.json` maps `stage_key → blob ContentDigest (hex)`; `blobs/<hex>` holds
/// the bincode-serialized [`CachedBundle`]. Reads re-hash the blob and HARD-fail on
/// a digest mismatch (self-verifying, no silent repair). The `<version>` segment
/// makes a prior cache-shape or codec revision a clean miss (greenfield, no
/// migration).
pub struct PipelineCache {
    dir: PathBuf,
    index: BTreeMap<String, String>,
}

impl PipelineCache {
    /// The conventional cache base directory under a repo root. [`open`](Self::open)
    /// appends the version segment, so this is the un-segmented base.
    pub fn default_dir(root: &Path) -> PathBuf {
        root.join(".cache").join("gmeow-sync").join("pipeline")
    }

    /// Open (or create) the cache rooted at `dir`, loading its index. The on-disk
    /// store lives under a `v<CACHE_VERSION>` leaf of `dir` so a prior cache-shape
    /// rev is isolated — a shape bump makes every older cache a clean miss
    /// (greenfield, no migration).
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, gmeow_errors::Diag> {
        let dir = dir.into().join(format!("v{CACHE_VERSION}"));
        fs::create_dir_all(dir.join("blobs"))?;
        let index_path = dir.join("index.json");
        let index: BTreeMap<String, String> = if index_path.exists() {
            let bytes = fs::read(&index_path)?;
            serde_json::from_slice(&bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("corrupt pipeline cache index: {e}"),
                })
            })?
        } else {
            BTreeMap::new()
        };
        Ok(Self { dir, index })
    }

    /// Look up a stage product by cache key. Returns `None` on a miss. HARD-fails
    /// (`CacheMismatch`) if the blob exists but its re-hashed digest disagrees
    /// with the index — the cache is never silently repaired.
    pub fn get(&self, stage_key: &str) -> Result<Option<StageProduct>, gmeow_errors::Diag> {
        let Some(digest_hex) = self.index.get(stage_key) else {
            return Ok(None);
        };
        let blob_path = self.dir.join("blobs").join(digest_hex);
        if !blob_path.exists() {
            // Index references a missing blob: a corrupt cache, not a clean miss.
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: digest_hex.clone(),
                actual: "<missing blob>".to_string(),
            }));
        }
        let bytes = fs::read(&blob_path)?;
        let actual = ContentDigest::of(&bytes).to_hex();
        if &actual != digest_hex {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: digest_hex.clone(),
                actual,
            }));
        }
        let cached: CachedBundle = bincode::deserialize(&bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("corrupt cached bundle: {e}"),
            })
        })?;
        // A version-mismatched manifest is treated as a clean MISS (greenfield): the
        // entry belongs to a prior shape rev and must not be mis-decoded.
        if cached.version != CACHE_VERSION {
            return Ok(None);
        }
        Ok(Some(cached.into_product()?))
    }

    /// Store a stage product under `stage_key`, persisting the blob and index.
    pub fn put(
        &mut self,
        stage_key: &str,
        product: &StageProduct,
    ) -> Result<(), gmeow_errors::Diag> {
        let manifest = CachedBundle::from_product(product)?;
        let bytes = bincode::serialize(&manifest).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cannot serialize cached bundle: {e}"),
            })
        })?;
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

    fn persist_index(&self) -> Result<(), gmeow_errors::Diag> {
        // Deterministic: BTreeMap serializes in sorted key order.
        let bytes = serde_json::to_vec_pretty(&self.index).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cannot serialize cache index: {e}"),
            })
        })?;
        write_atomic(&self.dir.join("index.json"), &bytes)?;
        Ok(())
    }
}

/// Write `bytes` to `target` atomically: stage them in a sibling temp file in the
/// SAME directory (so the final `rename` stays on one filesystem, where POSIX
/// guarantees atomicity), then rename over the target. An interrupted write can
/// only ever leave a stray temp file, never a half-written `target` — so the
/// cache is never bricked mid-write (no-optionality P2).
fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), gmeow_errors::Diag> {
    // Idempotency policy: an equal output is already the desired state. Avoiding
    // the temp write + rename preserves the target's mtime/inode and eliminates
    // filesystem churn for warm sync/check runs.
    if let Ok(existing) = fs::read(target)
        && existing == bytes
    {
        return Ok(());
    }

    let dir = target.parent().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            message: (std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("cache path {} has no parent directory", target.display()),
            ))
            .to_string(),
        })
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
    use purrdf::{PipelineBundle, RdfDatasetBuilder, RdfTerm, TermId, parse_dataset};

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(&format!("http://example.org/{n}"))
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
    fn dataset_with_named_graph() -> Arc<purrdf::RdfDataset> {
        let arts = gmeow_logic_compile::projections::compile_program(
            &sample_logic_program(),
            &Default::default(),
        )
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

        // Attach the REAL typed Logic handle (C6) over the named graph: the
        // payload is the compiled program, pinned to the canonical digest of its
        // backing `graph/logic` projection.
        let program = Arc::new(sample_logic_program());
        let pinned = bundle.graph_digest(GRAPH_IRI);
        bundle
            .pin_handle(GRAPH_IRI, PipelineHandle::Logic(program), pinned)
            .expect("pin handle over the named graph");
        bundle
    }

    fn canon_hex(ds: &purrdf::RdfDataset) -> String {
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
        assert_eq!(
            recon.handles().len(),
            original.handles().len(),
            "handle count"
        );
        let entry = recon.handle(GRAPH_IRI).expect("handle re-attached");
        let PipelineHandle::Logic(reconstituted) = &entry.payload else {
            panic!("handle arm preserved (Logic)");
        };
        // The REAL typed Logic handle (C6) re-derives from its backing
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
    fn cached_bundle_binary_encoding_avoids_json_byte_array_expansion() {
        let payload = vec![0xff; 4096];
        let mut blobs = BTreeMap::new();
        blobs.insert("blob-digest".to_owned(), payload.clone());
        let manifest = CachedBundle {
            version: CACHE_VERSION,
            stage_id: "compact-cache-regression".to_owned(),
            digest: "product-digest".to_owned(),
            dataset_pack: payload.clone(),
            lookaside: CachedLookaside::default(),
            blobs,
            provenance: Vec::new(),
            handles: vec![CachedHandle {
                graph: "http://example.org/graph".to_owned(),
                arm: "logic".to_owned(),
            }],
        };

        let binary = bincode::serialize(&manifest).expect("serialize compact cache manifest");
        let json = serde_json::to_vec(&manifest).expect("serialize comparison manifest");
        assert!(
            binary.len() * 3 < json.len(),
            "byte lanes must stay compact: binary={} JSON={}",
            binary.len(),
            json.len()
        );

        let decoded: CachedBundle =
            bincode::deserialize(&binary).expect("deserialize compact cache manifest");
        assert_eq!(decoded.dataset_pack, manifest.dataset_pack);
        assert_eq!(decoded.blobs, manifest.blobs);
        assert_eq!(decoded.handles[0].graph, manifest.handles[0].graph);
        assert_eq!(decoded.handles[0].arm, manifest.handles[0].arm);
    }

    #[test]
    fn tampered_handle_manifest_hard_fails_on_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        cache.put("k", &product).unwrap();

        // Tamper the persisted handle arm while keeping the packed dataset intact.
        // An unknown arm must HARD-FAIL rather than silently dropping the handle.
        let blobs_dir = cache.dir.join("blobs");
        let blob_path = std::fs::read_dir(&blobs_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = std::fs::read(&blob_path).unwrap();
        let mut manifest: CachedBundle = bincode::deserialize(&bytes).unwrap();
        manifest.handles[0].arm = "not-a-pipeline-handle".to_owned();
        // Re-serialize + re-file under the NEW content digest (and re-point the index),
        // so the blob still self-verifies and we exercise the handle re-pin path.
        let new_bytes = bincode::serialize(&manifest).unwrap();
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
            err.is::<crate::error::Decode>(),
            "tampered handle manifest hard-fails, got {err:?}"
        );
    }

    /// A `ReasoningResult` whose `graph/reasoning` projection backs a cache handle, so
    /// the cache's re-derivation (`parse_reasoning_graph`) reconstructs a faithful
    /// verdict-and-provenance result (C7).
    fn sample_reasoning_result() -> gmeow_logic::result::ReasoningResult {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };
        // projection_class mirrors the result's `preservation` axis in every real
        // construction; the parser reconstructs it from that axis, so the fixture sets
        // them equal (an inconsistent fixture would test a state no real result holds).
        let mut prov =
            ResultProvenance::native("contract:cache-test", "http://example.org/world/w");
        prov.projection_class = PreservationClaim::exact();
        ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Supported,
            prov,
            ResultPayload::Empty,
        )
    }

    /// A bundle whose dataset carries a `graph/reasoning` named graph (the projection
    /// of [`sample_reasoning_result`]) with a typed Reasoning handle pinned to it.
    fn reasoning_bundle() -> PipelineBundle<PipelineHandle> {
        use std::sync::Arc;
        let result = sample_reasoning_result();
        let projection = gmeow_logic::result_rdf::project_reasoning_result(&result);
        let parsed = parse_dataset(projection.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let graph_iri = gmeow_logic::result_rdf::GRAPH_REASONING;
        let mut b = RdfDatasetBuilder::new();
        let term = RdfTerm::Iri(graph_iri.to_owned());
        for quad in parsed.owned_quads() {
            let mut routed = quad.clone();
            routed.graph_name = Some(term.clone());
            b.push_owned_quad(&routed);
        }
        let dataset = b.freeze().expect("freeze");
        let mut bundle = PipelineBundle::new(
            dataset,
            RdfLookaside::default(),
            Arc::new(ContentStore::new()),
            DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(graph_iri);
        bundle
            .pin_handle(
                graph_iri,
                PipelineHandle::Reasoning(Arc::new(result)),
                pinned,
            )
            .expect("pin Reasoning handle");
        bundle
    }

    #[test]
    fn cached_reasoning_handle_re_derives_the_result() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let original = reasoning_bundle();
        let product = StageProduct::from_bundle("stage-reason", Arc::new(original.clone()));
        cache.put("k", &product).unwrap();

        let got = cache.get("k").unwrap().expect("cache hit");
        let recon = got.bundle();

        let graph_iri = gmeow_logic::result_rdf::GRAPH_REASONING;
        let entry = recon
            .handle(graph_iri)
            .expect("Reasoning handle re-attached");
        let PipelineHandle::Reasoning(result) = &entry.payload else {
            panic!("the re-derived handle arm is Reasoning");
        };
        // The verdict-and-provenance result round-trips faithfully (axes + provenance).
        assert_eq!(
            result.as_ref(),
            &sample_reasoning_result(),
            "the cache re-derived the Reasoning handle's result faithfully"
        );
        // The pin matches the reconstituted backing graph.
        assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
        // The bundle content fold round-trips.
        assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
    }

    /// A bundle whose dataset carries a `graph/relational-core` named graph (the
    /// projection of a lowered Horn program) with a typed RelationalCore handle pinned
    /// to it (C8).
    fn relational_core_bundle() -> (
        PipelineBundle<PipelineHandle>,
        gmeow_logic_compile::relational_core::RelationalCoreProgram,
    ) {
        use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
        use gmeow_logic_compile::relational_core::{lower_program, project_relational_core};
        use std::sync::Arc;
        let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let ax = |s: &str, o: &str| {
            LogicAxiom::new(s, sc, o, false, false, ContextualScope::default()).expect("axiom")
        };
        let rule = LogicRule::new(
            ax("?x", "?z"),
            vec![ax("?x", "?y"), ax("?y", "?z")],
            vec![],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(
            vec![ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                "https://blackcatinformatics.ca/gmeow/Animal",
            )],
            vec![rule],
            vec![],
            None,
        );
        let lowered = lower_program(&program);
        let projection = project_relational_core(&lowered);
        let parsed = parse_dataset(projection.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let graph_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
        let mut b = RdfDatasetBuilder::new();
        let term = RdfTerm::Iri(graph_iri.to_owned());
        for quad in parsed.owned_quads() {
            let mut routed = quad.clone();
            routed.graph_name = Some(term.clone());
            b.push_owned_quad(&routed);
        }
        let dataset = b.freeze().expect("freeze");
        let mut bundle = PipelineBundle::new(
            dataset,
            RdfLookaside::default(),
            Arc::new(ContentStore::new()),
            DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(graph_iri);
        bundle
            .pin_handle(
                graph_iri,
                PipelineHandle::RelationalCore(Arc::new(lowered.clone())),
                pinned,
            )
            .expect("pin RelationalCore handle");
        (bundle, lowered)
    }

    #[test]
    fn cached_relational_core_handle_re_derives_the_dialect() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let (original, lowered) = relational_core_bundle();
        let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
        cache.put("k", &product).unwrap();

        let got = cache.get("k").unwrap().expect("cache hit");
        let recon = got.bundle();

        let graph_iri = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;
        let entry = recon
            .handle(graph_iri)
            .expect("RelationalCore handle re-attached");
        let PipelineHandle::RelationalCore(program) = &entry.payload else {
            panic!("the re-derived handle arm is RelationalCore");
        };
        // The typed dialect round-trips faithfully (content-key-equal).
        assert_eq!(
            program.content_key(),
            lowered.content_key(),
            "the cache re-derived the RelationalCore handle's dialect faithfully"
        );
        // The pin matches the reconstituted backing graph.
        assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
        // The bundle content fold round-trips.
        assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
    }

    /// A bundle whose dataset carries a `graph/correspondence` named graph (the §14
    /// affine-triangle worked example) with a typed Correspondence handle pinned to it
    /// (C10).
    fn correspondence_bundle() -> (
        PipelineBundle<PipelineHandle>,
        gmeow_logic_compile::projections::correspondence::CorrespondenceProgram,
    ) {
        use gmeow_logic_compile::projections::correspondence::{
            affine_triangle_worked_example, project_correspondence,
        };
        use std::sync::Arc;
        let program = affine_triangle_worked_example();
        let projection = project_correspondence(&program);
        let parsed = parse_dataset(projection.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let graph_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
        let mut b = RdfDatasetBuilder::new();
        let term = RdfTerm::Iri(graph_iri.to_owned());
        for quad in parsed.owned_quads() {
            let mut routed = quad.clone();
            routed.graph_name = Some(term.clone());
            b.push_owned_quad(&routed);
        }
        let dataset = b.freeze().expect("freeze");
        let mut bundle = PipelineBundle::new(
            dataset,
            RdfLookaside::default(),
            Arc::new(ContentStore::new()),
            DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(graph_iri);
        bundle
            .pin_handle(
                graph_iri,
                PipelineHandle::Correspondence(Arc::new(program.clone())),
                pinned,
            )
            .expect("pin Correspondence handle");
        (bundle, program)
    }

    /// The C4 structural round-trip stays green for the Correspondence arm: the cache
    /// re-derives the typed [`CorrespondenceProgram`] from the backing graph on a hit, to
    /// a content-key-equal program, and the bundle digest is preserved.
    #[test]
    fn cached_correspondence_handle_re_derives_the_program() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();

        let (original, program) = correspondence_bundle();
        let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
        cache.put("k", &product).unwrap();

        let got = cache.get("k").unwrap().expect("cache hit");
        let recon = got.bundle();

        let graph_iri = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;
        let entry = recon
            .handle(graph_iri)
            .expect("Correspondence handle re-attached");
        let PipelineHandle::Correspondence(re_derived) = &entry.payload else {
            panic!("the re-derived handle arm is Correspondence");
        };
        assert_eq!(
            re_derived.content_key(),
            program.content_key(),
            "the cache re-derived the Correspondence handle's program faithfully"
        );
        assert_eq!(entry.content_digest, recon.graph_digest(graph_iri));
        assert_eq!(recon.digest(), original.digest(), "bundle digest preserved");
    }
}
