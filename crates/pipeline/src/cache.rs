// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The opt-in per-stage content-addressed cache (C4-cache).
//!
//! The cache key hashes a typed [`StageKeyContext`] containing build/toolchain
//! identity, stage/codec identity, producer-qualified whole/entity inputs, and raw
//! path/digest rows. `.cache/gmeow-sync/pipeline/<fingerprint>/<version>/`
//! (gitignored) stores immutable per-key receipts and content-addressed
//! [`CachedBundle`] blobs. It is self-verifying: a
//! digest recheck on load HARD-fails on mismatch and never silently repairs
//! (no-optionality).
//!
//! Full repository synchronization uses this cache only for RDF-declared
//! `cachePersistentContribution` stages. Before persistence, the cache proves that
//! every named graph, blob representation, logical artifact, and typed handle in the
//! product is part of the scheduler-authenticated output delta. Aggregate stages are
//! `cacheRecomputeAggregate` and never reach this codec. This makes the serialized unit
//! an independently bounded contribution, not a cumulative carrier snapshot; the
//! whole-run clean manifest remains the zero-work fixed-point boundary above it.
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
//! * **handles** — each `(graph_iri, HandleEntry)` persists its graph IRI, arm tag,
//!   and semantic digest. Graph-lossless arms are re-derived from the restored named
//!   graph. The Logic arm additionally persists its complete typed IR because its RDF
//!   surface is intentionally lossy. Every handle is re-attached via `pin_handle`, so
//!   both semantic identity and the graph digest-pin invariant are re-checked.
//!
//! # GREENFIELD cache version
//!
//! [`CACHE_VERSION`] is folded into BOTH the on-disk subdirectory and the manifest.
//! A version bump makes every prior cache (including the C4-spine byte-only
//! stand-in) a clean MISS — there is no migration path (greenfield).
//!
//! # On-disk layout
//!
//! `.cache/gmeow-sync/pipeline/<fingerprint>/<version>/` holds
//! `receipts/<action-key>.json` roots and `blobs/<digest>` bincode products. The
//! binary encoding keeps the manifest's large `Vec<u8>` lanes
//! byte-dense instead of expanding every byte into a JSON number. On load the blob
//! is re-hashed and compared to the indexed digest — a mismatch is a HARD failure,
//! never a silent repair (no-optionality).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
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
pub const CACHE_VERSION: u32 = 9;

/// Schema revision for the canonical action-key rows and immutable stage receipt.
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;

/// The structural codec identity. This is explicit in every action key rather than
/// relying only on [`CACHE_VERSION`], because a receipt must name the representation
/// it authenticates without knowing its storage path.
pub const CACHE_CODEC_IDENTITY: &str = "bincode-1+purrpack1+logic-ir1+receipt-json-1";

/// No independently reusable contribution may serialize above 256 MiB. The measured
/// useful persistent units are at most ~138 MiB; whole-document leaves at 1.5--2.5 GiB
/// and cumulative carriers are explicitly recomputed. This ratchet forces a future
/// growing stage through a fresh size/hydration census instead of silently turning the
/// cache into another multi-gigabyte carrier store. It is deliberately much stricter
/// than the repository's separate 16 GiB peak-build-memory contract.
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// Receipts are a compact census, never a payload lane. Bound them separately so a
/// forged root cannot make a reader allocate an attacker-sized JSON buffer before
/// structural validation runs.
const MAX_RECEIPT_BYTES: u64 = 4 * 1024 * 1024;

/// Default bounded-store quotas. They are storage economics, never correctness
/// switches: eviction turns a future lookup into ordinary recomputation.
const MAX_CACHE_ENTRIES: usize = 512;
const MAX_CACHE_BYTES: u64 = 24 * 1024 * 1024 * 1024;

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

/// The build fingerprint folded into every [`stage_key`]: workspace Rust sources and
/// Cargo manifests, lock/config/toolchain files, full compiler identity, target,
/// profile, features, and relevant code-generation flags, computed by `build.rs`.
pub const BUILD_FINGERPRINT: &str = env!("GMEOW_BUILD_FINGERPRINT");

/// Complete compiler identity digest emitted by `build.rs` from `rustc -Vv`.
pub const TOOLCHAIN_FINGERPRINT: &str = env!("GMEOW_TOOLCHAIN_FINGERPRINT");

/// Cargo-selected build target, profile, and enabled feature set. These are explicit
/// receipt fields as well as inputs to [`BUILD_FINGERPRINT`].
pub const BUILD_TARGET: &str = env!("GMEOW_BUILD_TARGET");
pub const BUILD_PROFILE: &str = env!("GMEOW_BUILD_PROFILE");
pub const BUILD_FEATURES: &str = env!("GMEOW_BUILD_FEATURES");

/// One typed upstream input row. `entity = None` means the producer's whole product;
/// `Some(iri)` means precisely that declared dataflow entity. Keeping the producer and
/// marker beside the digest prevents swapped equal-looking inputs from colliding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StageInputDigest {
    pub producer: String,
    pub entity: Option<String>,
    pub digest: String,
}

/// One declared raw input, named by its repository-relative logical path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RawInputDigest {
    pub path: String,
    pub digest: String,
}

/// The executable build identity embedded in an action key and repeated in its
/// receipt for inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildIdentity {
    pub fingerprint: String,
    pub toolchain: String,
    pub target: String,
    pub profile: String,
    pub features: Vec<String>,
}

impl BuildIdentity {
    pub fn current() -> Self {
        Self {
            fingerprint: BUILD_FINGERPRINT.to_string(),
            toolchain: TOOLCHAIN_FINGERPRINT.to_string(),
            target: BUILD_TARGET.to_string(),
            profile: BUILD_PROFILE.to_string(),
            features: BUILD_FEATURES
                .split(',')
                .filter(|feature| !feature.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }
}

/// Complete, domain-separated identity of one executable pipeline action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageKeyContext {
    pub schema_version: u32,
    pub stage_id: String,
    pub impl_version: String,
    pub codec: String,
    pub build: BuildIdentity,
    pub upstream: Vec<StageInputDigest>,
    pub raw_inputs: Vec<RawInputDigest>,
    /// First-class selected dimensions consumed by this action. Most stages have no
    /// rows here; scope/language/output profile belong only at their actual consumer.
    pub dimensions: BTreeMap<String, String>,
}

impl StageKeyContext {
    pub fn new(
        stage_id: impl Into<String>,
        impl_version: impl Into<String>,
        mut upstream: Vec<StageInputDigest>,
        mut raw_inputs: Vec<RawInputDigest>,
    ) -> Self {
        upstream.sort();
        upstream.dedup();
        raw_inputs.sort();
        raw_inputs.dedup();
        Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            stage_id: stage_id.into(),
            impl_version: impl_version.into(),
            codec: CACHE_CODEC_IDENTITY.to_string(),
            build: BuildIdentity::current(),
            upstream,
            raw_inputs,
            dimensions: BTreeMap::new(),
        }
    }

    pub fn with_dimension(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.dimensions.insert(name.into(), value.into());
        self
    }
}

/// The per-stage action key over the complete typed context. Rows sort canonically in
/// [`StageKeyContext::new`], but producer/entity/path identity is never discarded.
///
/// Folding [`BUILD_FINGERPRINT`] makes the key capture the producing CODE, not just
/// its declared `impl_version`: a stage whose Rust impl changed (here or in any
/// workspace crate it calls, e.g. `gmeow-logic`) gets a fresh key and recomputes,
/// so a persistent cache can never serve a stale pre-change product.
pub fn stage_key(context: &StageKeyContext) -> String {
    // `StageKeyContext` contains only deterministic maps/vectors. JSON is used here
    // because it is also the human-inspectable receipt projection; serialization of
    // this closed string/integer shape cannot fail.
    let bytes = serde_json::to_vec(context).expect("StageKeyContext JSON serialization");
    content_digest(&[b"gmeow:pipeline-action-key:v1", &bytes])
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

/// A persisted typed handle: the backing graph IRI, [`PipelineHandle`] arm tag,
/// semantic payload digest, and (only where the projection is deliberately lossy) a
/// complete typed payload. Reasoning/relational/correspondence handles remain derived
/// from their backing graphs; Logic carries its full typed IR because graph/logic is a
/// governed projection that intentionally omits source-verbatim collections.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHandle {
    /// The named-graph IRI the handle backs (the [`HandleKey`](purrdf::HandleKey)).
    graph: String,
    /// The [`PipelineHandle`] arm tag (see [`handle_arm_tag`]).
    arm: String,
    /// Digest of the typed payload's canonical semantic key. A backing graph whose
    /// reverse projection drops a typed field is not cache-admissible.
    payload_digest: String,
    /// Full typed payload only for an arm whose governed graph is not a lossless codec.
    typed_payload: Option<Vec<u8>>,
}

/// The output delta selected by the canonical stage declaration. Receipts describe
/// these independently reusable contributions, not every cumulative carrier byte an
/// implementation happens to retain while running.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReceiptOutputSelection {
    pub graphs: Vec<String>,
    pub blob_representations: Vec<String>,
    pub logical_artifacts: Vec<String>,
    pub handles: Vec<String>,
}

