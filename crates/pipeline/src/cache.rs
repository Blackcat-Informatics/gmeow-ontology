// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The opt-in per-stage content-addressed cache (C4-cache).
//!
//! The cache key hashes a typed [`StageKeyContext`] containing build/toolchain
//! identity, stage/codec identity, producer-qualified whole/entity inputs, and raw
//! path/digest rows. The shared `.cache/gmeow-sync/actions/v<STORE_FORMAT_VERSION>/`
//! store (gitignored) holds immutable per-key receipts and content-addressed
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
//! * **provenance** — the S0.5-safe PUBLIC projection only (stable asserted-quad
//!   content key, unit names+kinds, artifact paths, locations). Runtime
//!   `UnitId`/`ArtifactId`/`OriginSetId` and unstable quad ordinals are NEVER
//!   persisted; on load each content key is resolved against the restored dataset,
//!   then units/artifacts/occurrences are re-registered so the reconstituted prov's
//!   `public_projection()` equals the persisted one (and thus the bundle digest is
//!   preserved). The sidecar is a runtime accumulator and only its public projection
//!   feeds the digest.
//! * **handles** — each `(graph_iri, HandleEntry)` persists its graph IRI, arm tag,
//!   full typed payload and semantic digest. Governed projection contracts are checked
//!   before publication. Hydration restores typed values without reparsing RDF and
//!   rechecks their payload, graph pin and complete product commitments.
//!
//! # GREENFIELD cache version
//!
//! [`CACHE_VERSION`] is folded into BOTH the action key and the manifest. A version
//! bump makes every prior pipeline action (including the C4-spine byte-only stand-in)
//! a clean MISS inside the shared store — there is no migration path (greenfield).
//!
//! # On-disk layout
//!
//! `.cache/gmeow-sync/actions/v<STORE_FORMAT_VERSION>/` holds
//! `receipts/<action-key>.json` roots and `blobs/<digest>` bincode products. The
//! binary encoding keeps the manifest's large `Vec<u8>` lanes
//! byte-dense instead of expanding every byte into a JSON number. On load the blob
//! is re-hashed and compared to the indexed digest — a mismatch is a HARD failure,
//! never a silent repair (no-optionality).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionKey, ActionStore, BlobRef, FileKind, ProducerIdentity,
    STORE_FORMAT_VERSION, StoreLimits,
};
use purrdf::provenance::{DatasetProvenance, OriginKind};
use purrdf::{
    ContentDigest, ContentStore, PackBuilder, QuadHandle, RdfBlobOrigin, RdfBlobRecord,
    RdfLocation, RdfLookaside, RdfLookasideKind, RdfLookasideResource, RdfMetadataValue,
    canonicalize, restore_pack,
};
use serde::{Deserialize, Serialize};

pub use gmeow_action_cache::content_digest;

use crate::bundle::PipelineHandle;
use crate::handle_identity::{handle_arm_tag, handle_payload_digest};
use crate::node::StageProduct;

/// The GREENFIELD on-disk cache-shape revision. Folded into BOTH the cache
/// subdirectory and the [`CachedBundle`] manifest so a stale cache (e.g. the C4-spine
/// byte-only stand-in, version-less or an older rev) is treated as a clean MISS, not
/// mis-decoded. Bump on ANY change to the persisted shape (no migration path).
pub const CACHE_VERSION: u32 = 16;

/// Schema revision for the canonical action-key rows and immutable stage receipt.
pub const RECEIPT_SCHEMA_VERSION: u32 = 2;

/// The structural codec identity. This is explicit in every action key rather than
/// relying only on [`CACHE_VERSION`], because a receipt must name the representation
/// it authenticates without knowing its storage path.
pub const CACHE_CODEC_IDENTITY: &str = "bincode-1+purrpack1+typed-ir3+typed-payload6+compiled-logic-cbor1+diagnostics-cbor1+receipt-json-2";

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
// The shared store also carries one independently reusable receipt per declarative
// slice spec. Keep several complete frontiers so opening the pipeline cache cannot
// evict the fine-grained test DAG that was just produced. The byte ceiling remains the
// authoritative storage bound; receipts themselves are compact.
const MAX_CACHE_ENTRIES: usize = 4_096;
const MAX_CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

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
/// Admitted effective code-generation recipe; empty in separate test/debug builds.
pub const PRODUCER_BUILD_CONTRACT: &str = env!("GMEOW_PRODUCER_BUILD_CONTRACT");
/// Resolved compilation policy, separate from the executable-wide source identity.
pub const PRODUCER_COMPILATION_CONTRACT: &str = env!("GMEOW_PRODUCER_COMPILATION_CONTRACT");
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

    pub(crate) fn action_context(&self) -> ActionContext {
        let mut implementation = ProducerIdentity::new(self.build.fingerprint.clone());
        implementation.toolchain = Some(self.build.toolchain.clone());
        implementation.target = Some(self.build.target.clone());
        implementation.profile = Some(self.build.profile.clone());
        implementation.features = self.build.features.clone();
        let mut inputs = self
            .upstream
            .iter()
            .map(|input| ActionInput::Upstream {
                producer: input.producer.clone(),
                entity: input.entity.clone(),
                receipt_digest: None,
                product_digest: input.digest.clone(),
            })
            .chain(self.raw_inputs.iter().map(|input| ActionInput::Raw {
                logical_path: input.path.clone(),
                file_kind: FileKind::File,
                executable: false,
                digest: input.digest.clone(),
            }))
            .collect::<Vec<_>>();
        inputs.sort();
        inputs.dedup();
        let mut context = ActionContext::new(
            "pipeline",
            self.stage_id.clone(),
            implementation,
            self.codec.clone(),
            inputs,
        )
        .with_dimension("impl-version", self.impl_version.clone())
        .with_dimension("pipeline-receipt-schema", self.schema_version.to_string())
        .with_dimension("pipeline-cache-version", CACHE_VERSION.to_string());
        for (name, value) in &self.dimensions {
            context.dimensions.insert(name.clone(), value.clone());
        }
        context
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
    context.action_context().key().to_string()
}

