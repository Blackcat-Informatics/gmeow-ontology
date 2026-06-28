// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `PipelineBundle<H>` — the carrier that travels through the build pipeline:
//! the frozen hot graph, its out-of-band lookaside, the content-addressed blob
//! store, the provenance sidecar, and a typed-handle lane.
//!
//! ## Kernel boundary (#885)
//!
//! The kernel owns the bundle SHAPE but NOT the concrete handle payloads. The
//! payload type `H` is generic so that pipeline-side types (logic programs,
//! rendered docs, reasoning results) never enter `gmeow-rdf-core` — the
//! oxigraph-free / PyO3-free ring-fence stays intact. A handle bundles its payload
//! with a PINNED [`ContentDigest`] of the named graph it projects.
//!
//! ## Content addressing
//!
//! [`PipelineBundle::digest`] is a SHA-256 fold over, in a fixed order:
//! 1. the canonical N-Quads hash of the dataset ([`canonicalize`]),
//! 2. each lookaside resource's `content_digest` (collected and SORTED),
//! 3. each blob's [`ContentDigest`] in the store (SORTED),
//! 4. the provenance's runtime-id-free PUBLIC projection
//!    ([`DatasetProvenance::public_projection`], S0.5).
//!
//! The typed-handle lane contributes NOTHING to the digest: attaching or detaching
//! a handle leaves [`digest`](PipelineBundle::digest) byte-stable. This is the
//! contract a downstream cache keys on — the dataset/lookaside/blobs/public-
//! provenance are the content, the handles are derived views over it.
//!
//! ## Pin invariant (hard-fail)
//!
//! Attaching a handle ALWAYS checks that its pinned digest equals the canonical
//! digest of its backing named graph; a mismatch is a HARD failure
//! ([`PipelineBundleError::HandleDigestMismatch`]). The check runs on every attach,
//! not only in tests, so a bundle can never carry a handle that disagrees with the
//! graph it claims to project. Concrete pipeline-side handle types plug into this
//! lane unchanged.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::canon::canonicalize;
use super::dataset::RdfDataset;
use crate::provenance::DatasetProvenance;
use crate::{ContentDigest, ContentStore, RdfLookaside, RdfTerm};

/// Field separator inside the digest fold (mirrors `StageProduct::from_artifacts`).
const SEP_FIELD: u8 = 0x1f;
/// Record separator inside the digest fold.
const SEP_RECORD: u8 = 0x1e;
/// Section separator between the four digest contributions.
const SEP_SECTION: u8 = 0x1d;

/// The key identifying the named graph a typed handle backs. An IRI string is the
/// stable, dataset-independent name of the graph the handle projects.
pub type HandleKey = String;

/// A typed handle: a pipeline-side payload `H` paired with the PINNED
/// [`ContentDigest`] of the named graph it projects.
///
/// The digest is checked against the backing graph on every attach
/// ([`PipelineBundle::pin_handle`]); a `HandleEntry` in a constructed bundle is
/// therefore always in agreement with the graph at the time it was attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleEntry<H> {
    /// The pipeline-side typed payload (kernel-opaque).
    pub payload: H,
    /// The canonical digest of the backing named graph this handle projects,
    /// pinned at attach time.
    pub content_digest: ContentDigest,
}

impl<H> HandleEntry<H> {
    /// Pair a payload with the digest of the graph it projects.
    pub fn new(payload: H, content_digest: ContentDigest) -> Self {
        Self {
            payload,
            content_digest,
        }
    }
}

/// An error from attaching a typed handle to a [`PipelineBundle`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PipelineBundleError {
    /// A handle's pinned digest does not equal the canonical digest of the named
    /// graph it claims to back. Always a hard failure — the bundle never carries a
    /// handle that disagrees with its graph.
    HandleDigestMismatch {
        /// The graph IRI the handle keys on.
        graph: HandleKey,
        /// The digest the handle pinned.
        pinned: ContentDigest,
        /// The canonical digest the backing graph actually hashes to.
        actual: ContentDigest,
    },
}

impl fmt::Display for PipelineBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandleDigestMismatch {
                graph,
                pinned,
                actual,
            } => write!(
                f,
                "handle for graph <{graph}> pins digest {pinned} but the backing graph \
                 canonicalizes to {actual}"
            ),
        }
    }
}

impl std::error::Error for PipelineBundleError {}