/// One content-addressed output entity authenticated by a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptEntity {
    pub identity: String,
    pub digest: String,
    pub structural_count: u64,
    pub decoded_bytes: u64,
}

/// Immutable deterministic receipt for one stage action. Observations such as
/// hit/miss, elapsed time, RSS, and transfer bytes deliberately live elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageReceipt {
    pub schema_version: u32,
    pub action_key: String,
    pub context: StageKeyContext,
    pub stability: String,
    pub cache_disposition: String,
    pub product_digest: String,
    pub product_blob_digest: Option<String>,
    pub product_blob_bytes: u64,
    pub dataset_quads: u64,
    pub graphs: Vec<ReceiptEntity>,
    pub blob_representations: Vec<ReceiptEntity>,
    pub logical_artifacts: Vec<ReceiptEntity>,
    pub typed_handles: Vec<ReceiptEntity>,
}

impl StageReceipt {
    /// Digest of the canonical receipt payload (the envelope stores and verifies it).
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("StageReceipt JSON serialization");
        content_digest(&[b"gmeow:stage-receipt:v1", &bytes])
    }

    fn from_product(
        context: StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        selection: &ReceiptOutputSelection,
        product: &StageProduct,
        product_blob_digest: Option<String>,
        product_blob_bytes: u64,
    ) -> Result<Self, gmeow_errors::Diag> {
        let action_key = stage_key(&context);
        let bundle = product.bundle();

        let mut graph_names = selection.graphs.clone();
        graph_names.sort();
        graph_names.dedup();
        let graphs = graph_names
            .into_iter()
            .map(|graph| {
                let projected = bundle.dataset().project_named_graph(&graph);
                Ok(ReceiptEntity {
                    identity: graph.clone(),
                    digest: bundle.graph_digest(&graph).to_hex(),
                    structural_count: u64::try_from(projected.quad_count()).map_err(|_| {
                        gmeow_errors::Diag::of_kind(crate::error::Decode {
                            message: format!("receipt graph <{graph}> quad count exceeds u64"),
                        })
                    })?,
                    decoded_bytes: 0,
                })
            })
            .collect::<Result<Vec<_>, gmeow_errors::Diag>>()?;

        let selected_blob_reps: BTreeSet<&str> = selection
            .blob_representations
            .iter()
            .map(String::as_str)
            .collect();
        let mut blob_representations = Vec::new();
        for record in &bundle.lookaside().blobs {
            let Some(representation) = record.representation.as_deref() else {
                continue;
            };
            if !selected_blob_reps.contains(representation) {
                continue;
            }
            blob_representations.push(ReceiptEntity {
                identity: representation.to_string(),
                digest: record.digest.clone(),
                structural_count: 1,
                decoded_bytes: u64::try_from(record.decoded_len.unwrap_or(0)).unwrap_or(u64::MAX),
            });
        }
        blob_representations.sort_by(|left, right| {
            (&left.identity, &left.digest).cmp(&(&right.identity, &right.digest))
        });

        let selected_artifacts: BTreeSet<&str> = selection
            .logical_artifacts
            .iter()
            .map(String::as_str)
            .collect();
        let mut logical_artifacts = Vec::new();
        for resource in &bundle.lookaside().resources {
            let (Some(name), Some(digest)) =
                (resource.name.as_deref(), resource.content_digest.as_deref())
            else {
                continue;
            };
            if !selected_artifacts.contains(name) {
                continue;
            }
            let parsed = ContentDigest::from_hex(digest).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("receipt artifact {name:?} has malformed digest {digest:?}"),
                })
            })?;
            let bytes = bundle.blobs().get(&parsed).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("receipt artifact {name:?} references missing blob {digest}"),
                })
            })?;
            logical_artifacts.push(ReceiptEntity {
                identity: name.to_string(),
                digest: digest.to_string(),
                structural_count: 1,
                decoded_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            });
        }
        logical_artifacts.sort_by(|left, right| left.identity.cmp(&right.identity));

        let selected_handles: BTreeSet<&str> =
            selection.handles.iter().map(String::as_str).collect();
        let mut typed_handles = Vec::new();
        for (graph, entry) in bundle.handles() {
            if !selected_handles.contains(graph.as_str()) {
                continue;
            }
            let payload_digest = handle_payload_digest(&entry.payload);
            typed_handles.push(ReceiptEntity {
                identity: format!("{}#{graph}", handle_arm_tag(&entry.payload)),
                digest: content_digest(&[
                    entry.content_digest.to_hex().as_bytes(),
                    payload_digest.as_bytes(),
                ]),
                structural_count: 1,
                decoded_bytes: u64::try_from(payload_digest.len()).unwrap_or(u64::MAX),
            });
        }
        typed_handles.sort_by(|left, right| left.identity.cmp(&right.identity));

        Ok(Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            action_key,
            context,
            stability: stability.to_string(),
            cache_disposition: cache_disposition.to_string(),
            product_digest: product.digest.clone(),
            product_blob_digest,
            product_blob_bytes,
            dataset_quads: u64::try_from(bundle.dataset().quad_count()).unwrap_or(u64::MAX),
            graphs,
            blob_representations,
            logical_artifacts,
            typed_handles,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptEnvelope {
    receipt_digest: String,
    receipt: StageReceipt,
}

/// A verified cache hit plus deterministic receipt and observational hydration size.
#[derive(Debug)]
pub struct CacheHit {
    pub product: StageProduct,
    pub receipt: StageReceipt,
    pub hydrated_bytes: u64,
}

/// A verified selective cache hit containing only committed logical artifacts.
///
/// The enclosing product blob and immutable receipt are authenticated exactly as for
/// [`CacheHit`], but the packed RDF dataset and typed handles are never reconstructed.
/// This is for artifact-only consumers such as golden/parity tests; it is not a second
/// cache namespace or producer.
#[derive(Debug)]
pub struct ArtifactCacheHit {
    pub artifacts: BTreeMap<String, Vec<u8>>,
    pub receipt: StageReceipt,
    pub transferred_bytes: u64,
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

/// Canonical semantic identity of a typed handle payload, independent of its backing
/// graph pin. The two identities are deliberately separate: a graph projection may be
/// digest-valid while omitting typed fields that a downstream consumer reads.
fn handle_payload_digest(handle: &PipelineHandle) -> String {
    let key = match handle {
        PipelineHandle::Logic(program) => program.canonical_key(),
        PipelineHandle::Reasoning(result) => {
            gmeow_logic::result_rdf::project_reasoning_result(result)
        }
        PipelineHandle::RelationalCore(program) => program.content_key(),
        PipelineHandle::Correspondence(program) => program.content_key(),
    };
    content_digest(&[handle_arm_tag(handle).as_bytes(), key.as_bytes()])
}

/// Rebuild a [`PipelineHandle`] from its complete admitted representation.
///
/// Reasoning, relational-core, and correspondence are losslessly re-derived from their
/// backing sub-datasets. Logic's governed RDF surface is intentionally lossy, so that arm
/// instead requires its full typed IR bytes. Every arm is checked against its canonical
/// semantic payload digest before it is re-pinned to the restored backing graph.
fn rebuild_handle(
    arm: &str,
    graph: Arc<purrdf::RdfDataset>,
    typed_payload: Option<&[u8]>,
) -> Result<PipelineHandle, gmeow_errors::Diag> {
    Ok(match arm {
        "logic" => {
            let bytes = typed_payload.ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: "cache: Logic handle is missing its complete typed IR payload"
                        .to_string(),
                })
            })?;
            let program = bincode::deserialize(bytes).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: decode complete Logic handle IR: {error}"),
                })
            })?;
            PipelineHandle::Logic(Arc::new(program))
        }
        "reasoning" => {
            reject_unexpected_typed_payload(arm, typed_payload)?;
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
            reject_unexpected_typed_payload(arm, typed_payload)?;
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
            reject_unexpected_typed_payload(arm, typed_payload)?;
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

fn reject_unexpected_typed_payload(
    arm: &str,
    typed_payload: Option<&[u8]>,
) -> Result<(), gmeow_errors::Diag> {
    if typed_payload.is_some() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!("cache: {arm} handle carries an undeclared typed payload"),
        }));
    }
    Ok(())
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
    /// Prove that `product` is exactly the independently reusable output delta the
    /// scheduler authenticated. A persistent stage may not carry an upstream graph,
    /// blob, artifact, or handle merely because its implementation retained a
    /// cumulative carrier: any such lane is absent from `selection` and hard-fails.
    ///
    /// The default graph is the stage's native contribution surface and is bound by
    /// the product digest/dataset structural count. Named side lanes are the places a
    /// cumulative carrier can hide, so they require exact set equality here.
    fn validate_bounded_contribution(
        product: &StageProduct,
        selection: &ReceiptOutputSelection,
    ) -> Result<(), gmeow_errors::Diag> {
        fn exact_lane(
            stage: &str,
            lane: &str,
            actual: BTreeSet<String>,
            selected: &[String],
        ) -> Result<(), gmeow_errors::Diag> {
            let selected: BTreeSet<String> = selected.iter().cloned().collect();
            if actual == selected {
                return Ok(());
            }
            let unselected: Vec<String> = actual.difference(&selected).cloned().collect();
            let absent: Vec<String> = selected.difference(&actual).cloned().collect();
            Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage.to_string(),
                message: format!(
                    "persistent cache unit is not an exact bounded contribution on {lane}: \
                     product-only={unselected:?}, selection-only={absent:?}; cumulative \
                     carriers must use cacheRecomputeAggregate"
                ),
            }))
        }

        let bundle = product.bundle();
        let graphs = bundle
            .dataset()
            .owned_named_graphs()
            .map(|term| match term {
                purrdf::RdfTerm::Iri(iri) => Ok(iri),
                other => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: format!(
                        "persistent contribution carries a non-IRI named graph {other:?}"
                    ),
                })),
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        exact_lane(&product.stage_id, "named graphs", graphs, &selection.graphs)?;

        let mut blob_representations = BTreeSet::new();
        for record in &bundle.lookaside().blobs {
            let representation = record.representation.clone().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: "persistent contribution carries a blob without a representation"
                        .to_string(),
                })
            })?;
            blob_representations.insert(representation);
        }
        exact_lane(
            &product.stage_id,
            "blob representations",
            blob_representations,
            &selection.blob_representations,
        )?;

        let mut artifacts = BTreeSet::new();
        for resource in &bundle.lookaside().resources {
            let name = resource.name.clone().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: "persistent contribution carries a resource without a logical name"
                        .to_string(),
                })
            })?;
            if resource.content_digest.is_none() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: format!(
                        "persistent contribution artifact {name:?} has no content digest"
                    ),
                }));
            }
            artifacts.insert(name);
        }
        exact_lane(
            &product.stage_id,
            "logical artifacts",
            artifacts,
            &selection.logical_artifacts,
        )?;

        exact_lane(
            &product.stage_id,
            "typed handles",
            bundle.handles().keys().cloned().collect(),
            &selection.handles,
        )
    }

    /// Project a [`StageProduct`] into its serde manifest (every lane captured).
    fn from_product(
        product: &StageProduct,
        selection: &ReceiptOutputSelection,
    ) -> Result<Self, gmeow_errors::Diag> {
        Self::validate_bounded_contribution(product, selection)?;
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
            let arm = handle_arm_tag(&entry.payload);
            let payload_digest = handle_payload_digest(&entry.payload);
            let typed_payload = match &entry.payload {
                PipelineHandle::Logic(program) => {
                    Some(bincode::serialize(program.as_ref()).map_err(|error| {
                        gmeow_errors::Diag::of_kind(crate::error::Decode {
                            message: format!("cache: encode complete Logic handle IR: {error}"),
                        })
                    })?)
                }
                PipelineHandle::Reasoning(_)
                | PipelineHandle::RelationalCore(_)
                | PipelineHandle::Correspondence(_) => None,
            };
            let backing = Arc::new(bundle.dataset().project_named_graph(graph));
            let rebuilt = rebuild_handle(arm, backing, typed_payload.as_deref())?;
            let rebuilt_digest = handle_payload_digest(&rebuilt);
            if rebuilt_digest != payload_digest {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: stage {} typed handle <{graph}> ({arm}) is not losslessly \
                         reconstructible from its backing graph: live payload {payload_digest}, \
                         rebuilt payload {rebuilt_digest}; mark the stage recompute-only or add \
                         a lossless typed-handle codec before persistence",
                        product.stage_id
                    ),
                }));
            }
            handles.push(CachedHandle {
                graph: graph.clone(),
                arm: arm.to_string(),
                payload_digest,
                typed_payload,
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
            let payload = rebuild_handle(&h.arm, subgraph, h.typed_payload.as_deref())?;
            let payload_digest = handle_payload_digest(&payload);
            if payload_digest != h.payload_digest {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("{}:{}", h.graph, h.payload_digest),
                    actual: format!("{}:{payload_digest}", h.graph),
                }));
            }
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

    /// Extract and authenticate the committed artifact lane without restoring the
    /// packed dataset or rebuilding any typed handle.
    fn verified_artifacts(
        &self,
        receipt: &StageReceipt,
    ) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
        let mut references: BTreeMap<&str, &str> = BTreeMap::new();
        for resource in &self.lookaside.resources {
            let (Some(name), Some(digest)) =
                (resource.name.as_deref(), resource.content_digest.as_deref())
            else {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: stage {} logical artifact resource lacks name or digest",
                        self.stage_id
                    ),
                }));
            };
            if references.insert(name, digest).is_some() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: stage {} carries duplicate logical artifact {name:?}",
                        self.stage_id
                    ),
                }));
            }
        }

        let expected_names: BTreeSet<&str> = receipt
            .logical_artifacts
            .iter()
            .map(|entity| entity.identity.as_str())
            .collect();
        if expected_names.len() != receipt.logical_artifacts.len() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cache: stage {} receipt carries duplicate logical-artifact identities",
                    self.stage_id
                ),
            }));
        }
        let actual_names: BTreeSet<&str> = references.keys().copied().collect();
        if actual_names != expected_names {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: format!("logical artifacts {expected_names:?}"),
                actual: format!("logical artifacts {actual_names:?}"),
            }));
        }

        let mut artifacts = BTreeMap::new();
        for entity in &receipt.logical_artifacts {
            if entity.structural_count != 1 {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("{} structural-count=1", entity.identity),
                    actual: format!(
                        "{} structural-count={}",
                        entity.identity, entity.structural_count
                    ),
                }));
            }
            let referenced_digest = references[entity.identity.as_str()];
            if referenced_digest != entity.digest {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("{}:{}", entity.identity, entity.digest),
                    actual: format!("{}:{referenced_digest}", entity.identity),
                }));
            }
            let bytes = self.blobs.get(referenced_digest).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: referenced_digest.to_string(),
                    actual: format!("<missing artifact blob for {}>", entity.identity),
                })
            })?;
            let actual_digest = ContentDigest::of(bytes).to_hex();
            let actual_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if actual_digest != entity.digest || actual_bytes != entity.decoded_bytes {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("{}:{}", entity.digest, entity.decoded_bytes),
                    actual: format!("{actual_digest}:{actual_bytes}"),
                }));
            }
            artifacts.insert(entity.identity.clone(), bytes.clone());
        }
        Ok(artifacts)
    }
}