// ── The serde bundle manifest ────────────────────────────────────────────────

/// The serde-able mirror of a [`PipelineBundle<PipelineHandle>`] sufficient to
/// reconstruct a digest- and structure-equal bundle. The kernel bundle has no serde
/// (ring-fence), so this pipeline-side manifest captures every lane the bundle uses.
/// The same schema owns bytes when publishing and borrows them when reading.
/// Bincode encodes both byte representations identically; no second wire model
/// or cached carrier is introduced.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBundle<Bytes = Vec<u8>> {
    /// The on-disk cache-shape revision; a mismatch is a clean miss (greenfield).
    version: u32,
    /// The producing stage id.
    stage_id: String,
    /// The published product commitment, including complete typed bindings. Only
    /// abstract products with a completely empty carrier may use an external digest.
    digest: String,
    /// The complete deterministic `PURRPCK1` dataset image.
    dataset_pack: Bytes,
    /// The lookaside mirror: resources + blob records (the byte-artifact lane and
    /// later typed sidecar lanes ride here).
    lookaside: CachedLookaside,
    /// The content store: blob digest hex → payload bytes (rebuilt via
    /// `insert_checked`, so a corrupt blob hard-fails on load).
    blobs: BTreeMap<String, Bytes>,
    /// The S0.5 PUBLIC provenance projection rows `(unit, kind, artifact, location)`.
    /// NEVER the runtime numeric ids.
    provenance: Vec<CachedProvRow>,
    /// The typed-handle lane: each backing graph IRI + its arm tag. The backing
    /// graph itself already lives in `dataset_pack` and is never duplicated here.
    handles: Vec<CachedHandle<Bytes>>,
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

/// One public-projection provenance row keyed by the asserted quad's complete
/// length-delimited RDF content rather than a process-local numeric ordinal.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProvRow {
    quad_key: String,
    unit: String,
    kind: String,
    artifact: String,
    location: Option<String>,
}

/// A persisted typed handle: the backing graph IRI, [`PipelineHandle`] arm tag,
/// complete typed payload and its framed semantic commitment. The governed backing
/// graph remains a separately authenticated projection; hydration never tries to
/// recover omitted native fields from that projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHandle<Bytes = Vec<u8>> {
    /// The named-graph IRI the handle backs (the [`HandleKey`](purrdf::HandleKey)).
    graph: String,
    /// The [`PipelineHandle`] arm tag (see [`handle_arm_tag`]).
    arm: String,
    /// Digest of every typed field, with explicit structural framing.
    payload_digest: String,
    /// Full typed payload. Every admitted arm requires it; absence is corruption.
    typed_payload: Option<Bytes>,
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
    pub default_graph: Option<ReceiptEntity>,
    pub provenance: Option<ReceiptEntity>,
    pub content_store: Option<ReceiptEntity>,
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
    pub default_graph: Option<ReceiptEntity>,
    pub provenance: Option<ReceiptEntity>,
    pub content_store: Option<ReceiptEntity>,
    pub graphs: Vec<ReceiptEntity>,
    pub blob_representations: Vec<ReceiptEntity>,
    pub logical_artifacts: Vec<ReceiptEntity>,
    pub typed_handles: Vec<ReceiptEntity>,
}

impl StageReceipt {
    /// Digest of the canonical receipt payload (the envelope stores and verifies it).
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("StageReceipt JSON serialization");
        content_digest(&[b"gmeow:stage-receipt:v2", &bytes])
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
        for (graph, binding) in product.handle_commitments() {
            if !selected_handles.contains(graph.as_str()) {
                continue;
            }
            typed_handles.push(ReceiptEntity {
                identity: binding.identity.clone(),
                digest: binding.digest.clone(),
                structural_count: 1,
                decoded_bytes: u64::try_from(binding.digest.len()).unwrap_or(u64::MAX),
            });
        }
        typed_handles.sort_by(|left, right| left.identity.cmp(&right.identity));

        let default_graph = default_graph_commitment(product)?;
        let provenance = provenance_commitment(product)?;
        let content_store = content_store_commitment(product)?;

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
            default_graph,
            provenance,
            content_store,
            graphs,
            blob_representations,
            logical_artifacts,
            typed_handles,
        })
    }
}

fn lane_commitment(
    identity: &str,
    rows: impl IntoIterator<Item = String>,
) -> Option<ReceiptEntity> {
    let rows = rows.into_iter().collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }
    let mut canonical = Vec::new();
    let mut decoded_bytes = 0_u64;
    for row in &rows {
        let bytes = row.as_bytes();
        decoded_bytes =
            decoded_bytes.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        canonical.extend_from_slice(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        canonical.extend_from_slice(bytes);
    }
    Some(ReceiptEntity {
        identity: identity.to_string(),
        digest: gmeow_action_cache::bytes_digest(&canonical),
        structural_count: u64::try_from(rows.len()).unwrap_or(u64::MAX),
        decoded_bytes,
    })
}