/// The pipeline carrier: the frozen hot graph plus its out-of-band material and a
/// typed-handle lane.
///
/// `#[non_exhaustive]` so later tasks grow the carrier additively; construct it
/// through [`PipelineBundle::new`] and the builder methods. Generic over the
/// typed-handle payload `H` — see the module docs for the kernel-boundary rationale.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PipelineBundle<H> {
    /// The immutable, value-interned RDF 1.2 dataset — the hot graph.
    pub dataset: Arc<RdfDataset>,
    /// Structured non-triple companion material (typed sidecar resources, blobs by
    /// reference, segments, metadata, …).
    pub lookaside: RdfLookaside,
    /// The single owner of blob payload bytes (by-reference doctrine).
    pub blobs: Arc<ContentStore>,
    /// The provenance sidecar (units / artifacts / origin-sets / occurrences).
    pub provenance: DatasetProvenance,
    /// The typed-handle lane: backing-graph IRI → typed payload + pinned digest.
    /// EXCLUDED from [`digest`](Self::digest).
    pub handles: BTreeMap<HandleKey, HandleEntry<H>>,
}

impl<H> PipelineBundle<H> {
    /// Assemble a pipeline bundle from its parts, with an empty handle lane.
    ///
    /// Mirrors [`GtsBundle::new`](super::bundle::GtsBundle::new): the dataset is the
    /// frozen hot graph and the remaining parts are the out-of-band material that
    /// travels with it. Attach typed handles afterwards via
    /// [`pin_handle`](Self::pin_handle).
    pub fn new(
        dataset: Arc<RdfDataset>,
        lookaside: RdfLookaside,
        blobs: Arc<ContentStore>,
        provenance: DatasetProvenance,
    ) -> Self {
        Self {
            dataset,
            lookaside,
            blobs,
            provenance,
            handles: BTreeMap::new(),
        }
    }

    /// Borrow the frozen hot graph.
    pub fn dataset(&self) -> &RdfDataset {
        &self.dataset
    }

    /// Borrow the out-of-band lookaside.
    pub fn lookaside(&self) -> &RdfLookaside {
        &self.lookaside
    }

    /// Borrow the blob store.
    pub fn blobs(&self) -> &ContentStore {
        &self.blobs
    }

    /// Borrow the provenance sidecar.
    pub fn provenance(&self) -> &DatasetProvenance {
        &self.provenance
    }

    /// The typed handle for a backing graph IRI, if one is attached.
    pub fn handle(&self, graph: &str) -> Option<&HandleEntry<H>> {
        self.handles.get(graph)
    }

    /// Attach a typed handle for the named graph `graph`, pinning `payload` to the
    /// canonical digest of that graph's subgraph.
    ///
    /// The supplied `content_digest` MUST equal the canonical digest of the backing
    /// named graph (see [`Self::graph_digest`]); on mismatch this HARD-fails with
    /// [`PipelineBundleError::HandleDigestMismatch`] and the bundle is left
    /// unchanged. A previously attached handle for the same graph is replaced.
    ///
    /// # Errors
    ///
    /// [`PipelineBundleError::HandleDigestMismatch`] if the pinned digest disagrees
    /// with the backing graph.
    pub fn pin_handle(
        &mut self,
        graph: impl Into<HandleKey>,
        payload: H,
        content_digest: ContentDigest,
    ) -> Result<(), PipelineBundleError> {
        let graph = graph.into();
        let actual = self.graph_digest(&graph);
        if actual != content_digest {
            return Err(PipelineBundleError::HandleDigestMismatch {
                graph,
                pinned: content_digest,
                actual,
            });
        }
        self.handles
            .insert(graph, HandleEntry::new(payload, content_digest));
        Ok(())
    }

    /// Detach the typed handle for `graph`, returning it if present. Detaching does
    /// NOT change [`digest`](Self::digest) (the handle lane is excluded).
    pub fn detach_handle(&mut self, graph: &str) -> Option<HandleEntry<H>> {
        self.handles.remove(graph)
    }

