// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! THE IDENTITY GATE: a zstd-compressed claim is the SAME claim.
//!
//! Everything else about the medium axis — that the dictionaries are declared, trained,
//! measured, pinned, projected and cited — is true of an axis that quietly changed what
//! the bundle says. This gate is the razor. The REAL carrier is emitted TWICE from ONE
//! in-memory DAG run:
//!
//! * once under the authored assignment, whose primed blob reps all name
//!   `gmeow:mediumProfileDistL12` — the shipped emission, taken off the terminal sink's
//!   own product so the subject is literally the deliverable;
//! * once under the DECLARED `gmeow:mediumProfileBaselineL12`, a first-class named
//!   selection, through the SAME production door
//!   (`carrier::serialize_carrier_snapshot`, the function `stage-gts-sink` itself
//!   calls) at a second medium. Not a sibling test-only serializer, not an empty
//!   registry and not `MediumPlan::undicted` — the first would compare two code paths
//!   rather than two media, and the other two would be the legacy no-dict mode this axis
//!   exists to remove, leaving nothing on the artifact saying which medium it is. The
//!   baseline emission still PINS every declared dictionary (the pack is the dictionary
//!   family's distribution channel) and primes no frame with any of them, so the two
//!   emissions differ in priming and in nothing else.
//!
//! Three assertions, then the artifact-invariance leg:
//!
//! 1. the decoded FOLD of both is byte-identical — the same RDFC-1.0 canonical N-Quads
//!    per named graph, and the same reconstructed bytes for every committed path the
//!    bundle carries — with the ONE difference confined to, and exactly characterized on,
//!    the `gmeow:MediumEnvelope` subgraph, which is the projection OF the medium and so is
//!    the one thing that must differ;
//! 2. every envelope's `gmeow:contentDigest` equals the blake3 of the bytes ACTUALLY
//!    DECODED off the wire, and the frame's own in-band `pub.digest`;
//! 3. the rsyncable block count and the uncompressed cut points are unchanged between the
//!    two emissions — MEASURED through `purrdf::gts::codec::zstd_block_layout`, not
//!    asserted. This is the claim that priming preserves the delta-transfer property, and
//!    an unmeasured version of it would be a comment.
//!
//! Plus **leg 1 of zero-model-facing-change**: every GMN-dialect artifact reconstructs
//! byte-identically from both emissions, over a path set DERIVED from the emitted bundle
//! (never hardcoded), with its own red fixture.
//!
//! The DAG is run ONCE and every clause lives in one test function, for the same reason
//! `tests/medium_bundle.rs` does: splitting them would multiply a whole-pipeline execution
//! by the number of clauses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ciborium::value::Value;
use gmeow_pipeline::gmn_dialect::{self, ModelFacingReport, check_artifact_invariance};
use gmeow_pipeline::medium::MEDIUM_REGISTRY_GRAPH;
use gmeow_pipeline::medium::registry::{MediumRegistry, MediumSelection};
use gmeow_pipeline::stages::medium_dictionaries::frame_iri;
use gmeow_pipeline::{PipelineCache, RunContext, bind, default_registry, full_spec, run};
use purrdf::gts::codec::{Codec, ZstdBlockInfo, decode_chain, zstd_block_layout};
use purrdf::gts::wire::{iter_items, map_get, unwrap_header};
use purrdf::{RdfQuad, RdfTerm};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The two declared media this gate emits through, as WIRE labels (the test may not
/// borrow a Rust constant — the point is that the declaration and the emitted envelopes
/// agree).
const MEDIUM_DIST: &str = "https://blackcatinformatics.ca/gmeow/mediumProfileDistL12";
const MEDIUM_BASELINE: &str = "https://blackcatinformatics.ca/gmeow/mediumProfileBaselineL12";

/// The ONLY two envelope predicates allowed to differ between the two emissions: the
/// medium the bytes were written through, and the dictionary that primed them. Both are
/// projections OF the medium, so a medium change that did not move them would mean the
/// envelope is not reading the wire.
const MEDIUM_DEPENDENT_PREDICATES: [&str; 2] = ["envelopeMedium", "envelopeDictionary"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Run the REAL production DAG (`full_spec`, the same spec `make regen` executes) once,
/// in memory, over a temp cache, and return every stage product.
fn run_the_dag(root: &Path) -> BTreeMap<String, gmeow_pipeline::node::StageProduct> {
    let spec = full_spec();
    let graph = spec.validate().expect("the production DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("every production stage binds");
    let cache_dir = tempfile::tempdir().expect("tempdir");
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    let mut ctx = RunContext::open(root, jobs).expect("run context");
    ctx.cache = PipelineCache::open(cache_dir.path()).expect("temp cache");
    run(&graph, &bound, &mut ctx)
        .expect("the production DAG runs end to end")
        .products
}

fn is_segment_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == 55799,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

/// One payload-bearing frame, as the wire carries it.
struct WireFrame {
    /// `pub.rep`, absent on the snapshot frame (the one frame with no public metadata).
    rep: Option<String>,
    /// `pub.digest`, absent on the snapshot frame.
    digest: Option<String>,
    /// The RAW, still-encoded payload bytes — what `zstd_block_layout` walks.
    encoded: Vec<u8>,
    /// The frame's single transform, resolved to a decode-side catalog entry with the
    /// header's pinned dictionary bytes already substituted in.
    codec: Codec,
    /// The `"dct"` name the frame's catalog entry binds, when it binds one.
    dictionary: Option<String>,
}

impl WireFrame {
    /// The bytes this frame ACTUALLY decodes to, through the frame's own declared
    /// transform primed by the header's own pinned dictionary.
    ///
    /// Decoded here rather than through the reader's `decoded_vec` helper because this
    /// gate must see EVERY frame, including the documentation-scale payloads that trip
    /// the reader's decode safety bound — and because "the bytes actually decoded off the
    /// wire" is precisely what assertion 2 is about.
    fn decode(&self) -> Vec<u8> {
        decode_chain(std::slice::from_ref(&self.codec), &self.encoded).unwrap_or_else(|err| {
            panic!(
                "the {:?} frame does not decode through its own declared transform {:?}: {err}",
                self.rep, self.codec.name
            )
        })
    }
}

/// Every payload-bearing frame of `bundle`, with its transform resolved against the
/// segment header's codec catalog and in-band `"dct"` map.
fn wire_frames(bundle: &[u8]) -> Vec<WireFrame> {
    let (items, torn) = iter_items(bundle);
    assert!(torn.is_none(), "the emitted bundle is a torn CBOR sequence");
    let dicts = gmeow_gts_profile::segment_dictionaries(bundle)
        .expect("the emitted bundle's header reads back");

    let mut catalog: BTreeMap<i128, (String, Option<String>, Option<i32>)> = BTreeMap::new();
    let mut out: Vec<WireFrame> = Vec::new();
    for (_, item) in &items {
        if is_segment_header(item) {
            let header = unwrap_header(item).expect("a segment header unwraps");
            let Some(Value::Map(entries)) = map_get(header, "cat") else {
                panic!("the segment header carries no codec catalog");
            };
            catalog.clear();
            for (id, descriptor) in entries {
                let (Value::Integer(id), Value::Map(fields)) = (id, descriptor) else {
                    continue;
                };
                let name = match map_get(fields, "name") {
                    Some(Value::Text(name)) => name.clone(),
                    _ => continue,
                };
                let dct = match map_get(fields, "dct") {
                    Some(Value::Text(dct)) => Some(dct.clone()),
                    _ => None,
                };
                let level = match map_get(fields, "level") {
                    Some(Value::Integer(level)) => i32::try_from(i128::from(*level)).ok(),
                    _ => None,
                };
                catalog.insert(i128::from(*id), (name, dct, level));
            }
            continue;
        }
        let Value::Map(entries) = item else { continue };
        let Some(Value::Bytes(encoded)) = map_get(entries, "d") else {
            continue;
        };
        let id = match map_get(entries, "x") {
            Some(Value::Array(chain)) if chain.len() == 1 => match &chain[0] {
                Value::Integer(id) => i128::from(*id),
                other => panic!("a transform id must be a CBOR integer, got {other:?}"),
            },
            other => panic!("a payload frame must carry exactly one transform, got {other:?}"),
        };
        let (name, dct, level) = catalog
            .get(&id)
            .unwrap_or_else(|| panic!("frame transform id {id} is not in the segment catalog"))
            .clone();
        let (rep, digest) = match map_get(entries, "pub") {
            Some(Value::Map(meta)) => (
                match map_get(meta, "rep") {
                    Some(Value::Text(rep)) => Some(rep.clone()),
                    _ => None,
                },
                match map_get(meta, "digest") {
                    Some(Value::Text(digest)) => Some(digest.clone()),
                    _ => None,
                },
            ),
            _ => (None, None),
        };
        let dct_bytes = dct.as_ref().map(|name| {
            dicts
                .get(name)
                .unwrap_or_else(|| {
                    panic!("the header binds codec dictionary {name:?} but pins no such entry")
                })
                .clone()
        });
        out.push(WireFrame {
            rep,
            digest,
            encoded: encoded.clone(),
            codec: Codec {
                name,
                cls: "compress".to_string(),
                dct: dct_bytes,
                level,
            },
            dictionary: dct,
        });
    }
    out
}

/// The `graph → RDFC-1.0 canonical N-Quads` map of a bundle's decoded fold.
///
/// Canonicalized PER NAMED GRAPH so the comparison is the one the claim is about ("the
/// same claim in every graph") rather than one whole-dataset digest that would say only
/// that something, somewhere, moved.
fn canonical_by_graph(bundle: &[u8]) -> BTreeMap<String, String> {
    let folded = purrdf::import_gts_events(bundle).expect("the emitted bundle folds back");
    let quads: Vec<RdfQuad> = purrdf::flat_rdf_quads_from_dataset(folded.dataset.as_ref());
    let mut by_graph: BTreeMap<String, Vec<RdfQuad>> = BTreeMap::new();
    for quad in quads {
        let key = match &quad.graph_name {
            Some(RdfTerm::Iri(iri)) => iri.clone(),
            Some(other) => format!("{other:?}"),
            None => "<default>".to_string(),
        };
        by_graph.entry(key).or_default().push(quad);
    }
    by_graph
        .into_iter()
        .map(|(graph, quads)| {
            let frozen =
                purrdf::flat_dataset_from_quads(&quads).expect("the graph's quad set freezes");
            let canonical =
                purrdf::canonical_flat_nquads(&frozen).expect("the graph canonicalizes");
            (graph, canonical)
        })
        .collect()
}

/// The `gmeow:MediumEnvelope` quads of a fold, as `(subject, predicate-local, object)`
/// triples in canonical order.
fn envelope_rows(bundle: &[u8]) -> BTreeSet<(String, String, String)> {
    let folded = purrdf::import_gts_events(bundle).expect("the emitted bundle folds back");
    let quads: Vec<RdfQuad> = purrdf::flat_rdf_quads_from_dataset(folded.dataset.as_ref());
    let registry_graph = Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH));
    let envelope_class = RdfTerm::iri(format!("{GMEOW}MediumEnvelope"));
    let subjects: BTreeSet<String> = quads
        .iter()
        .filter(|quad| {
            quad.graph_name == registry_graph
                && quad.predicate == RDF_TYPE
                && quad.object == envelope_class
        })
        .filter_map(|quad| match &quad.subject {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect();
    quads
        .iter()
        .filter(|quad| quad.graph_name == registry_graph)
        .filter_map(|quad| {
            let RdfTerm::Iri(subject) = &quad.subject else {
                return None;
            };
            if !subjects.contains(subject) {
                return None;
            }
            let object = match &quad.object {
                RdfTerm::Iri(iri) => iri.clone(),
                RdfTerm::Literal(literal) => literal.lexical_form.clone(),
                other => format!("{other:?}"),
            };
            Some((
                subject.clone(),
                quad.predicate
                    .strip_prefix(GMEOW)
                    .unwrap_or(&quad.predicate)
                    .to_string(),
                object,
            ))
        })
        .collect()
}

fn blake3(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// The block layout of one frame's payload — the rsyncable delta property, observed on
/// the wire without decompressing it.
fn layout(frame: &WireFrame) -> Vec<ZstdBlockInfo> {
    zstd_block_layout(&frame.encoded).unwrap_or_else(|err| {
        panic!(
            "the {:?} frame's payload is not a readable zstd frame sequence: {err}",
            frame.rep
        )
    })
}

#[test]
fn medium_identity_gate() {
    let root = repo_root();
    let products = run_the_dag(&root);

    // ── the two emissions, from ONE run's carrier ──
    let dist = products
        .get("stage-gts-sink")
        .expect("the terminal sink produced a product")
        .artifact(gmeow_pipeline::stages::gts_sink::GTS_PATH)
        .expect("the sink product carries the gmeow.gts artifact")
        .to_vec();
    let carrier = gmeow_pipeline::stages::carrier::snapshot_dataset(&products)
        .expect("this run's assembled carrier");
    // The counterfactual goes through the PRODUCTION door — the very function
    // `stage-gts-sink` calls, at a second declared medium instead of a second entry
    // point. A sibling test-only serializer would make this comparison one between two
    // code paths rather than between two media, which is not the claim.
    let baseline = gmeow_pipeline::stages::carrier::serialize_carrier_snapshot(
        &root,
        &products,
        carrier.as_ref(),
        &MediumSelection::baseline_profile(),
    )
    .expect("the SAME carrier re-emits through the declared no-dictionary medium");

    assert!(dist.len() > 1024 && baseline.len() > 1024);
    // Both emissions still satisfy the universal frame profile: one zstd-rsyncable
    // transform at the declared level. The baseline is a different MEDIUM, never a
    // weaker profile.
    gmeow_pipeline::validate_mandated_frames(&dist)
        .expect("the primed emission uses the mandated frame profile");
    gmeow_pipeline::validate_mandated_frames(&baseline)
        .expect("the baseline emission uses the mandated frame profile");
    // NON-VACUITY: the two emissions must actually be different bytes, or every
    // agreement below is an agreement of a thing with itself.
    assert_ne!(
        blake3(&dist),
        blake3(&baseline),
        "the two declared media produced byte-identical bundles — the counterfactual did \
         not happen, so nothing below proves anything"
    );
    println!(
        "emissions: dist {} B, declared-baseline {} B",
        dist.len(),
        baseline.len()
    );

    // The counterfactual is a DECLARED, NAMED medium: both emissions pin the same
    // dictionary family (the pack is that family's distribution channel), and they differ
    // only in what primes a frame.
    let dist_dicts =
        gmeow_gts_profile::segment_dictionaries(&dist).expect("the primed header reads back");
    let baseline_dicts =
        gmeow_gts_profile::segment_dictionaries(&baseline).expect("the baseline header reads back");
    assert_eq!(
        dist_dicts, baseline_dicts,
        "the baseline emission must still SHIP the declared dictionaries — it is a named \
         medium that primes nothing with them, not an empty registry"
    );
    assert!(
        !dist_dicts.is_empty(),
        "neither emission pins a dictionary — the dist arm is not dictionary-primed at all"
    );

    let module = {
        let folded = purrdf::import_gts_events(&dist).expect("the primed bundle folds back");
        MediumRegistry::from_dataset(folded.dataset.as_ref())
            .expect("the shipped bundle carries a readable medium axis")
    };
    assert!(
        module.media().contains_key(MEDIUM_BASELINE),
        "the baseline medium must be DECLARED — an undeclared counterfactual would be the \
         legacy no-dict mode wearing a name"
    );

    // ── (1) the decoded FOLD of both is byte-identical ──
    let dist_graphs = canonical_by_graph(&dist);
    let baseline_graphs = canonical_by_graph(&baseline);
    assert_eq!(
        dist_graphs.keys().collect::<Vec<_>>(),
        baseline_graphs.keys().collect::<Vec<_>>(),
        "the two emissions fold to different NAMED GRAPH sets"
    );
    assert!(
        dist_graphs.len() > 10,
        "only {} graph(s) folded back — the comparison is too thin to be the razor",
        dist_graphs.len()
    );
    let differing: Vec<&String> = dist_graphs
        .iter()
        .filter(|(graph, canonical)| baseline_graphs.get(*graph) != Some(*canonical))
        .map(|(graph, _)| graph)
        .collect();
    assert_eq!(
        differing,
        vec![&MEDIUM_REGISTRY_GRAPH.to_string()],
        "a zstd-primed claim is the SAME claim: every named graph must canonicalize \
         identically under both declared media. The ONE permitted difference is \
         graph/medium-registry, which carries the gmeow:MediumEnvelope projection OF the \
         medium — and that graph MUST differ, or the envelopes are not reading the wire"
    );
    println!(
        "fold: {} named graph(s) canonically identical; the medium-registry graph differs by \
         construction",
        dist_graphs.len() - 1
    );

    // …and the medium-registry difference is EXACTLY the envelope subgraph's medium
    // coordinates. Anything else moving there would be a claim that changed.
    let dist_rows = envelope_rows(&dist);
    let baseline_rows = envelope_rows(&baseline);
    assert!(!dist_rows.is_empty(), "the bundle carries no envelope rows");
    let only_dist: BTreeSet<&(String, String, String)> =
        dist_rows.difference(&baseline_rows).collect();
    let only_baseline: BTreeSet<&(String, String, String)> =
        baseline_rows.difference(&dist_rows).collect();
    assert!(
        !only_dist.is_empty() && !only_baseline.is_empty(),
        "the envelope subgraph is identical under both media — the envelopes are projecting \
         an INTENTION rather than the wire"
    );
    for (subject, predicate, object) in only_dist.iter().chain(only_baseline.iter()) {
        assert!(
            MEDIUM_DEPENDENT_PREDICATES.contains(&predicate.as_str()),
            "envelope <{subject}> gmeow:{predicate} {object:?} differs between the two \
             emissions, but only the medium coordinates ({MEDIUM_DEPENDENT_PREDICATES:?}) may \
             — a digest, a schema or a stratum that moved means the medium changed the CLAIM"
        );
    }
    // The two emissions name the two DECLARED media, and nothing else.
    let medium_of = |rows: &BTreeSet<(String, String, String)>| -> BTreeSet<String> {
        rows.iter()
            .filter(|(_, predicate, _)| predicate == "envelopeMedium")
            .map(|(_, _, object)| object.clone())
            .collect()
    };
    assert!(
        medium_of(&dist_rows).contains(MEDIUM_DIST),
        "the shipped emission's envelopes must name the dist medium; got {:?}",
        medium_of(&dist_rows)
    );
    assert_eq!(
        medium_of(&baseline_rows),
        [MEDIUM_BASELINE.to_string()].into_iter().collect(),
        "every envelope of the counterfactual emission must name the ONE declared \
         no-dictionary medium"
    );
    assert!(
        !baseline_rows
            .iter()
            .any(|(_, predicate, _)| predicate == "envelopeDictionary"),
        "the declared no-dictionary medium primes nothing, so no envelope may name a \
         gmeow:envelopeDictionary — its absence IS that medium's selection"
    );
    assert!(
        dist_rows
            .iter()
            .any(|(_, predicate, _)| predicate == "envelopeDictionary"),
        "no envelope of the shipped emission names a dictionary — the dist arm is not primed"
    );

    // …and the same reconstructed bytes for every committed path the bundle carries.
    let dist_projection = gmeow_pipeline::stages::superset::project_bundle(&dist)
        .expect("the primed bundle projects")
        .files;
    let baseline_projection = gmeow_pipeline::stages::superset::project_bundle(&baseline)
        .expect("the baseline bundle projects")
        .files;
    assert!(
        dist_projection.len() > 100,
        "only {} committed path(s) reconstruct — too thin",
        dist_projection.len()
    );
    let moved: Vec<String> = dist_projection
        .iter()
        .filter(|(path, bytes)| baseline_projection.get(*path) != Some(*bytes))
        .map(|(path, bytes)| {
            format!(
                "{path} ({} B vs {:?} B)",
                bytes.len(),
                baseline_projection.get(path).map(Vec::len)
            )
        })
        .collect();
    assert!(
        moved.is_empty(),
        "{} committed artifact(s) reconstruct DIFFERENTLY under the two declared media: \
         {moved:?}",
        moved.len()
    );
    assert_eq!(
        dist_projection.keys().collect::<Vec<_>>(),
        baseline_projection.keys().collect::<Vec<_>>(),
        "the two emissions reconstruct different committed path SETS"
    );
    println!(
        "projection: {} committed path(s) byte-identical across both media",
        dist_projection.len()
    );

    // ── (2) every envelope's contentDigest is the blake3 of the DECODED bytes ──
    let dist_frames = wire_frames(&dist);
    let baseline_frames = wire_frames(&baseline);
    assert_eq!(
        dist_frames.len(),
        baseline_frames.len(),
        "the two emissions carry different frame counts"
    );
    assert!(
        dist_frames.len() > 10,
        "only {} payload frame(s) — too thin",
        dist_frames.len()
    );

    let digest_of_envelope: BTreeMap<String, String> = dist_rows
        .iter()
        .filter(|(_, predicate, _)| predicate == "contentDigest")
        .map(|(subject, _, object)| (subject.clone(), object.clone()))
        .collect();
    // The envelope SUBJECT is not the frame IRI: an envelope POINTS at the frame it
    // describes through `gmeow:envelopePayloadFrame`. Resolving through that predicate
    // (rather than assuming the two coincide) is also what makes the content-addressed
    // frame identity itself part of the check — a frame IRI that stopped being
    // `blake3(rep ␀ digest)` would leave the lookup empty.
    let envelope_of_frame: BTreeMap<String, String> = dist_rows
        .iter()
        .filter(|(_, predicate, _)| predicate == "envelopePayloadFrame")
        .map(|(subject, _, object)| (object.clone(), subject.clone()))
        .collect();
    let mut checked_blob_envelopes = 0usize;
    let mut snapshot_frames = 0usize;
    for frame in &dist_frames {
        let Some(rep) = &frame.rep else {
            snapshot_frames += 1;
            continue;
        };
        let in_band = frame
            .digest
            .as_ref()
            .expect("a blob frame declares its in-band pub.digest");
        // The bytes ACTUALLY DECODED off the wire, through the frame's own transform.
        let decoded = frame.decode();
        let actual = blake3(&decoded);
        assert_eq!(
            &actual, in_band,
            "the {rep:?} frame's in-band pub.digest is not the blake3 of what it decodes to"
        );
        let frame_id = frame_iri(rep, in_band);
        let envelope = envelope_of_frame.get(&frame_id).unwrap_or_else(|| {
            panic!(
                "no gmeow:MediumEnvelope describes the {rep:?} frame <{frame_id}> — the frame \
                 identity is content-addressed on (rep, digest), so an empty lookup means the \
                 emission described fewer frames than it carries"
            )
        });
        assert_eq!(
            digest_of_envelope.get(envelope),
            Some(&actual),
            "the {rep:?} envelope's gmeow:contentDigest must be the blake3 of the ACTUALLY \
             DECODED bytes — a digest that is never recomputed is a comment"
        );
        checked_blob_envelopes += 1;
    }
    assert_eq!(
        snapshot_frames, 1,
        "exactly one payload frame carries no pub metadata — the snapshot"
    );
    assert!(
        checked_blob_envelopes >= 10,
        "only {checked_blob_envelopes} blob envelope(s) were digest-checked"
    );
    println!("digests: {checked_blob_envelopes} blob envelope(s) recomputed from decoded bytes");

    // The ONE self-referential envelope — the snapshot's — commits to a STRATUM instead,
    // because its own contentDigest is taken over the payload it lives in and a reader
    // folding the bundle back re-interns that payload's blank nodes. So what is
    // recomputed here is the reader-checkable half, and the identity claim is that the
    // medium moved NEITHER half.
    let strata_of = |rows: &BTreeSet<(String, String, String)>| -> BTreeMap<String, String> {
        rows.iter()
            .filter(|(_, predicate, _)| predicate == "strataDigest")
            .map(|(subject, _, object)| (subject.clone(), object.clone()))
            .collect()
    };
    assert_eq!(
        strata_of(&dist_rows),
        strata_of(&baseline_rows),
        "every gmeow:strataDigest must be identical under both media — the stratum is the \
         CLAIM, and re-coding it may not move it"
    );
    let content_of = |rows: &BTreeSet<(String, String, String)>| -> BTreeMap<String, String> {
        rows.iter()
            .filter(|(_, predicate, _)| predicate == "contentDigest")
            .map(|(subject, _, object)| (subject.clone(), object.clone()))
            .collect()
    };
    assert_eq!(
        content_of(&dist_rows),
        content_of(&baseline_rows),
        "every gmeow:contentDigest must be identical under both media — including the \
         snapshot's, whose payload is digested BEFORE the medium-naming envelopes are folded \
         into it"
    );

    // ── (3) the rsyncable delta property, MEASURED ──
    //
    // Scoped to the BLOB frames, and the exclusion is DECLARED rather than silent: the
    // snapshot frame's payload legitimately differs between the emissions because it
    // CARRIES the envelopes that name the medium, so its cut points are a function of the
    // very difference under test. It is the same frame the dictionary-effect measurement
    // declares out of its population, for the same reason.
    // Keyed on `(rep, in-band digest)`: the frame IDENTITY, which is a function of the
    // DECODED payload and so is stable across a re-coding. Keying on position would
    // silently compare unrelated frames if emission order ever moved.
    let by_key =
        |frames: &[WireFrame]| -> BTreeMap<(String, String), (Vec<ZstdBlockInfo>, Option<String>)> {
            frames
                .iter()
                .filter_map(|frame| {
                    let rep = frame.rep.clone()?;
                    let digest = frame.digest.clone()?;
                    Some(((rep, digest), (layout(frame), frame.dictionary.clone())))
                })
                .collect()
        };
    let dist_layout = by_key(&dist_frames);
    let baseline_layout = by_key(&baseline_frames);
    assert_eq!(
        dist_layout.keys().collect::<Vec<_>>(),
        baseline_layout.keys().collect::<Vec<_>>(),
        "the two emissions carry different (rep, digest) frame identities — the payloads \
         themselves moved, so no cut-point comparison is meaningful"
    );

    let mut multi_block_frames = 0usize;
    let mut primed_frames = 0usize;
    let mut wire_primed_frames = 0usize;
    let mut denser = 0usize;
    let mut saved: i64 = 0;
    for (key, (dist_blocks, dist_dict)) in &dist_layout {
        let (baseline_blocks, baseline_dict) = &baseline_layout[key];
        assert!(
            baseline_dict.is_none(),
            "{key:?}: the DECLARED no-dictionary emission bound catalog dictionary {baseline_dict:?} \
             — the counterfactual is not unprimed"
        );
        assert_eq!(
            dist_blocks.len(),
            baseline_blocks.len(),
            "{key:?}: the rsyncable BLOCK COUNT changed between the two media — priming must \
             buy density without costing the delta-transfer property"
        );
        let dist_cuts: Vec<u64> = dist_blocks.iter().map(|b| b.content_len).collect();
        let baseline_cuts: Vec<u64> = baseline_blocks.iter().map(|b| b.content_len).collect();
        assert_eq!(
            dist_cuts, baseline_cuts,
            "{key:?}: the UNCOMPRESSED cut points moved between the two media — a primed \
             block that ends somewhere else re-writes the whole delta downstream of it"
        );
        if dist_blocks.len() > 1 {
            multi_block_frames += 1;
        }
        let dist_compressed: usize = dist_blocks.iter().map(|b| b.compressed_len).sum();
        let baseline_compressed: usize = baseline_blocks.iter().map(|b| b.compressed_len).sum();
        assert!(
            baseline_blocks.iter().all(|b| b.dictionary_id.is_none()),
            "{key:?}: the DECLARED no-dictionary emission still names a Dictionary_ID on the \
             wire — the counterfactual is not unprimed"
        );
        if dist_dict.is_some() {
            primed_frames += 1;
        }
        if dist_blocks.iter().any(|b| b.dictionary_id.is_some()) {
            wire_primed_frames += 1;
        }
        if dist_compressed < baseline_compressed {
            denser += 1;
        }
        saved += baseline_compressed as i64 - dist_compressed as i64;
    }
    println!(
        "cut points: {} blob frame(s) unchanged ({multi_block_frames} multi-block), \
         {primed_frames} primed by catalog / {wire_primed_frames} carrying an in-frame \
         Dictionary_ID, {denser} denser under priming, {saved} B net",
        dist_layout.len()
    );
    assert!(
        primed_frames > 0,
        "no frame of the shipped emission is bound to a catalog dictionary — the delta \
         property was compared between two UNPRIMED encodings, which proves nothing about \
         priming"
    );
    assert!(
        wire_primed_frames > 0,
        "no frame of the shipped emission carries a Dictionary_ID in its own zstd frame \
         header — priming would then be a catalog claim nothing on the wire corroborates"
    );
    assert!(
        multi_block_frames > 0,
        "no frame crosses a single rsyncable block, so 'the cut points are unchanged' is a \
         comparison of one-element lists"
    );
    assert!(
        denser > 0,
        "priming made no frame smaller — the dictionaries bought nothing on this emission"
    );

    // ── leg 1 of ZERO MODEL-FACING CHANGE: artifact invariance ──
    let mut invariance = ModelFacingReport::default();
    let compared =
        check_artifact_invariance(&dist_projection, &baseline_projection, &mut invariance);
    assert!(invariance.is_clean(), "{invariance}");
    println!(
        "leg 1: {} GMN-dialect artifact(s) byte-identical across both media: {compared:?}",
        compared.len()
    );
    // The clause census, printed with its two DECLARED-ZERO clauses named, so the
    // exclusions are visible rather than merely absent.
    for clause in gmn_dialect::clauses() {
        let matched: Vec<&String> = compared
            .iter()
            .filter(|path| clause.matches(path))
            .collect();
        match clause.zero_reason {
            None => println!("  clause {:?}: {} path(s)", clause.id, matched.len()),
            Some(reason) => println!(
                "  clause {:?}: 0 paths — DECLARED zero: {}",
                clause.id,
                reason
                    .split_whitespace()
                    .take(14)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }

    // The RED FIXTURE for leg 1, on the LIVE projections: perturb ONE GMN-dialect
    // artifact between the emissions and require the comparison to refuse. A gate whose
    // failure arm cannot be reached is not a gate.
    let victim = compared
        .iter()
        .next()
        .expect("the dialect set is non-empty")
        .clone();
    let mut perturbed = baseline_projection.clone();
    let original = perturbed
        .get(&victim)
        .expect("the victim path is in both projections")
        .clone();
    let mut flipped = original.clone();
    match flipped.first_mut() {
        Some(byte) => *byte ^= 0xFF,
        None => flipped.push(0),
    }
    assert_ne!(
        flipped, original,
        "the red fixture must actually perturb {victim}"
    );
    perturbed.insert(victim.clone(), flipped);
    let mut red = ModelFacingReport::default();
    check_artifact_invariance(&dist_projection, &perturbed, &mut red);
    assert!(
        !red.is_clean(),
        "a perturbed GMN-dialect artifact must red the invariance leg"
    );
    assert!(red.to_string().contains(&victim), "{red}");
    assert!(red.to_string().contains("the same claim"), "{red}");
    println!("leg 1 red fixture: perturbing {victim} reds the comparison");
}