fn append_field(out: &mut String, tag: &str, value: &str) {
    use std::fmt::Write as _;
    let _ = write!(out, "{tag}{}:{value};", value.len());
}

fn term_content_key(term: &purrdf::RdfTerm) -> String {
    let mut out = String::new();
    match term {
        purrdf::RdfTerm::Iri(iri) => append_field(&mut out, "i", iri),
        purrdf::RdfTerm::BlankNode(label) => append_field(&mut out, "b", label),
        purrdf::RdfTerm::Literal(literal) => {
            append_field(&mut out, "l", &literal.lexical_form);
            append_field(&mut out, "d", literal.datatype.as_deref().unwrap_or(""));
            append_field(&mut out, "g", literal.language.as_deref().unwrap_or(""));
            append_field(
                &mut out,
                "r",
                literal
                    .direction
                    .map_or("", purrdf::RdfTextDirection::as_str),
            );
        }
        purrdf::RdfTerm::Triple(triple) => {
            append_field(&mut out, "s", &term_content_key(&triple.subject));
            append_field(&mut out, "p", &triple.predicate);
            append_field(&mut out, "o", &term_content_key(&triple.object));
        }
    }
    out
}

fn quad_content_key(quad: &purrdf::RdfQuad) -> String {
    let mut out = String::new();
    append_field(&mut out, "s", &term_content_key(&quad.subject));
    append_field(&mut out, "p", &quad.predicate);
    append_field(&mut out, "o", &term_content_key(&quad.object));
    append_field(
        &mut out,
        "g",
        &quad
            .graph_name
            .as_ref()
            .map_or_else(String::new, term_content_key),
    );
    out
}

fn triple_content_key(triple: &purrdf::RdfTriple) -> String {
    let mut out = String::new();
    append_field(&mut out, "s", &term_content_key(&triple.subject));
    append_field(&mut out, "p", &triple.predicate);
    append_field(&mut out, "o", &term_content_key(&triple.object));
    out
}

fn reifier_content_key(reifier: &purrdf::RdfReifier) -> String {
    let mut out = String::new();
    append_field(&mut out, "r", &term_content_key(&reifier.reifier));
    append_field(&mut out, "t", &triple_content_key(&reifier.statement));
    append_field(
        &mut out,
        "g",
        &reifier
            .graph
            .as_ref()
            .map_or_else(String::new, term_content_key),
    );
    out
}

fn annotation_content_key(annotation: &purrdf::RdfAnnotation) -> String {
    let mut out = String::new();
    append_field(&mut out, "r", &term_content_key(&annotation.reifier));
    append_field(&mut out, "p", &annotation.predicate);
    append_field(&mut out, "o", &term_content_key(&annotation.object));
    append_field(
        &mut out,
        "g",
        &annotation
            .graph
            .as_ref()
            .map_or_else(String::new, term_content_key),
    );
    out
}

pub(crate) fn default_graph_keys(product: &StageProduct) -> BTreeSet<String> {
    let dataset = product.dataset();
    let mut rows = BTreeSet::new();
    rows.extend(
        dataset
            .owned_quads()
            .filter(|quad| quad.graph_name.is_none())
            .map(|quad| format!("quad:{}", quad_content_key(&quad))),
    );
    rows.extend(
        dataset
            .owned_reifiers()
            .filter(|reifier| reifier.graph.is_none())
            .map(|reifier| format!("reifier:{}", reifier_content_key(&reifier))),
    );
    rows.extend(
        dataset
            .owned_annotations()
            .filter(|annotation| annotation.graph.is_none())
            .map(|annotation| format!("annotation:{}", annotation_content_key(&annotation))),
    );
    rows
}

pub(crate) fn default_graph_commitment(
    product: &StageProduct,
) -> Result<Option<ReceiptEntity>, gmeow_errors::Diag> {
    Ok(lane_commitment(
        "default-graph",
        default_graph_keys(product),
    ))
}

pub(crate) fn provenance_keys(
    product: &StageProduct,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let quads = product.dataset().owned_quads().collect::<Vec<_>>();
    product
        .bundle()
        .provenance()
        .public_projection()
        .into_iter()
        .map(|(quad_index, unit, kind, artifact, location)| {
            let quad = quads.get(quad_index).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: format!(
                        "provenance row references absent quad index {quad_index} of {}",
                        quads.len()
                    ),
                })
            })?;
            serde_json::to_string(&(quad_content_key(quad), unit, kind, artifact, location))
                .map_err(|error| {
                    gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: format!("encode stable provenance commitment: {error}"),
                    })
                })
        })
        .collect()
}

pub(crate) fn provenance_commitment(
    product: &StageProduct,
) -> Result<Option<ReceiptEntity>, gmeow_errors::Diag> {
    Ok(lane_commitment("provenance", provenance_keys(product)?))
}