/// The pipeline bundle alias the cache reconstitutes (`PipelineBundle<PipelineHandle>`).
type PipelineBundleAlias = purrdf::PipelineBundle<PipelineHandle>;

// ── On-disk content-addressed cache ──────────────────────────────────────────

/// The persistent per-stage cache under `.cache/gmeow-sync/pipeline/<version>/`
/// (gitignored and worktree-local).
///
/// `receipts/<action-key>.json` is the immutable root for one action and
/// `blobs/<content-digest>` holds the bincode-serialized [`CachedBundle`]. There is
/// no mutable global index: writers of different keys cannot erase one another and
/// writers of the same key must agree byte-for-byte.
pub struct PipelineCache {
    dir: PathBuf,
    max_entries: usize,
    max_bytes: u64,
    /// Shared lease on the build-fingerprint namespace for this handle's lifetime.
    /// Namespace GC must win the corresponding exclusive lock before removal.
    _namespace_lease: Option<File>,
}

struct CacheLock {
    file: File,
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl PipelineCache {
    /// Construct an inert cache handle without touching the filesystem.
    ///
    /// [`crate::scheduler::RunContext::open_uncached`] uses this for explicit
    /// diagnostic/test runs: scheduler cache probes and writes are disabled, so the
    /// path is never read.
    pub fn inert() -> Self {
        Self {
            dir: PathBuf::new(),
            max_entries: 0,
            max_bytes: 0,
            _namespace_lease: None,
        }
    }

    /// The conventional cache base directory under a repo root. [`open`](Self::open)
    /// appends the version segment, so this is the un-segmented base.
    pub fn default_dir(root: &Path) -> PathBuf {
        root.join(".cache").join("gmeow-sync").join("pipeline")
    }

    /// Open (or create) the cache rooted at `dir`. The on-disk
    /// store lives under a `v<CACHE_VERSION>` leaf of `dir` so a prior cache-shape
    /// rev is isolated — a shape bump makes every older cache a clean miss
    /// (greenfield, no migration).
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, gmeow_errors::Diag> {
        let namespace = dir.into();
        fs::create_dir_all(&namespace)?;
        let parent = namespace.parent().unwrap_or(&namespace);
        let namespace_guard = open_lock(&parent.join(".pipeline-cache-namespaces.lock"))?;
        namespace_guard.lock_shared()?;
        let lease = open_lock(&namespace.join(".lease.lock"))?;
        lease.lock_shared()?;
        let dir = namespace.join(format!("v{CACHE_VERSION}"));
        fs::create_dir_all(dir.join("blobs"))?;
        fs::create_dir_all(dir.join("receipts"))?;
        fs::create_dir_all(dir.join("locks"))?;
        namespace_guard.unlock()?;
        Ok(Self {
            dir,
            max_entries: MAX_CACHE_ENTRIES,
            max_bytes: MAX_CACHE_BYTES,
            _namespace_lease: Some(lease),
        })
    }

    /// Bound build-fingerprint namespaces while protecting every live cache handle.
    /// The current namespace and the newest prior namespace are retained; older idle
    /// namespaces are removed. A live sibling is skipped, never reaped by age.
    pub fn prune_namespaces(
        base: &Path,
        current: &str,
        max_namespaces: usize,
    ) -> Result<(), gmeow_errors::Diag> {
        fs::create_dir_all(base)?;
        let guard = open_lock(&base.join(".pipeline-cache-namespaces.lock"))?;
        guard.lock()?;
        let mut namespaces: Vec<(PathBuf, std::time::SystemTime)> = fs::read_dir(base)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((entry.path(), modified))
            })
            .collect();
        namespaces.sort_by(|left, right| (&left.1, &left.0).cmp(&(&right.1, &right.0)));
        let mut retained = namespaces.len();
        for (namespace, _) in namespaces {
            if retained <= max_namespaces.max(1) {
                break;
            }
            if namespace.file_name().is_some_and(|name| name == current) {
                continue;
            }
            let lease = open_lock(&namespace.join(".lease.lock"))?;
            match lease.try_lock() {
                Ok(()) => {
                    fs::remove_dir_all(&namespace)?;
                    retained = retained.saturating_sub(1);
                    let _ = lease.unlock();
                }
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        guard.unlock()?;
        Ok(())
    }

    #[cfg(test)]
    fn with_limits(mut self, max_entries: usize, max_bytes: u64) -> Self {
        self.max_entries = max_entries;
        self.max_bytes = max_bytes;
        self
    }

    /// Look up a stage product by cache key. Returns `None` on a miss. HARD-fails
    /// (`CacheMismatch`) if the blob exists but its re-hashed digest disagrees
    /// with the index — the cache is never silently repaired.
    pub fn get(&self, context: &StageKeyContext) -> Result<Option<CacheHit>, gmeow_errors::Diag> {
        let key = stage_key(context);
        let _store = self.lock_store(false)?;
        let _action = self.lock_action(&key, false)?;
        let receipt_path = self.receipt_path(&key);
        if !receipt_path.exists() {
            return Ok(None);
        }
        let receipt = self.read_receipt(&receipt_path)?;
        if receipt.action_key != key || receipt.context != *context {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: key,
                actual: receipt.action_key,
            }));
        }
        let digest_hex = receipt.product_blob_digest.as_ref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "persistent receipt {} carries no product blob digest",
                    receipt.action_key
                ),
            })
        })?;
        let blob_path = self.dir.join("blobs").join(digest_hex);
        if !blob_path.exists() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: digest_hex.clone(),
                actual: "<missing blob>".to_string(),
            }));
        }
        if receipt.product_blob_bytes > MAX_ENTRY_BYTES
            || receipt.product_blob_bytes > self.max_bytes
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle declares {} bytes, above its configured byte quota \
                     (store={}, entry-ceiling={MAX_ENTRY_BYTES})",
                    receipt.product_blob_bytes, self.max_bytes
                ),
            }));
        }
        let bytes = read_bounded(&blob_path, MAX_ENTRY_BYTES, "pipeline cache product blob")?;
        let actual = ContentDigest::of(&bytes).to_hex();
        if actual != *digest_hex {
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
        if cached.version != CACHE_VERSION {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle version {} != expected {CACHE_VERSION}",
                    cached.version
                ),
            }));
        }
        let product = cached.into_product()?;
        if product.stage_id != context.stage_id || product.digest != receipt.product_digest {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: format!("{}:{}", context.stage_id, receipt.product_digest),
                actual: format!("{}:{}", product.stage_id, product.digest),
            }));
        }
        let hydrated_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if hydrated_bytes != receipt.product_blob_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: receipt.product_blob_bytes.to_string(),
                actual: hydrated_bytes.to_string(),
            }));
        }
        Ok(Some(CacheHit {
            product,
            receipt,
            hydrated_bytes,
        }))
    }

    /// Authenticate a current receipt and its referenced product blob without
    /// deserializing or reconstructing the product.
    ///
    /// This supplies action identities for a receipt-only dependency walk. A missing
    /// receipt is an ordinary cache miss; a present but missing/corrupt blob hard-fails.
    pub fn inspect_receipt(
        &self,
        context: &StageKeyContext,
    ) -> Result<Option<StageReceipt>, gmeow_errors::Diag> {
        let key = stage_key(context);
        let _store = self.lock_store(false)?;
        let _action = self.lock_action(&key, false)?;
        let receipt_path = self.receipt_path(&key);
        if !receipt_path.exists() {
            return Ok(None);
        }
        let receipt = self.read_receipt(&receipt_path)?;
        if receipt.action_key != key || receipt.context != *context {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: key,
                actual: receipt.action_key,
            }));
        }
        let digest = receipt.product_blob_digest.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "persistent receipt {} carries no product blob digest",
                    receipt.action_key
                ),
            })
        })?;
        let blob_path = self.dir.join("blobs").join(digest);
        if !blob_path.exists() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: digest.to_string(),
                actual: "<missing blob>".to_string(),
            }));
        }
        self.verify_blob(digest, receipt.product_blob_bytes)?;
        Ok(Some(receipt))
    }

    /// Load only the committed logical-artifact lane from a verified product blob.
    ///
    /// The complete bincode manifest is authenticated and decoded, but its packed RDF
    /// image is never passed to `restore_pack`; no dataset indexes or typed handles are
    /// constructed. Every selected path, digest, byte count, and structural count is
    /// checked against the immutable production receipt before bytes are returned.
    pub fn get_artifacts(
        &self,
        context: &StageKeyContext,
    ) -> Result<Option<ArtifactCacheHit>, gmeow_errors::Diag> {
        let key = stage_key(context);
        let _store = self.lock_store(false)?;
        let _action = self.lock_action(&key, false)?;
        let receipt_path = self.receipt_path(&key);
        if !receipt_path.exists() {
            return Ok(None);
        }
        let receipt = self.read_receipt(&receipt_path)?;
        if receipt.action_key != key || receipt.context != *context {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: key,
                actual: receipt.action_key,
            }));
        }
        let digest = receipt.product_blob_digest.as_ref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "persistent receipt {} carries no product blob digest",
                    receipt.action_key
                ),
            })
        })?;
        let blob_path = self.dir.join("blobs").join(digest);
        if !blob_path.exists() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: digest.clone(),
                actual: "<missing blob>".to_string(),
            }));
        }
        if receipt.product_blob_bytes > MAX_ENTRY_BYTES
            || receipt.product_blob_bytes > self.max_bytes
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle declares {} bytes, above its configured byte quota \
                     (store={}, entry-ceiling={MAX_ENTRY_BYTES})",
                    receipt.product_blob_bytes, self.max_bytes
                ),
            }));
        }
        let bytes = read_bounded(&blob_path, MAX_ENTRY_BYTES, "pipeline cache product blob")?;
        let actual_digest = ContentDigest::of(&bytes).to_hex();
        let transferred_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_digest != *digest || transferred_bytes != receipt.product_blob_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: format!("{digest}:{}", receipt.product_blob_bytes),
                actual: format!("{actual_digest}:{transferred_bytes}"),
            }));
        }
        let cached: CachedBundle = bincode::deserialize(&bytes).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("corrupt cached bundle: {error}"),
            })
        })?;
        if cached.version != CACHE_VERSION {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle version {} != expected {CACHE_VERSION}",
                    cached.version
                ),
            }));
        }
        if cached.stage_id != context.stage_id || cached.digest != receipt.product_digest {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: format!("{}:{}", context.stage_id, receipt.product_digest),
                actual: format!("{}:{}", cached.stage_id, cached.digest),
            }));
        }
        let artifacts = cached.verified_artifacts(&receipt)?;
        Ok(Some(ArtifactCacheHit {
            artifacts,
            receipt,
            transferred_bytes,
        }))
    }

    /// Store a stage product under its typed action context and publish its immutable
    /// receipt. Same-key writers must produce the same receipt; otherwise the action
    /// is nondeterministic and publication hard-fails.
    pub fn put(
        &self,
        context: &StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        selection: &ReceiptOutputSelection,
        product: &StageProduct,
    ) -> Result<StageReceipt, gmeow_errors::Diag> {
        let manifest = CachedBundle::from_product(product, selection)?;
        let bytes = bincode::serialize(&manifest).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cannot serialize cached bundle: {e}"),
            })
        })?;
        let serialized_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if serialized_bytes > MAX_ENTRY_BYTES || serialized_bytes > self.max_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: context.stage_id.clone(),
                message: format!(
                    "cache entry is {serialized_bytes} bytes, above its configured \
                     byte quota (store={}, entry-ceiling={MAX_ENTRY_BYTES})",
                    self.max_bytes
                ),
            }));
        }
        let digest_hex = ContentDigest::of(&bytes).to_hex();
        let receipt = StageReceipt::from_product(
            context.clone(),
            stability,
            cache_disposition,
            selection,
            product,
            Some(digest_hex.clone()),
            serialized_bytes,
        )?;
        let key = receipt.action_key.clone();
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.digest(),
            receipt: receipt.clone(),
        };
        let receipt_bytes = serde_json::to_vec_pretty(&envelope).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("cannot serialize stage receipt: {e}"),
            })
        })?;

        {
            let _store = self.lock_store(false)?;
            let _action = self.lock_action(&key, true)?;
            let receipt_path = self.receipt_path(&key);
            if receipt_path.exists() {
                let existing = self.read_receipt(&receipt_path)?;
                if existing != receipt {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                        expected: existing.digest(),
                        actual: receipt.digest(),
                    }));
                }
                // An identical receipt must still have an intact reachable product.
                self.verify_blob(&digest_hex, serialized_bytes)?;
                return Ok(existing);
            }
            write_content_addressed(&self.dir.join("blobs").join(&digest_hex), &bytes)?;
            write_atomic(&receipt_path, &receipt_bytes)?;
        }
        self.prune(Some(&key))?;
        Ok(receipt)
    }

    /// Build a deterministic non-persisted receipt for a recomputed aggregate.
    pub fn receipt_only(
        context: &StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        selection: &ReceiptOutputSelection,
        product: &StageProduct,
    ) -> Result<StageReceipt, gmeow_errors::Diag> {
        StageReceipt::from_product(
            context.clone(),
            stability,
            cache_disposition,
            selection,
            product,
            None,
            0,
        )
    }

    /// Rebuild the deterministic receipt projection from a hydrated product and
    /// compare it with the stored receipt. This catches structurally incomplete but
    /// otherwise digest-valid receipts (for example, a removed graph/handle row).
    pub fn validate_hit_receipt(
        context: &StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        selection: &ReceiptOutputSelection,
        hit: &CacheHit,
    ) -> Result<(), gmeow_errors::Diag> {
        let expected = StageReceipt::from_product(
            context.clone(),
            stability,
            cache_disposition,
            selection,
            &hit.product,
            hit.receipt.product_blob_digest.clone(),
            hit.receipt.product_blob_bytes,
        )?;
        if expected != hit.receipt {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: expected.digest(),
                actual: hit.receipt.digest(),
            }));
        }
        Ok(())
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        fs::read_dir(self.dir.join("receipts"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| receipt_key(&entry.path()).is_some())
                    .count()
            })
            .unwrap_or(0)
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn receipt_path(&self, key: &str) -> PathBuf {
        self.dir.join("receipts").join(format!("{key}.json"))
    }

    fn lock_store(&self, exclusive: bool) -> Result<CacheLock, gmeow_errors::Diag> {
        self.lock_path(&self.dir.join("locks").join("store.lock"), exclusive)
    }

    fn lock_action(&self, key: &str, exclusive: bool) -> Result<CacheLock, gmeow_errors::Diag> {
        // Fixed 256-way lock striping bounds sidecar growth without sacrificing the
        // same-key exclusion law. Unrelated keys sharing a stripe may serialize only
        // during their short publish/read boundary; stage compute remains parallel.
        let stripe = key.get(..2).unwrap_or("00");
        self.lock_path(
            &self.dir.join("locks").join(format!("action-{stripe}.lock")),
            exclusive,
        )
    }

    fn lock_path(&self, path: &Path, exclusive: bool) -> Result<CacheLock, gmeow_errors::Diag> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        if exclusive {
            file.lock()?;
        } else {
            file.lock_shared()?;
        }
        Ok(CacheLock { file })
    }

    fn read_receipt(&self, path: &Path) -> Result<StageReceipt, gmeow_errors::Diag> {
        let bytes = read_bounded(path, MAX_RECEIPT_BYTES, "pipeline cache receipt")?;
        let envelope: ReceiptEnvelope = serde_json::from_slice(&bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("corrupt pipeline cache receipt {}: {e}", path.display()),
            })
        })?;
        if envelope.receipt.schema_version != RECEIPT_SCHEMA_VERSION {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "receipt schema {} != expected {RECEIPT_SCHEMA_VERSION}",
                    envelope.receipt.schema_version
                ),
            }));
        }
        let actual = envelope.receipt.digest();
        if actual != envelope.receipt_digest {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: envelope.receipt_digest,
                actual,
            }));
        }
        Ok(envelope.receipt)
    }

    fn verify_blob(&self, digest: &str, expected_bytes: u64) -> Result<(), gmeow_errors::Diag> {
        let path = self.dir.join("blobs").join(digest);
        if expected_bytes > MAX_ENTRY_BYTES || expected_bytes > self.max_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "cached bundle declares {expected_bytes} bytes, above its configured byte \
                     quota (store={}, entry-ceiling={MAX_ENTRY_BYTES})",
                    self.max_bytes
                ),
            }));
        }
        let (actual, actual_bytes) =
            digest_bounded(&path, MAX_ENTRY_BYTES, "pipeline cache product blob")?;
        if actual != digest || actual_bytes != expected_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: format!("{digest}:{expected_bytes}"),
                actual: format!("{actual}:{actual_bytes}"),
            }));
        }
        Ok(())
    }

    /// Enforce entry/byte quotas by removing oldest immutable receipt roots, then
    /// deleting only blobs unreachable from every remaining root. Active readers hold
    /// the store's shared lock, so the exclusive lock protects the whole reachability
    /// scan and deletion transaction.
    fn prune(&self, protected_key: Option<&str>) -> Result<(), gmeow_errors::Diag> {
        let _store = self.lock_store(true)?;
        let receipts_dir = self.dir.join("receipts");
        let mut roots: Vec<(PathBuf, std::time::SystemTime, StageReceipt)> = Vec::new();
        for entry in fs::read_dir(&receipts_dir)? {
            let entry = entry?;
            if !entry.path().is_file() {
                continue;
            }
            if receipt_key(&entry.path()).is_none() {
                fs::remove_file(entry.path())?;
                continue;
            }
            let modified = entry
                .metadata()?
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            roots.push((entry.path(), modified, self.read_receipt(&entry.path())?));
        }
        roots.sort_by(|left, right| (&left.1, &left.0).cmp(&(&right.1, &right.0)));

        let mut blob_ref_counts: BTreeMap<String, usize> = BTreeMap::new();
        for digest in roots
            .iter()
            .filter_map(|(_, _, receipt)| receipt.product_blob_digest.clone())
        {
            *blob_ref_counts.entry(digest).or_default() += 1;
        }
        let mut rooted_blobs: BTreeSet<String> = blob_ref_counts.keys().cloned().collect();
        let mut bytes = rooted_blobs.iter().try_fold(0_u64, |sum, digest| {
            let len = fs::metadata(self.dir.join("blobs").join(digest))?.len();
            Ok::<u64, std::io::Error>(sum.saturating_add(len))
        })?;
        let mut retained = roots.len();
        for (path, _, receipt) in &roots {
            if retained <= self.max_entries && bytes <= self.max_bytes {
                break;
            }
            let Some(key) = receipt_key(path) else {
                continue;
            };
            if protected_key.is_some_and(|protected| protected == key) {
                continue;
            }
            let stripe = key.get(..2).unwrap_or("00");
            let action_file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(self.dir.join("locks").join(format!("action-{stripe}.lock")))?;
            match action_file.try_lock() {
                Ok(()) => {
                    fs::remove_file(path)?;
                    retained = retained.saturating_sub(1);
                    if let Some(digest) = &receipt.product_blob_digest
                        && let Some(count) = blob_ref_counts.get_mut(digest)
                    {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            bytes = bytes.saturating_sub(receipt.product_blob_bytes);
                            rooted_blobs.remove(digest);
                        }
                    }
                    let _ = action_file.unlock();
                }
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }

        // Re-read the retained roots rather than trusting the pre-prune set; this also
        // makes an externally removed receipt a conservative blob deletion candidate.
        rooted_blobs.clear();
        for entry in fs::read_dir(&receipts_dir)? {
            let entry = entry?;
            if receipt_key(&entry.path()).is_some()
                && let Some(digest) = self.read_receipt(&entry.path())?.product_blob_digest
            {
                rooted_blobs.insert(digest);
            }
        }
        for entry in fs::read_dir(self.dir.join("blobs"))? {
            let entry = entry?;
            if entry.path().is_file()
                && !rooted_blobs.contains(entry.file_name().to_string_lossy().as_ref())
            {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}

/// Result of one cross-process fixture action. `built` is observational telemetry;
/// it is deliberately absent from the immutable [`StageReceipt`].
#[derive(Debug)]
pub struct FixtureOutcome {
    /// Verified fixture product, fresh or hydrated.
    pub product: StageProduct,
    /// Immutable receipt shared by cold and warm execution.
    pub receipt: StageReceipt,
    /// `true` only for the process elected to execute the producer.
    pub built: bool,
    /// Serialized bytes written on a build or read on a hit.
    pub transferred_bytes: u64,
}

/// Cross-process fixture coordinator using the production action-key, receipt, and
/// product-blob authorities.
///
/// The coordinator adds only the election missing from an ordinary stage-cache probe:
/// a blocking, per-action OS lock held across a cache recheck and the exact producer.
/// There is no elapsed correctness ceiling. A live builder may take as long as its
/// declared action requires; a dead builder releases the kernel lock automatically.
pub struct FixtureCoordinator {
    cache: PipelineCache,
    lock_dir: PathBuf,
}

impl FixtureCoordinator {
    /// Open the worktree-local fixture namespace for the current executable identity.
    pub fn open(root: &Path) -> Result<Self, gmeow_errors::Diag> {
        // Fixtures and the production scheduler intentionally share ONE immutable
        // receipt/blob authority. Priming a fixture therefore warms the exact stage
        // action a later full DAG run probes; there is no shadow fixture producer or
        // duplicate cache namespace.
        let base = PipelineCache::default_dir(root);
        let fingerprint = &BUILD_FINGERPRINT[..16];
        let cache = PipelineCache::open(base.join(fingerprint))?;
        PipelineCache::prune_namespaces(&base, fingerprint, 2)?;
        let lock_dir = root
            .join(".cache")
            .join("gmeow-sync")
            .join("pipeline-fixture-locks");
        fs::create_dir_all(&lock_dir)?;
        Ok(Self { cache, lock_dir })
    }

    /// Load or build exactly one fixture action.
    ///
    /// `select` is the same output-delta projection the scheduler receipts. It runs
    /// for cold and warm products, so a declaration/fixture drift fails on either path.
    /// A miss is rechecked after acquiring the action lock; only the elected process
    /// calls `build`.
    pub fn get_or_build<B, S>(
        &self,
        context: &StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        select: S,
        build: B,
    ) -> Result<FixtureOutcome, gmeow_errors::Diag>
    where
        B: FnOnce() -> Result<StageProduct, gmeow_errors::Diag>,
        S: Fn(&StageProduct) -> Result<ReceiptOutputSelection, gmeow_errors::Diag>,
    {
        if let Some(hit) = self.cache.get(context)? {
            let selection = select(&hit.product)?;
            PipelineCache::validate_hit_receipt(
                context,
                stability,
                cache_disposition,
                &selection,
                &hit,
            )?;
            return Ok(FixtureOutcome {
                product: hit.product,
                receipt: hit.receipt,
                built: false,
                transferred_bytes: hit.hydrated_bytes,
            });
        }

        let key = stage_key(context);
        let action_file = open_lock(&self.lock_dir.join(format!("{key}.lock")))?;
        action_file.lock()?;
        let _election = CacheLock { file: action_file };

        // Another process may have published while this process waited for election.
        if let Some(hit) = self.cache.get(context)? {
            let selection = select(&hit.product)?;
            PipelineCache::validate_hit_receipt(
                context,
                stability,
                cache_disposition,
                &selection,
                &hit,
            )?;
            return Ok(FixtureOutcome {
                product: hit.product,
                receipt: hit.receipt,
                built: false,
                transferred_bytes: hit.hydrated_bytes,
            });
        }

        let product = build()?;
        let selection = select(&product)?;
        let receipt =
            self.cache
                .put(context, stability, cache_disposition, &selection, &product)?;
        let transferred_bytes = receipt.product_blob_bytes;
        Ok(FixtureOutcome {
            product,
            receipt,
            built: true,
            transferred_bytes,
        })
    }
}

fn receipt_key(path: &Path) -> Option<String> {
    if path.extension()?.to_str()? != "json" {
        return None;
    }
    let key = path.file_stem()?.to_str()?;
    (key.len() == 64 && key.chars().all(|character| character.is_ascii_hexdigit()))
        .then(|| key.to_string())
}

fn open_lock(path: &Path) -> Result<File, gmeow_errors::Diag> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?)
}