    /// The canonical [`ContentDigest`] of the named graph `graph` — the subgraph of
    /// the dataset whose quads carry `g == <graph>`, canonicalized to N-Quads.
    ///
    /// Built by projecting the matching quads into a fresh dataset and hashing its
    /// canonical form. The RDF 1.2 reifier/annotation side-tables are graph-scopeless
    /// in this IR (a reifier binding carries no graph dimension), so they travel with
    /// the projection in whole — a handle over a reified subgraph therefore pins over
    /// the same statement layer the dataset carries. This is the value a handle's
    /// pinned digest is checked against in [`pin_handle`](Self::pin_handle).
    #[must_use]
    pub fn graph_digest(&self, graph: &str) -> ContentDigest {
        let subgraph = self.project_named_graph(graph);
        ContentDigest::of(canonicalize(&subgraph).nquads.as_bytes())
    }

    /// Project the quads of one named graph into a fresh default-graph dataset,
    /// carrying the reifier/annotation side-tables whose statements lie in that
    /// graph's quads. Used only to compute [`graph_digest`](Self::graph_digest).
    fn project_named_graph(&self, graph: &str) -> RdfDataset {
        let mut builder = super::builder::RdfDatasetBuilder::new();
        // Only quads in the requested named graph contribute; we drop the graph
        // label so the projection is the graph's content in isolation.
        for quad in self.dataset.owned_quads() {
            let in_graph = matches!(
                &quad.graph_name,
                Some(RdfTerm::Iri(iri)) if iri == graph
            );
            if !in_graph {
                continue;
            }
            let mut projected = quad.clone();
            projected.graph_name = None;
            builder.push_owned_quad(&projected);
        }
        // Carry the RDF 1.2 statement layer: reifier bindings and annotations whose
        // reified statement is one of the projected quads travel with the subgraph,
        // so a handle over a reified graph pins over the same content the graph holds.
        for reifier in self.dataset.owned_reifiers() {
            builder.push_owned_reifier(&reifier);
        }
        for annotation in self.dataset.owned_annotations() {
            builder.push_owned_annotation(&annotation);
        }
        Arc::try_unwrap(
            builder
                .freeze()
                .expect("a sub-projection of a valid dataset is valid"),
        )
        .unwrap_or_else(|arc| (*arc).owned_snapshot())
    }

    /// The content [`ContentDigest`] of this bundle: a SHA-256 fold over the
    /// dataset's canonical hash, the SORTED lookaside resource digests, the SORTED
    /// blob digests, and the runtime-id-free public provenance projection. The
    /// typed-handle lane contributes NOTHING (see the module docs).
    #[must_use]
    pub fn digest(&self) -> ContentDigest {
        let mut hasher = Sha256::new();

        // 1. The canonical N-Quads hash of the dataset (RDF-1.2 overlay included).
        hasher.update(canonicalize(&self.dataset).nquads.as_bytes());
        hasher.update([SEP_SECTION]);

        // 2. Each lookaside resource's content_digest, collected + SORTED so the fold
        //    is order-independent. A resource without a declared digest contributes
        //    an empty marker so present-but-undigested resources still register.
        let mut resource_digests: Vec<&str> = self
            .lookaside
            .resources
            .iter()
            .map(|r| r.content_digest.as_deref().unwrap_or(""))
            .collect();
        resource_digests.sort_unstable();
        for d in resource_digests {
            hasher.update(d.as_bytes());
            hasher.update([SEP_RECORD]);
        }
        hasher.update([SEP_SECTION]);

        // 3. Each blob's ContentDigest in the store, SORTED (the store is a hash map,
        //    so iteration order is otherwise nondeterministic).
        let mut blob_digests: Vec<ContentDigest> =
            self.blobs.iter().map(|(digest, _)| *digest).collect();
        blob_digests.sort_unstable();
        for d in blob_digests {
            hasher.update(d.as_bytes());
            hasher.update([SEP_RECORD]);
        }
        hasher.update([SEP_SECTION]);

        // 4. The PUBLIC provenance projection (unit names, kinds, artifact paths,
        //    locations) — NEVER the runtime numeric ids (S0.5). The projection is
        //    sorted by `public_projection`, so it is allocation-order-independent.
        for (unit, kind, artifact, location) in self.provenance.public_projection() {
            hasher.update(unit.as_bytes());
            hasher.update([SEP_FIELD]);
            hasher.update(kind.as_bytes());
            hasher.update([SEP_FIELD]);
            hasher.update(artifact.as_bytes());
            hasher.update([SEP_FIELD]);
            hasher.update(location.as_deref().unwrap_or("").as_bytes());
            hasher.update([SEP_RECORD]);
        }

        let out = hasher.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        ContentDigest::from_raw(buf)
    }
}