pub(crate) fn content_store_keys(
    product: &StageProduct,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let bundle = product.bundle();
    bundle.blobs().verify_all().map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: product.stage_id.clone(),
            message: format!("content store digest verification failed: {error}"),
        })
    })?;
    let actual = bundle
        .blobs()
        .iter()
        .map(|(digest, _)| digest.to_hex())
        .collect::<BTreeSet<_>>();
    let mut referenced = BTreeSet::new();
    for record in &bundle.lookaside().blobs {
        let digest = ContentDigest::from_hex(&record.digest).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: product.stage_id.clone(),
                message: format!("blob record carries malformed digest {:?}", record.digest),
            })
        })?;
        let bytes = bundle.blobs().get(&digest).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: product.stage_id.clone(),
                message: format!("blob record references missing content {}", record.digest),
            })
        })?;
        if record
            .decoded_len
            .is_some_and(|declared| declared != bytes.len())
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: product.stage_id.clone(),
                message: format!(
                    "blob record {} decoded_len {:?} != stored bytes {}",
                    record.digest,
                    record.decoded_len,
                    bytes.len()
                ),
            }));
        }
        referenced.insert(record.digest.clone());
    }
    for resource in &bundle.lookaside().resources {
        if let Some(digest_hex) = &resource.content_digest {
            let digest = ContentDigest::from_hex(digest_hex).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: format!("resource carries malformed content digest {digest_hex:?}"),
                })
            })?;
            if bundle.blobs().get(&digest).is_none() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: product.stage_id.clone(),
                    message: format!("resource references missing content {digest_hex}"),
                }));
            }
            referenced.insert(digest_hex.clone());
        }
    }
    if actual != referenced {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: product.stage_id.clone(),
            message: format!(
                "persistent content store is not closed over lookaside references: orphan={:?}, missing={:?}",
                actual.difference(&referenced).collect::<Vec<_>>(),
                referenced.difference(&actual).collect::<Vec<_>>()
            ),
        }));
    }
    Ok(actual)
}

pub(crate) fn content_store_commitment(
    product: &StageProduct,
) -> Result<Option<ReceiptEntity>, gmeow_errors::Diag> {
    let keys = content_store_keys(product)?;
    let rows = keys
        .into_iter()
        .map(|digest_hex| {
            let digest = ContentDigest::from_hex(&digest_hex).expect("validated digest");
            let len = product
                .bundle()
                .blobs()
                .get(&digest)
                .map(Vec::len)
                .expect("validated content-store key");
            format!("{digest_hex}:{len}")
        })
        .collect::<Vec<_>>();
    Ok(lane_commitment("content-store", rows))
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

/// Validate the existing governed projection contract once before publication.
/// Cache hydration then restores complete typed fields without another RDF parse.
fn validate_handle_projection(
    handle: &PipelineHandle,
    graph: &purrdf::RdfDataset,
) -> Result<(), gmeow_errors::Diag> {
    let error = |detail: String| {
        gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!("cache: typed handle projection binding: {detail}"),
        })
    };
    let agrees = match handle {
        PipelineHandle::SourceCatalog(_) => {
            return Err(error(
                "ephemeral source catalogs cannot enter the persistent cache".into(),
            ));
        }
        // The Logic projection deliberately omits source-verbatim collections and
        // report inputs; each complete typed codec is the authority for those fields.
        // Payload and whole-product commitments independently authenticate them.
        PipelineHandle::Logic(_) | PipelineHandle::CompiledLogic(_) => true,
        // This is a selected rich report publication over the shared finding
        // graph, which also carries gate/meta conclusions and omits report-only
        // fields. Its producer pairing, graph pin and complete native commitment
        // govern it; reconstruction from that projection cannot recover a Report.
        PipelineHandle::Diagnostics(publication) => {
            publication.validate()?;
            true
        }
        PipelineHandle::Reasoning(result) => {
            let expected = gmeow_logic::result_rdf::project_reasoning_dataset(result)
                .map_err(|e| error(e.to_string()))?;
            purrdf::datasets_isomorphic(graph, &expected)
        }
        PipelineHandle::RelationalCore(program) => {
            let projected = gmeow_logic_compile::relational_core::parse_relational_core(graph)
                .map_err(|e| error(e.to_string()))?;
            program.projection_key().map_err(|e| error(e.to_string()))?
                == projected
                    .projection_key()
                    .map_err(|e| error(e.to_string()))?
        }
        PipelineHandle::Correspondence(program) => {
            let projected =
                gmeow_logic_compile::projections::correspondence::parse_correspondence(graph)
                    .map_err(|e| error(e.to_string()))?;
            crate::handle_identity::typed_digest(program.as_ref())
                == crate::handle_identity::typed_digest(&projected)
        }
    };
    if !agrees {
        return Err(error(format!(
            "{} payload does not agree with its governed backing graph",
            handle_arm_tag(handle)
        )));
    }
    Ok(())
}

/// Encode a complete typed publication only at the persistent action boundary.
/// Report-bearing arms use CBOR for diagnostic nodes, optional map fields and
/// arbitrary metadata; program-only arms retain their established binary codecs.
fn encode_handle(handle: &PipelineHandle) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let encoded = match handle {
        PipelineHandle::SourceCatalog(_) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: "ephemeral source catalogs have no persistent codec".into(),
            }));
        }
        PipelineHandle::CompiledLogic(publication) => {
            let mut bytes = Vec::new();
            ciborium::ser::into_writer(publication.as_ref(), &mut bytes).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: encode complete compiled-logic publication: {error}"),
                })
            })?;
            return Ok(bytes);
        }
        PipelineHandle::Diagnostics(publication) => {
            publication.validate()?;
            let mut bytes = Vec::new();
            ciborium::ser::into_writer(publication.as_ref(), &mut bytes).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: encode complete diagnostics publication: {error}"),
                })
            })?;
            return Ok(bytes);
        }
        PipelineHandle::Logic(program) => bincode::serialize(program.as_ref()),
        PipelineHandle::Reasoning(result) => bincode::serialize(result.as_ref()),
        PipelineHandle::RelationalCore(program) => bincode::serialize(program.as_ref()),
        PipelineHandle::Correspondence(program) => bincode::serialize(program.as_ref()),
    };
    encoded.map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!(
                "cache: encode complete {} handle IR: {error}",
                handle_arm_tag(handle)
            ),
        })
    })
}