/// Read one cache object without ever allocating past its lane's admission bound.
/// Metadata is an early rejection only; `take(max + 1)` closes the growth race between
/// `metadata` and `read_to_end`, and the final length check rejects that extra byte.
fn read_bounded(path: &Path, max_bytes: u64, lane: &str) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!(
                "{lane} {} is not a regular file within the {max_bytes}-byte bound",
                path.display()
            ),
        }));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!(
                "{lane} {} grew beyond the {max_bytes}-byte bound while being read",
                path.display()
            ),
        }));
    }
    Ok(bytes)
}

/// Stream-authenticate a bounded cache object without allocating its payload.
fn digest_bounded(
    path: &Path,
    max_bytes: u64,
    lane: &str,
) -> Result<(String, u64), gmeow_errors::Diag> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!(
                "{lane} {} is not a regular file within the {max_bytes}-byte bound",
                path.display()
            ),
        }));
    }
    let mut reader = File::open(path)?.take(max_bytes.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > max_bytes {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!(
                    "{lane} {} grew beyond the {max_bytes}-byte bound while being read",
                    path.display()
                ),
            }));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
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
    let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if let Ok(existing) = read_bounded(target, expected_bytes, "pipeline cache publication")
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
    let mut tmp = tempfile::Builder::new()
        .prefix(".pipeline-cache-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(target).map_err(|error| error.error)?;
    File::open(dir)?.sync_all()?;
    Ok(())
}