impl RdfDataset {
    /// A fresh owned `RdfDataset` snapshotting this one's frozen tables. The
    /// fallback for the rare case a freshly-frozen `Arc` is shared; the lazy caches
    /// rebuild on demand. Crate-internal — the public deep-copy path is
    /// [`union`](RdfDataset::union) of a single input.
    pub(crate) fn owned_snapshot(&self) -> RdfDataset {
        RdfDataset::union(&[self])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;
    use crate::provenance::OriginKind;
    use crate::{RdfLookasideKind, RdfLookasideResource, TermId};

    /// A trivial synthetic handle payload — C1 has no real pipeline handle types yet,
    /// so the pin check is exercised with this stand-in. Pipeline-side payloads plug
    /// into the same lane.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SyntheticHandle {
        note: String,
    }

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    /// Build a dataset with one default-graph quad and one quad in named graph `g`.
    fn dataset_with_named_graph() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let go = iri(&mut b, "go");
        let g = b.intern_iri("http://example.org/graph".to_string());
        b.push_quad(s, p, o, None); // default graph
        b.push_quad(s, p, go, Some(g)); // named graph
        b.freeze().expect("valid")
    }

    fn empty_bundle() -> PipelineBundle<SyntheticHandle> {
        PipelineBundle::new(
            dataset_with_named_graph(),
            RdfLookaside::default(),
            Arc::new(ContentStore::new()),
            DatasetProvenance::new(),
        )
    }

    #[test]
    fn new_bundle_exposes_parts_and_empty_handles() {
        let bundle = empty_bundle();
        assert_eq!(bundle.dataset().quad_count(), 2);
        assert!(bundle.lookaside().is_empty());
        assert!(bundle.blobs().is_empty());
        assert!(bundle.handles.is_empty());
    }

    #[test]
    fn pin_handle_matching_digest_succeeds() {
        let mut bundle = empty_bundle();
        let graph = "http://example.org/graph";
        let digest = bundle.graph_digest(graph);
        let payload = SyntheticHandle {
            note: "logic-program".to_owned(),
        };
        bundle
            .pin_handle(graph, payload.clone(), digest)
            .expect("matching digest pins");
        assert_eq!(bundle.handle(graph).map(|h| &h.payload), Some(&payload));
    }

    #[test]
    fn pin_handle_mismatched_digest_hard_fails() {
        let mut bundle = empty_bundle();
        let graph = "http://example.org/graph";
        // A digest of unrelated bytes — cannot equal the backing graph's canon.
        let wrong = ContentDigest::of(b"not the graph");
        let err = bundle
            .pin_handle(
                graph,
                SyntheticHandle {
                    note: "bad".to_owned(),
                },
                wrong,
            )
            .expect_err("mismatched digest must hard-fail");
        assert!(matches!(
            err,
            PipelineBundleError::HandleDigestMismatch { .. }
        ));
        // The bundle is unchanged on failure.
        assert!(bundle.handle(graph).is_none());
    }

    #[test]
    fn graph_digest_distinguishes_graphs_and_is_isolated() {
        let bundle = empty_bundle();
        // The named graph's projection (one quad) differs from an absent graph
        // (empty projection → canon of "").
        let present = bundle.graph_digest("http://example.org/graph");
        let absent = bundle.graph_digest("http://example.org/missing");
        assert_ne!(present, absent, "present vs empty projection differ");
        // The absent-graph digest is the canon of the empty dataset.
        let empty_ds = RdfDatasetBuilder::new().freeze().expect("empty");
        assert_eq!(
            absent,
            ContentDigest::of(canonicalize(&empty_ds).nquads.as_bytes())
        );
    }

    #[test]
    fn digest_is_stable_across_handle_attach_and_detach() {
        let mut bundle = empty_bundle();
        let before = bundle.digest();
        let graph = "http://example.org/graph";
        let digest = bundle.graph_digest(graph);
        bundle
            .pin_handle(
                graph,
                SyntheticHandle {
                    note: "h".to_owned(),
                },
                digest,
            )
            .expect("pin");
        assert_eq!(
            bundle.digest(),
            before,
            "attaching a handle does not change the bundle digest"
        );
        let _ = bundle.detach_handle(graph);
        assert_eq!(
            bundle.digest(),
            before,
            "detaching a handle does not change the bundle digest"
        );
    }

    #[test]
    fn digest_is_sensitive_to_the_dataset() {
        let a = empty_bundle();
        let b = {
            let mut bld = RdfDatasetBuilder::new();
            let (s, p, o) = (
                iri(&mut bld, "s"),
                iri(&mut bld, "p"),
                iri(&mut bld, "DIFFERENT"),
            );
            bld.push_quad(s, p, o, None);
            PipelineBundle::<SyntheticHandle>::new(
                bld.freeze().expect("valid"),
                RdfLookaside::default(),
                Arc::new(ContentStore::new()),
                DatasetProvenance::new(),
            )
        };
        assert_ne!(
            a.digest(),
            b.digest(),
            "a different dataset changes the digest"
        );
    }

    #[test]
    fn digest_is_sensitive_to_a_lookaside_resource() {
        let base = empty_bundle();
        let base_digest = base.digest();
        let mut with_resource = empty_bundle();
        with_resource.lookaside.resources.push(
            RdfLookasideResource::new(RdfLookasideKind::Reasoning)
                .with_name("closure")
                .with_digest("deadbeef"),
        );
        assert_ne!(
            with_resource.digest(),
            base_digest,
            "adding a lookaside resource changes the digest"
        );
    }

    #[test]
    fn digest_is_sensitive_to_a_blob() {
        let base_digest = empty_bundle().digest();
        let mut store = ContentStore::new();
        store.insert(b"a blob payload".to_vec());
        let with_blob = PipelineBundle::<SyntheticHandle>::new(
            dataset_with_named_graph(),
            RdfLookaside::default(),
            Arc::new(store),
            DatasetProvenance::new(),
        );
        assert_ne!(
            with_blob.digest(),
            base_digest,
            "adding a blob changes the digest"
        );
    }

    #[test]
    fn digest_is_sensitive_to_the_public_provenance() {
        let base_digest = empty_bundle().digest();
        let mut prov = DatasetProvenance::new();
        let unit = prov.register_unit("slices/core/epistemics", OriginKind::Source);
        let artifact = prov.register_artifact("slices/core/epistemics/epistemics.ttl");
        prov.record_occurrence(
            crate::ir::QuadHandle::from_index(0),
            unit,
            artifact,
            Some("epistemics.ttl:1".to_owned()),
        );
        let with_prov = PipelineBundle::<SyntheticHandle>::new(
            dataset_with_named_graph(),
            RdfLookaside::default(),
            Arc::new(ContentStore::new()),
            prov,
        );
        assert_ne!(
            with_prov.digest(),
            base_digest,
            "a non-empty public provenance changes the digest"
        );
    }

    /// S0.5: the digest is over the PUBLIC projection, never runtime ids. Two
    /// provenances with the SAME public content but DIFFERENT internal id allocation
    /// order must produce the SAME bundle digest. We allocate the same two
    /// (unit, artifact) occurrences in opposite registration orders — the numeric
    /// `UnitId`/`ArtifactId` differ, but the public names/paths are identical.
    #[test]
    fn digest_excludes_runtime_ids_public_projection_only() {
        let build = |reversed: bool| -> ContentDigest {
            let mut prov = DatasetProvenance::new();
            // Two occurrences sharing one quad handle, registered in one of two
            // internal orders. The PUBLIC content (names, paths, locations) is the
            // same set either way; only the numeric ids differ.
            let specs = [
                ("unit-a", "art-a.ttl", "a:1"),
                ("unit-b", "art-b.ttl", "b:1"),
            ];
            let order: Vec<usize> = if reversed { vec![1, 0] } else { vec![0, 1] };
            for &i in &order {
                let (uname, apath, loc) = specs[i];
                let unit = prov.register_unit(uname, OriginKind::Source);
                let artifact = prov.register_artifact(apath);
                prov.record_occurrence(
                    crate::ir::QuadHandle::from_index(0),
                    unit,
                    artifact,
                    Some(loc.to_owned()),
                );
            }
            PipelineBundle::<SyntheticHandle>::new(
                dataset_with_named_graph(),
                RdfLookaside::default(),
                Arc::new(ContentStore::new()),
                prov,
            )
            .digest()
        };
        assert_eq!(
            build(false),
            build(true),
            "identical public provenance in a different internal id order must digest identically"
        );
    }
}