/// Restore a complete native payload without reparsing a governed RDF projection.
fn rebuild_handle(
    arm: &str,
    typed_payload: Option<&[u8]>,
) -> Result<PipelineHandle, gmeow_errors::Diag> {
    let bytes = typed_payload.ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!("cache: {arm} handle is missing its complete typed IR payload"),
        })
    })?;
    let decode_error = |error| {
        gmeow_errors::Diag::of_kind(crate::error::Decode {
            message: format!("cache: decode complete {arm} handle IR: {error}"),
        })
    };
    Ok(match arm {
        "logic" => {
            PipelineHandle::Logic(Arc::new(bincode::deserialize(bytes).map_err(decode_error)?))
        }
        "compiled-logic" => {
            // Diagnostic nodes use map-shaped Serde fields, including omitted empty
            // fields. CBOR preserves that complete schema; bincode cannot decode it.
            let mut reader = std::io::Cursor::new(bytes);
            let publication = ciborium::de::from_reader(&mut reader).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: decode complete compiled-logic publication: {error}"),
                })
            })?;
            if reader.position() != bytes.len() as u64 {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: "cache: trailing bytes after compiled-logic publication".into(),
                }));
            }
            PipelineHandle::CompiledLogic(Arc::new(publication))
        }
        "reasoning" => {
            PipelineHandle::Reasoning(Arc::new(bincode::deserialize(bytes).map_err(decode_error)?))
        }
        "diagnostics" => {
            let mut reader = std::io::Cursor::new(bytes);
            let publication: crate::bundle::DiagnosticsPublication =
                ciborium::de::from_reader(&mut reader).map_err(|error| {
                    gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: format!("cache: decode complete diagnostics publication: {error}"),
                    })
                })?;
            if reader.position() != bytes.len() as u64 {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: "cache: trailing bytes after diagnostics publication".into(),
                }));
            }
            publication.validate()?;
            PipelineHandle::Diagnostics(Arc::new(publication))
        }
        "relational-core" => PipelineHandle::RelationalCore(Arc::new(
            bincode::deserialize(bytes).map_err(decode_error)?,
        )),
        "correspondence" => PipelineHandle::Correspondence(Arc::new(
            bincode::deserialize(bytes).map_err(decode_error)?,
        )),
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
    /// Prove that `product` is exactly the independently reusable output delta the
    /// scheduler authenticated. A persistent stage may not carry an upstream graph,
    /// blob, artifact, or handle merely because its implementation retained a
    /// cumulative carrier: any such lane is absent from `selection` and hard-fails.
    ///
    /// Every lane is explicit, including the default graph, stable provenance rows,
    /// and the exact closed ContentStore. No cumulative carrier can hide behind an
    /// aggregate product digest.
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

        fn exact_commitment(
            stage: &str,
            lane: &str,
            actual: Option<ReceiptEntity>,
            selected: &Option<ReceiptEntity>,
        ) -> Result<(), gmeow_errors::Diag> {
            if actual == *selected {
                return Ok(());
            }
            Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage.to_string(),
                message: format!(
                    "persistent cache unit commitment mismatch on {lane}: product={actual:?}, \
                     scheduler-selection={selected:?}; cumulative carriers must use \
                     cacheRecomputeAggregate"
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

        exact_commitment(
            &product.stage_id,
            "default graph",
            default_graph_commitment(product)?,
            &selection.default_graph,
        )?;
        exact_commitment(
            &product.stage_id,
            "provenance",
            provenance_commitment(product)?,
            &selection.provenance,
        )?;
        exact_commitment(
            &product.stage_id,
            "content store",
            content_store_commitment(product)?,
            &selection.content_store,
        )?;

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
        let quads = bundle.dataset().owned_quads().collect::<Vec<_>>();
        let provenance = bundle
            .provenance()
            .public_projection()
            .into_iter()
            .map(|(quad_index, unit, kind, artifact, location)| {
                let quad = quads.get(quad_index).ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        stage: product.stage_id.clone(),
                        message: format!(
                            "provenance row references absent quad index {quad_index} of {}",
                            quads.len()
                        ),
                    })
                })?;
                Ok(CachedProvRow {
                    quad_key: quad_content_key(quad),
                    unit,
                    kind,
                    artifact,
                    location,
                })
            })
            .collect::<Result<Vec<_>, gmeow_errors::Diag>>()?;

        // handles → graph IRI + arm tag, sorted by graph IRI (BTreeMap iteration is
        // already sorted). The graph data itself is already present once in the
        // packed dataset and must not be duplicated in the manifest.
        let mut handles = Vec::with_capacity(bundle.handles().len());
        for (graph, entry) in bundle.handles() {
            if let PipelineHandle::Diagnostics(publication) = &entry.payload {
                publication.validate_binding(&product.stage_id, graph)?;
                if !crate::handle_identity::contains_graph(bundle, graph) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: "cache: native diagnostics require their declared graph".into(),
                    }));
                }
            }
            let backing = bundle.dataset().project_named_graph(graph);
            validate_handle_projection(&entry.payload, &backing)?;
            let arm = handle_arm_tag(&entry.payload);
            let payload_digest = handle_payload_digest(&entry.payload);
            let encoded = encode_handle(&entry.payload)?;
            let typed_payload = Some(encoded);
            let rebuilt = rebuild_handle(arm, typed_payload.as_deref())?;
            let rebuilt_digest = handle_payload_digest(&rebuilt);
            if rebuilt_digest != payload_digest {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!(
                        "cache: stage {} typed handle <{graph}> ({arm}) is not losslessly \
                         reconstructible from its native codec: live payload {payload_digest}, \
                         rebuilt payload {rebuilt_digest}; typed codec reconstruction changed the value",
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
}

impl<Bytes: AsRef<[u8]> + Into<Vec<u8>>> CachedBundle<Bytes> {
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
        let dataset = restore_pack(self.dataset_pack.as_ref()).map_err(|e| {
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
            store.insert_checked(digest, bytes.into()).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: hex.clone(),
                    actual: format!("{e}"),
                })
            })?;
        }

        // Provenance: resolve every stable asserted-quad content key against the
        // restored dataset, then re-register units/artifacts/occurrences. Numeric
        // ordinals are deliberately derived here rather than persisted.
        let mut quad_indices = BTreeMap::new();
        for (index, quad) in dataset.owned_quads().enumerate() {
            let key = quad_content_key(&quad);
            if quad_indices.insert(key.clone(), index).is_some() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: duplicate stable quad content key {key:?}"),
                }));
            }
        }
        let mut provenance = DatasetProvenance::new();
        for row in &self.provenance {
            let kind = origin_kind_from_str(&row.kind)?;
            let unit = provenance.register_unit(row.unit.clone(), kind);
            let artifact = provenance.register_artifact(row.artifact.clone());
            let quad_index = *quad_indices.get(&row.quad_key).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("restored asserted quad {}", row.quad_key),
                    actual: "no matching quad in restored dataset".to_string(),
                })
            })?;
            let quad_ordinal = u32::try_from(quad_index).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("cache: provenance quad ordinal {quad_index} exceeds u32"),
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
            let payload = rebuild_handle(&h.arm, h.typed_payload.as_ref().map(AsRef::as_ref))?;
            if let PipelineHandle::Diagnostics(publication) = &payload {
                publication.validate_binding(&self.stage_id, &h.graph)?;
                if !crate::handle_identity::contains_graph(&bundle, &h.graph) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                        message: "cache: restored diagnostics have no declared graph".into(),
                    }));
                }
            }
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
        if product.digest != self.digest {
            // Abstract stages intentionally carry an external digest over an empty
            // carrier. This exception cannot admit any graph, payload or sidecar.
            let empty =
                crate::bundle::bundle_from_artifacts(BTreeMap::new(), DatasetProvenance::new());
            if !product.bundle().handles().is_empty() || product.bundle().digest() != empty.digest()
            {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: self.digest,
                    actual: product.digest,
                }));
            }
            product.digest = self.digest;
        }
        Ok(product)
    }
}