/// Publish immutable content-addressed bytes. An existing equal blob is reused; an
/// existing unequal blob under the same digest is corruption and hard-fails rather
/// than being silently repaired.
fn write_content_addressed(target: &Path, bytes: &[u8]) -> Result<(), gmeow_errors::Diag> {
    let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    match read_bounded(
        target,
        expected_bytes,
        "pipeline content-addressed publication",
    ) {
        Ok(existing) if existing == bytes => return Ok(()),
        Ok(existing) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: ContentDigest::of(bytes).to_hex(),
                actual: ContentDigest::of(&existing).to_hex(),
            }));
        }
        Err(error) => match target.try_exists() {
            Ok(false) => {}
            Ok(true) => return Err(error),
            Err(io_error) => return Err(io_error.into()),
        },
    }
    let dir = target.parent().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            message: format!("cache blob path {} has no parent", target.display()),
        })
    })?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".pipeline-blob-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    match tmp.persist_noclobber(target) {
        Ok(_) => {
            File::open(dir)?.sync_all()?;
            Ok(())
        }
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_bounded(
                target,
                expected_bytes,
                "concurrent pipeline content-addressed publication",
            )?;
            if existing == bytes {
                Ok(())
            } else {
                Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: ContentDigest::of(bytes).to_hex(),
                    actual: ContentDigest::of(&existing).to_hex(),
                }))
            }
        }
        Err(error) => Err(error.error.into()),
    }
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

    fn test_context(stage_id: &str, salt: &str) -> StageKeyContext {
        StageKeyContext::new(stage_id, "test-v1", Vec::new(), Vec::new())
            .with_dimension("test-salt", salt)
    }

    #[test]
    fn selected_dimension_is_a_first_class_action_key_input() {
        let base = StageKeyContext::new("stage", "test-v1", Vec::new(), Vec::new());
        let english = base.clone().with_dimension("language", "en");
        let french = base.clone().with_dimension("language", "fr");
        let docs = base.with_dimension("output", "docs");
        assert_ne!(stage_key(&english), stage_key(&french));
        assert_ne!(stage_key(&english), stage_key(&docs));
        assert_eq!(
            stage_key(&english),
            stage_key(
                &StageKeyContext::new("stage", "test-v1", vec![], vec![])
                    .with_dimension("language", "en")
            ),
            "identical explicit feature selections must be cache-stable",
        );
    }

    fn full_selection(product: &StageProduct) -> ReceiptOutputSelection {
        ReceiptOutputSelection {
            graphs: product_graph_names(product),
            blob_representations: product
                .bundle()
                .lookaside()
                .blobs
                .iter()
                .filter_map(|record| record.representation.clone())
                .collect(),
            logical_artifacts: product
                .bundle()
                .lookaside()
                .resources
                .iter()
                .filter_map(|resource| resource.name.clone())
                .collect(),
            handles: product.bundle().handles().keys().cloned().collect(),
        }
    }

    fn product_graph_names(product: &StageProduct) -> Vec<String> {
        product
            .dataset()
            .owned_named_graphs()
            .filter_map(|term| match term {
                RdfTerm::Iri(iri) => Some(iri),
                _ => None,
            })
            .collect()
    }

    fn persist_test_product(
        cache: &PipelineCache,
        context: &StageKeyContext,
        product: &StageProduct,
    ) -> StageReceipt {
        cache
            .put(
                context,
                "stable",
                "persistent",
                &full_selection(product),
                product,
            )
            .unwrap()
    }

    fn write_test_receipt(cache: &PipelineCache, receipt: StageReceipt) {
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.digest(),
            receipt,
        };
        std::fs::write(
            cache.receipt_path(&envelope.receipt.action_key),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();
    }

    /// A tiny but real [`LogicProgram`] whose canonical RDF-1.2 projection backs the
    /// cache's `Logic` handle. The cache persists this complete typed value separately
    /// because the governed graph projection is deliberately lossy.
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
        let cache = PipelineCache::open(dir.path()).unwrap();

        let original = rich_bundle();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(original.clone()));
        let context = test_context("stage-rich", "structural-round-trip");
        persist_test_product(&cache, &context, &product);

        let got = cache.get(&context).unwrap().expect("cache hit");
        let recon = got.product.bundle();

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
        // The complete typed Logic handle (C6) survives serialization while remaining
        // pinned to its governed `graph/logic` projection.
        assert_eq!(
            reconstituted.canonical_key(),
            sample_logic_program().canonical_key(),
            "the cache preserved the Logic handle's program canonical-key-equal"
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
            got.product.artifacts(),
            product.artifacts(),
            "byte-artifact lane reproduced exactly"
        );
        // The product's cache-key digest is preserved too.
        assert_eq!(
            got.product.digest, product.digest,
            "stage-product digest preserved"
        );
    }

    #[test]
    fn selective_artifact_hit_matches_full_hydration_and_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        let context = test_context("stage-rich", "artifact-only-hit");
        let receipt = persist_test_product(&cache, &context, &product);

        assert_eq!(
            cache
                .inspect_receipt(&context)
                .expect("receipt inspection")
                .expect("receipt exists"),
            receipt
        );
        let selective = cache
            .get_artifacts(&context)
            .expect("selective lookup")
            .expect("selective hit");
        let full = cache.get(&context).expect("full lookup").expect("full hit");
        assert_eq!(selective.receipt, full.receipt);
        assert_eq!(selective.artifacts, full.product.artifacts());
        assert_eq!(
            selective.transferred_bytes, full.hydrated_bytes,
            "both paths authenticate the same complete product blob"
        );
    }

    #[test]
    fn selective_artifact_hit_rejects_inner_digest_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        let context = test_context("stage-rich", "artifact-inner-corruption");
        let mut receipt = persist_test_product(&cache, &context, &product);
        let original_blob = cache
            .dir
            .join("blobs")
            .join(receipt.product_blob_digest.as_ref().unwrap());
        let mut manifest: CachedBundle =
            bincode::deserialize(&std::fs::read(original_blob).unwrap()).unwrap();
        let artifact_digest = manifest.lookaside.resources[0]
            .content_digest
            .clone()
            .expect("artifact digest");
        manifest.blobs.get_mut(&artifact_digest).unwrap()[0] ^= 0xff;

        // Keep the enclosing product blob self-consistent so the selective reader must
        // reach and enforce the receipt's artifact-level digest, not merely the outer hash.
        let corrupted = bincode::serialize(&manifest).unwrap();
        let corrupted_digest = ContentDigest::of(&corrupted).to_hex();
        std::fs::write(cache.dir.join("blobs").join(&corrupted_digest), &corrupted).unwrap();
        receipt.product_blob_digest = Some(corrupted_digest);
        receipt.product_blob_bytes = u64::try_from(corrupted.len()).unwrap();
        write_test_receipt(&cache, receipt);

        let error = cache
            .get_artifacts(&context)
            .expect_err("artifact digest mismatch must hard-fail");
        assert!(error.is::<crate::error::CacheMismatch>(), "{error}");
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
                payload_digest: "payload-digest".to_owned(),
                typed_payload: Some(payload.clone()),
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
    fn persistent_unit_rejects_unselected_cumulative_lanes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        let context = test_context("stage-rich", "bounded-delta");

        let mut missing_graph = full_selection(&product);
        missing_graph.graphs.clear();
        let err = cache
            .put(&context, "stable", "persistent", &missing_graph, &product)
            .expect_err("an unselected graph is cumulative carrier residue");
        assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

        let mut missing_artifact = full_selection(&product);
        missing_artifact.logical_artifacts.clear();
        let err = cache
            .put(
                &context,
                "stable",
                "persistent",
                &missing_artifact,
                &product,
            )
            .expect_err("an unselected artifact is cumulative carrier residue");
        assert!(err.is::<crate::error::StageFailed>(), "got {err:?}");

        assert_eq!(cache.len(), 0, "a rejected unit publishes no receipt");
    }

    #[test]
    fn tampered_handle_manifest_hard_fails_on_reload() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();

        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        let context = test_context("stage-rich", "tampered-handle");
        let mut receipt = persist_test_product(&cache, &context, &product);

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
        // Re-serialize + re-file under the NEW content digest (and re-point the receipt),
        // so the blob still self-verifies and we exercise the handle re-pin path.
        let new_bytes = bincode::serialize(&manifest).unwrap();
        let new_hex = ContentDigest::of(&new_bytes).to_hex();
        std::fs::write(blobs_dir.join(&new_hex), &new_bytes).unwrap();
        receipt.product_blob_digest = Some(new_hex);
        receipt.product_blob_bytes = u64::try_from(new_bytes.len()).unwrap();
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.digest(),
            receipt,
        };
        std::fs::write(
            cache.receipt_path(&stage_key(&context)),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();
        let reopened = PipelineCache::open(dir.path()).unwrap();

        let err = reopened
            .get(&context)
            .expect_err("a stale/dropped handle must hard-fail");
        assert!(
            err.is::<crate::error::Decode>(),
            "tampered handle manifest hard-fails, got {err:?}"
        );
    }

    #[test]
    fn receipt_is_cold_warm_identical_and_structurally_complete() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let product = StageProduct::from_bundle("stage-rich", Arc::new(rich_bundle()));
        let context = test_context("stage-rich", "receipt-parity");
        let selection = full_selection(&product);
        let cold = persist_test_product(&cache, &context, &product);
        let warm = cache.get(&context).unwrap().expect("cache hit");
        assert_eq!(cold, warm.receipt);
        assert_eq!(cold.digest(), warm.receipt.digest());
        assert_eq!(cold.graphs.len(), 1);
        assert_eq!(cold.typed_handles.len(), 1);
        assert_eq!(cold.logical_artifacts.len(), 1);
        PipelineCache::validate_hit_receipt(&context, "stable", "persistent", &selection, &warm)
            .unwrap();

        // A self-consistent envelope that silently drops an output row is still
        // structurally invalid against the live stage declaration/product.
        let mut incomplete = cold;
        incomplete.graphs.clear();
        write_test_receipt(&cache, incomplete);
        let hit = cache.get(&context).unwrap().expect("blob remains readable");
        assert!(
            PipelineCache::validate_hit_receipt(
                &context,
                "stable",
                "persistent",
                &selection,
                &hit,
            )
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
        );
    }

    #[test]
    fn receipt_and_blob_corruption_matrix_hard_fails() {
        // Truncated receipt.
        let truncated = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(truncated.path()).unwrap();
        let product = StageProduct::new("stage", "digest");
        let context = test_context("stage", "truncated");
        persist_test_product(&cache, &context, &product);
        std::fs::write(cache.receipt_path(&stage_key(&context)), b"{").unwrap();
        assert!(
            cache
                .get(&context)
                .unwrap_err()
                .is::<crate::error::Decode>()
        );

        // Referenced missing blob.
        let missing = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(missing.path()).unwrap();
        let context = test_context("stage", "missing-blob");
        let receipt = persist_test_product(&cache, &context, &product);
        std::fs::remove_file(
            cache
                .dir
                .join("blobs")
                .join(receipt.product_blob_digest.unwrap()),
        )
        .unwrap();
        assert!(
            cache
                .get(&context)
                .unwrap_err()
                .is::<crate::error::CacheMismatch>()
        );

        // Receipt copied under a different action key.
        let wrong = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(wrong.path()).unwrap();
        let first = test_context("stage", "first-key");
        persist_test_product(&cache, &first, &product);
        let second = test_context("stage", "second-key");
        std::fs::copy(
            cache.receipt_path(&stage_key(&first)),
            cache.receipt_path(&stage_key(&second)),
        )
        .unwrap();
        assert!(
            cache
                .get(&second)
                .unwrap_err()
                .is::<crate::error::CacheMismatch>()
        );

        // Oversized receipt root: sparse growth proves the bound without allocating
        // the forged size. The reader rejects it before JSON allocation/parsing.
        let oversized_receipt = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(oversized_receipt.path()).unwrap();
        let context = test_context("stage", "oversized-receipt");
        persist_test_product(&cache, &context, &product);
        OpenOptions::new()
            .write(true)
            .open(cache.receipt_path(&stage_key(&context)))
            .unwrap()
            .set_len(MAX_RECEIPT_BYTES + 1)
            .unwrap();
        assert!(
            cache
                .get(&context)
                .unwrap_err()
                .is::<crate::error::Decode>()
        );

        // Oversized referenced blob: the same sparse-file attack is rejected before
        // hydration, even though the immutable receipt still names a small product.
        let oversized_blob = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(oversized_blob.path()).unwrap();
        let context = test_context("stage", "oversized-blob");
        let receipt = persist_test_product(&cache, &context, &product);
        let blob = cache
            .dir
            .join("blobs")
            .join(receipt.product_blob_digest.unwrap());
        OpenOptions::new()
            .write(true)
            .open(blob)
            .unwrap()
            .set_len(MAX_ENTRY_BYTES + 1)
            .unwrap();
        assert!(
            cache
                .get(&context)
                .unwrap_err()
                .is::<crate::error::Decode>()
        );
    }

    #[test]
    fn concurrent_publication_is_atomic_and_nondeterminism_fails() {
        use std::sync::Barrier;

        fn race(
            root: PathBuf,
            context: StageKeyContext,
            product: StageProduct,
            barrier: Arc<Barrier>,
        ) -> Result<StageReceipt, gmeow_errors::Diag> {
            let cache = PipelineCache::open(root).unwrap();
            barrier.wait();
            cache.put(
                &context,
                "stable",
                "persistent",
                &ReceiptOutputSelection::default(),
                &product,
            )
        }

        let dir = tempfile::tempdir().unwrap();
        let context = test_context("stage", "same-key");
        let barrier = Arc::new(Barrier::new(2));
        let left = {
            let root = dir.path().to_path_buf();
            let context = context.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                race(root, context, StageProduct::new("stage", "left"), barrier)
            })
        };
        let right = {
            let root = dir.path().to_path_buf();
            let context = context.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                race(root, context, StageProduct::new("stage", "right"), barrier)
            })
        };
        let outcomes = [left.join().unwrap(), right.join().unwrap()];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| {
                    result
                        .as_ref()
                        .is_err_and(|error| error.is::<crate::error::CacheMismatch>())
                })
                .count(),
            1,
            "same action key with different output is nondeterminism"
        );
        assert_eq!(PipelineCache::open(dir.path()).unwrap().len(), 1);

        // Different keys publish independently and neither receipt is lost.
        let dir = tempfile::tempdir().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["left", "right"]
            .into_iter()
            .map(|salt| {
                let root = dir.path().to_path_buf();
                let context = test_context("stage", salt);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    race(root, context, StageProduct::new("stage", salt), barrier)
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(PipelineCache::open(dir.path()).unwrap().len(), 2);
    }

    #[test]
    fn fixture_coordinator_elects_exactly_one_thread_builder() {
        use std::sync::Barrier;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let root = tempfile::tempdir().unwrap();
        let context = test_context("fixture-stage", "one-builder");
        let starts = Arc::new(Barrier::new(2));
        let builds = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let root = root.path().to_path_buf();
            let context = context.clone();
            let starts = Arc::clone(&starts);
            let builds = Arc::clone(&builds);
            workers.push(std::thread::spawn(move || {
                let coordinator = FixtureCoordinator::open(&root).unwrap();
                starts.wait();
                coordinator
                    .get_or_build(
                        &context,
                        "stable",
                        "persistent",
                        |_| Ok(ReceiptOutputSelection::default()),
                        || {
                            builds.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(25));
                            Ok(StageProduct::new("fixture-stage", "fixture-digest"))
                        },
                    )
                    .unwrap()
            }));
        }
        let outcomes: Vec<FixtureOutcome> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(outcomes.iter().filter(|outcome| outcome.built).count(), 1);
        assert_eq!(outcomes[0].receipt, outcomes[1].receipt);
        assert_eq!(outcomes[0].product.digest, outcomes[1].product.digest);
    }

    const FIXTURE_PROCESS_ROOT: &str = "GMEOW_FIXTURE_PROCESS_TEST_ROOT";

    #[test]
    fn fixture_coordinator_process_worker() {
        let Ok(root) = std::env::var(FIXTURE_PROCESS_ROOT) else {
            return;
        };
        let root = PathBuf::from(root);
        let context = test_context("fixture-process-stage", "one-process-builder");
        let coordinator = FixtureCoordinator::open(&root).unwrap();
        let outcome = coordinator
            .get_or_build(
                &context,
                "stable",
                "persistent",
                |_| Ok(ReceiptOutputSelection::default()),
                || {
                    let mut marker = OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(root.join("builder.marker"))?;
                    writeln!(marker, "{}", std::process::id())?;
                    marker.sync_all()?;
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    Ok(StageProduct::new(
                        "fixture-process-stage",
                        "fixture-process-digest",
                    ))
                },
            )
            .unwrap();
        println!("fixture-process-built={}", outcome.built);
    }

    #[test]
    fn fixture_coordinator_elects_exactly_one_builder_across_processes() {
        use std::process::{Command, Stdio};

        let root = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let spawn = || {
            Command::new(&executable)
                .arg("--exact")
                .arg("cache::tests::fixture_coordinator_process_worker")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env(FIXTURE_PROCESS_ROOT, root.path())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        };
        let left = spawn();
        let right = spawn();
        let outputs = [
            left.wait_with_output().unwrap(),
            right.wait_with_output().unwrap(),
        ];
        for output in &outputs {
            assert!(
                output.status.success(),
                "fixture worker failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        let stdout = outputs
            .iter()
            .map(|output| String::from_utf8_lossy(&output.stdout))
            .collect::<Vec<_>>();
        assert_eq!(
            stdout
                .iter()
                .filter(|text| text.contains("fixture-process-built=true"))
                .count(),
            1,
            "exactly one OS process must execute the fixture producer: {stdout:?}",
        );
        assert_eq!(
            stdout
                .iter()
                .filter(|text| text.contains("fixture-process-built=false"))
                .count(),
            1,
            "the losing OS process must hydrate the elected product: {stdout:?}",
        );
        assert!(root.path().join("builder.marker").is_file());
    }

    #[test]
    fn bounded_store_evicts_only_unreachable_entries_and_ignores_crash_temps() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path())
            .unwrap()
            .with_limits(1, 1024 * 1024);
        let first = test_context("stage", "first");
        let second = test_context("stage", "second");
        persist_test_product(&cache, &first, &StageProduct::new("stage", "first"));
        std::fs::write(
            cache.dir.join("receipts").join(".pipeline-cache-crash.tmp"),
            b"partial",
        )
        .unwrap();
        persist_test_product(&cache, &second, &StageProduct::new("stage", "second"));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&first).unwrap().is_none());
        assert!(cache.get(&second).unwrap().is_some());
        assert!(
            !cache
                .dir
                .join("receipts")
                .join(".pipeline-cache-crash.tmp")
                .exists()
        );
        assert_eq!(
            std::fs::read_dir(cache.dir.join("blobs")).unwrap().count(),
            1
        );

        let tiny = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(tiny.path()).unwrap().with_limits(1, 1);
        assert!(
            cache
                .put(
                    &test_context("stage", "too-large"),
                    "stable",
                    "persistent",
                    &ReceiptOutputSelection::default(),
                    &StageProduct::new("stage", "digest"),
                )
                .unwrap_err()
                .is::<crate::error::StageFailed>()
        );
        assert!(cache.is_empty());
    }

    #[test]
    fn namespace_gc_never_removes_a_live_reader() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("pipeline");
        let live = PipelineCache::open(base.join("a")).unwrap();
        drop(PipelineCache::open(base.join("b")).unwrap());
        let current = PipelineCache::open(base.join("c")).unwrap();

        PipelineCache::prune_namespaces(&base, "c", 1).unwrap();
        assert!(base.join("a").exists(), "live namespace is protected");
        assert!(!base.join("b").exists(), "idle old namespace is collected");
        assert!(base.join("c").exists(), "current namespace is retained");

        drop(live);
        PipelineCache::prune_namespaces(&base, "c", 1).unwrap();
        assert!(
            !base.join("a").exists(),
            "released namespace becomes collectible"
        );
        assert!(current.dir.exists());
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
        let cache = PipelineCache::open(dir.path()).unwrap();

        let original = reasoning_bundle();
        let product = StageProduct::from_bundle("stage-reason", Arc::new(original.clone()));
        let context = test_context("stage-reason", "reasoning-round-trip");
        persist_test_product(&cache, &context, &product);

        let got = cache.get(&context).unwrap().expect("cache hit");
        let recon = got.product.bundle();

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
        let cache = PipelineCache::open(dir.path()).unwrap();

        let (original, lowered) = relational_core_bundle();
        let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
        let context = test_context("stage-compile-logic", "relational-round-trip");
        persist_test_product(&cache, &context, &product);

        let got = cache.get(&context).unwrap().expect("cache hit");
        let recon = got.product.bundle();

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
        use gmeow_logic_compile::projections::correspondence::project_correspondence;
        use std::sync::Arc;
        let program = crate::stages::compile_logic::affine_worked_example_program();
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
        let cache = PipelineCache::open(dir.path()).unwrap();

        let (original, program) = correspondence_bundle();
        let product = StageProduct::from_bundle("stage-compile-logic", Arc::new(original.clone()));
        let context = test_context("stage-compile-logic", "correspondence-round-trip");
        persist_test_product(&cache, &context, &product);

        let got = cache.get(&context).unwrap().expect("cache hit");
        let recon = got.product.bundle();

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