impl<Bytes: AsRef<[u8]>> CachedBundle<Bytes> {
    /// Authenticate every committed artifact without restoring the packed dataset
    /// or rebuilding typed handles. Returned payloads borrow the authenticated
    /// input; consumers materialize only the requested output bytes.
    fn verified_artifacts(
        &self,
        receipt: &StageReceipt,
    ) -> Result<BTreeMap<String, &[u8]>, gmeow_errors::Diag> {
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
            let bytes = bytes.as_ref();
            let actual_digest = ContentDigest::of(bytes).to_hex();
            let actual_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if actual_digest != entity.digest || actual_bytes != entity.decoded_bytes {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: format!("{}:{}", entity.digest, entity.decoded_bytes),
                    actual: format!("{actual_digest}:{actual_bytes}"),
                }));
            }
            artifacts.insert(entity.identity.clone(), bytes);
        }
        Ok(artifacts)
    }
}

/// The pipeline bundle alias the cache reconstitutes (`PipelineBundle<PipelineHandle>`).
type PipelineBundleAlias = purrdf::PipelineBundle<PipelineHandle>;

// ── On-disk content-addressed cache ──────────────────────────────────────────

/// The persistent per-stage domain in the shared
/// `.cache/gmeow-sync/actions/v<version>/` store (gitignored and worktree-local).
///
/// `receipts/<action-key>.json` is the immutable root for one action and
/// `blobs/<content-digest>` holds the bincode-serialized [`CachedBundle`]. There is
/// no mutable global index: writers of different keys cannot erase one another and
/// writers of the same key must agree byte-for-byte.
pub struct PipelineCache {
    store: ActionStore,
    max_bytes: u64,
}

impl PipelineCache {
    /// Construct an inert cache handle without touching the filesystem.
    ///
    /// [`crate::scheduler::RunContext::open_uncached`] uses this for explicit
    /// diagnostic/test runs: scheduler cache probes and writes are disabled, so the
    /// path is never read.
    pub fn inert() -> Self {
        Self {
            store: ActionStore::inert(),
            max_bytes: 0,
        }
    }

    /// The conventional cache base directory under a repo root. [`open`](Self::open)
    /// appends the version segment, so this is the un-segmented base.
    pub fn default_dir(root: &Path) -> PathBuf {
        ActionStore::default_root(root)
    }

    /// Open the one worktree-local action-store namespace shared by scheduler
    /// stages and cross-process fixtures.
    ///
    /// Keeping the composition of [`default_dir`](Self::default_dir) and
    /// [`open`](Self::open) here prevents a fixture reader from accidentally adding
    /// a second namespace component that no producer writes. Executable identity is
    /// already part of every action key; it must not also partition the store path.
    pub fn open_default(root: &Path) -> Result<Self, gmeow_errors::Diag> {
        Self::open(Self::default_dir(root))
    }

    /// Open the existing worktree-local cache as a strict read-only consumer.
    /// Missing cache structure is an error and no lock, directory, sentinel, receipt,
    /// blob, or quota-pruning mutation is permitted.
    pub fn open_existing_default_read_only(root: &Path) -> Result<Self, gmeow_errors::Diag> {
        Self::open_existing_read_only(Self::default_dir(root))
    }

    /// Open (or create) the cache rooted at `dir`. The on-disk
    /// store lives under a `v<STORE_FORMAT_VERSION>` leaf of `dir`. Pipeline product
    /// shape is an action-key dimension, so a [`CACHE_VERSION`] bump makes every older
    /// pipeline action a clean miss without forcing unrelated action domains to move.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, gmeow_errors::Diag> {
        let limits = StoreLimits {
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_receipt_bytes: MAX_RECEIPT_BYTES,
            max_entries: MAX_CACHE_ENTRIES,
            max_total_bytes: MAX_CACHE_BYTES,
        };
        let store =
            ActionStore::open(dir, STORE_FORMAT_VERSION, limits).map_err(action_cache_diag)?;
        Ok(Self {
            store,
            max_bytes: MAX_CACHE_BYTES,
        })
    }

    /// Open an existing cache without admitting or mutating its storage namespace.
    pub fn open_existing_read_only(dir: impl Into<PathBuf>) -> Result<Self, gmeow_errors::Diag> {
        let limits = StoreLimits {
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_receipt_bytes: MAX_RECEIPT_BYTES,
            max_entries: MAX_CACHE_ENTRIES,
            max_total_bytes: MAX_CACHE_BYTES,
        };
        let store = ActionStore::open_existing_read_only(dir, STORE_FORMAT_VERSION, limits)
            .map_err(action_cache_diag)?;
        Ok(Self {
            store,
            max_bytes: MAX_CACHE_BYTES,
        })
    }

    /// Look up a stage product by cache key. Returns `None` on a miss. HARD-fails
    /// (`CacheMismatch`) if the blob exists but its re-hashed digest disagrees
    /// with the index — the cache is never silently repaired.
    pub fn get(&self, context: &StageKeyContext) -> Result<Option<CacheHit>, gmeow_errors::Diag> {
        let Some(entry) = self
            .store
            .get::<StageReceipt>(&context.action_context())
            .map_err(action_cache_diag)?
        else {
            return Ok(None);
        };
        let common = entry.receipt;
        let bytes = entry.bytes;
        validate_common_receipt(
            context,
            &common.action_key,
            &common.product_digest,
            &common.product_blob,
            &common.payload,
        )?;
        let hydrated_bytes = common.product_blob.bytes;
        let receipt = common.payload;
        let cached: CachedBundle<&[u8]> = bincode::deserialize(&bytes).map_err(|e| {
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
        let Some(common) = self
            .store
            .inspect::<StageReceipt>(&context.action_context())
            .map_err(action_cache_diag)?
        else {
            return Ok(None);
        };
        validate_common_receipt(
            context,
            &common.action_key,
            &common.product_digest,
            &common.product_blob,
            &common.payload,
        )?;
        Ok(Some(common.payload))
    }

    /// Load only the committed logical-artifact lane from a verified product blob.
    ///
    /// Packed RDF, blob and typed-handle payloads borrow the authenticated input
    /// buffer while the entire artifact lane is verified. No dataset indexes or
    /// typed handles are constructed. Every artifact's path, digest, byte count
    /// and structural count is checked before output bytes are copied.
    pub fn get_artifacts(
        &self,
        context: &StageKeyContext,
    ) -> Result<Option<ArtifactCacheHit>, gmeow_errors::Diag> {
        self.read_artifact_lane(context, None)
    }

    /// Return exactly one named artifact in the hit's artifact map. All receipt
    /// and artifact commitments are still verified; unselected payloads remain
    /// borrowed. An absent action is a miss, but an absent requested artifact in
    /// a present action is a hard failure.
    pub fn get_artifact(
        &self,
        context: &StageKeyContext,
        artifact_path: &str,
    ) -> Result<Option<ArtifactCacheHit>, gmeow_errors::Diag> {
        self.get_selected_artifacts(context, &[artifact_path])
    }

    /// Return only the named artifacts after authenticating the complete product
    /// and every artifact commitment once. Missing requested artifacts hard-fail;
    /// duplicate selections identify the same output and do not copy it twice.
    pub fn get_selected_artifacts(
        &self,
        context: &StageKeyContext,
        artifact_paths: &[&str],
    ) -> Result<Option<ArtifactCacheHit>, gmeow_errors::Diag> {
        self.read_artifact_lane(context, Some(artifact_paths))
    }

    fn read_artifact_lane(
        &self,
        context: &StageKeyContext,
        selected_paths: Option<&[&str]>,
    ) -> Result<Option<ArtifactCacheHit>, gmeow_errors::Diag> {
        let Some(entry) = self
            .store
            .get::<StageReceipt>(&context.action_context())
            .map_err(action_cache_diag)?
        else {
            return Ok(None);
        };
        let common = entry.receipt;
        validate_common_receipt(
            context,
            &common.action_key,
            &common.product_digest,
            &common.product_blob,
            &common.payload,
        )?;
        let transferred_bytes = common.product_blob.bytes;
        let receipt = common.payload;
        let bytes = entry.bytes;
        let cached: CachedBundle<&[u8]> = bincode::deserialize(&bytes).map_err(|error| {
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
        let verified = cached.verified_artifacts(&receipt)?;
        let artifacts = if let Some(paths) = selected_paths {
            paths
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|path| {
                    verified
                        .get(path)
                        .map(|bytes| (path.to_owned(), bytes.to_vec()))
                        .ok_or_else(|| {
                            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                                stage: context.stage_id.clone(),
                                message: format!(
                                    "authenticated stage product carries no artifact {path}"
                                ),
                            })
                        })
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?
        } else {
            verified
                .into_iter()
                .map(|(path, bytes)| (path, bytes.to_vec()))
                .collect()
        };
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
        let digest_hex = gmeow_action_cache::bytes_digest(&bytes);
        let receipt = StageReceipt::from_product(
            context.clone(),
            stability,
            cache_disposition,
            selection,
            product,
            Some(digest_hex.clone()),
            serialized_bytes,
        )?;
        let common = self
            .store
            .publish(
                &context.action_context(),
                product.digest.clone(),
                receipt.clone(),
                &bytes,
            )
            .map_err(action_cache_diag)?;
        validate_common_receipt(
            context,
            &common.action_key,
            &common.product_digest,
            &common.product_blob,
            &common.payload,
        )?;
        Ok(common.payload)
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
        self.store.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn action_cache_diag(error: gmeow_action_cache::ActionCacheError) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
        expected: "a verified immutable action-cache entry".to_string(),
        actual: error.to_string(),
    })
}

fn validate_common_receipt(
    context: &StageKeyContext,
    action_key: &ActionKey,
    common_product_digest: &str,
    common_blob: &BlobRef,
    receipt: &StageReceipt,
) -> Result<(), gmeow_errors::Diag> {
    let expected_key = stage_key(context);
    let stage_blob_digest = receipt
        .product_blob_digest
        .as_deref()
        .unwrap_or("<missing>");
    if action_key.as_str() != expected_key
        || receipt.action_key != expected_key
        || receipt.context != *context
        || common_product_digest != receipt.product_digest
        || common_blob.digest != stage_blob_digest
        || common_blob.bytes != receipt.product_blob_bytes
    {
        return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
            expected: format!(
                "key={expected_key};product={};blob={stage_blob_digest}:{}",
                receipt.product_digest, receipt.product_blob_bytes
            ),
            actual: format!(
                "key={action_key};product={common_product_digest};blob={}:{}",
                common_blob.digest, common_blob.bytes
            ),
        }));
    }
    Ok(())
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
}

enum FixtureCoordinateError {
    Cache(gmeow_action_cache::ActionCacheError),
    Pipeline(gmeow_errors::Diag),
}

impl From<gmeow_action_cache::ActionCacheError> for FixtureCoordinateError {
    fn from(error: gmeow_action_cache::ActionCacheError) -> Self {
        Self::Cache(error)
    }
}

impl FixtureCoordinator {
    /// Open the worktree-local fixture namespace for the current executable identity.
    pub fn open(root: &Path) -> Result<Self, gmeow_errors::Diag> {
        // Fixtures and the production scheduler intentionally share ONE immutable
        // receipt/blob authority. Priming a fixture therefore warms the exact stage
        // action a later full DAG run probes; there is no shadow fixture producer or
        // duplicate cache namespace.
        let cache = PipelineCache::open_default(root)?;
        Ok(Self { cache })
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
        let action_context = context.action_context();
        let key = action_context.key();
        let coordinated = self
            .cache
            .store
            .coordinate::<_, FixtureCoordinateError, _, _>(
                &key,
                || {
                    self.fixture_hit(context, stability, cache_disposition, &select)
                        .map_err(FixtureCoordinateError::Pipeline)
                },
                || {
                    let product = build().map_err(FixtureCoordinateError::Pipeline)?;
                    let selection = select(&product).map_err(FixtureCoordinateError::Pipeline)?;
                    let receipt = self
                        .cache
                        .put(context, stability, cache_disposition, &selection, &product)
                        .map_err(FixtureCoordinateError::Pipeline)?;
                    Ok(FixtureOutcome {
                        transferred_bytes: receipt.product_blob_bytes,
                        product,
                        receipt,
                        built: true,
                    })
                },
            )
            .map_err(|error| match error {
                FixtureCoordinateError::Cache(error) => action_cache_diag(error),
                FixtureCoordinateError::Pipeline(error) => error,
            })?;
        let mut outcome = coordinated.value;
        outcome.built = coordinated.built;
        Ok(outcome)
    }

    fn fixture_hit<S>(
        &self,
        context: &StageKeyContext,
        stability: &str,
        cache_disposition: &str,
        select: &S,
    ) -> Result<Option<FixtureOutcome>, gmeow_errors::Diag>
    where
        S: Fn(&StageProduct) -> Result<ReceiptOutputSelection, gmeow_errors::Diag>,
    {
        let Some(hit) = self.cache.get(context)? else {
            return Ok(None);
        };
        let selection = select(&hit.product)?;
        PipelineCache::validate_hit_receipt(
            context,
            stability,
            cache_disposition,
            &selection,
            &hit,
        )?;
        Ok(Some(FixtureOutcome {
            product: hit.product,
            receipt: hit.receipt,
            built: false,
            transferred_bytes: hit.hydrated_bytes,
        }))
    }
}

#[cfg(test)]
#[path = "cache_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "cache_loss_codec_tests.rs"]
mod loss_codec_tests;

#[cfg(test)]
#[path = "cache_diagnostics_tests.rs"]
mod diagnostics_tests;

#[path = "cache.tests.rs"]
#[cfg(test)]
mod tests;
