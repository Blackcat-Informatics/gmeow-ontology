// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The whole-bundle MEDIUM gate: the real DAG runs once, in memory, and the bundle
//! it emits is audited against the medium axis `slices/core/gts/module.ttl` declares.
//!
//! Every assertion here is about the SHIPPED artifact rather than about a component:
//! the eight declared dictionaries are pinned in the segment header a consumer
//! actually reads, one `gmeow:MediumEnvelope` describes each payload-bearing frame
//! the pack actually carries, and the self-referential snapshot envelope's stratified
//! digest is recomputed FROM the emitted bytes rather than trusted. A unit test over
//! the sealing code could pass with none of that true.
//!
//! The DAG is run ONCE, so every clause lives in one test function. Splitting them
//! would multiply a whole-pipeline execution by the number of clauses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ciborium::value::Value;
use gmeow_pipeline::medium::MEDIUM_REGISTRY_GRAPH;
use gmeow_pipeline::medium::registry::MediumRegistry;
use gmeow_pipeline::node::{Stage, StageInput, StageProduct};
use gmeow_pipeline::stages::medium_dictionaries::frame_iri;
use gmeow_pipeline::{PipelineCache, RunContext, bind, default_registry, full_spec, run};
use purrdf::gts::wire::{iter_items, map_get, unwrap_header};
use purrdf::{RdfQuad, RdfTerm};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The eight dictionaries `slices/core/gts/module.ttl` declares. Spelled out rather
/// than read back off the same registry the producer used, so a dictionary silently
/// dropped from the declaration is a FAILURE here instead of a smaller expectation.
const SHIPPED_DICTIONARIES: [&str; 8] = [
    "gmeow-claims-v1",
    "gmeow-core-v1",
    "gmeow-lang-ast-v1",
    "gmeow-logic-v1",
    "gmeow-math-v1",
    "gmeow-memory-compact-v1",
    "gmeow-memory-hot-v1",
    "gmeow-prooftrace-v1",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Run the REAL production DAG (`full_spec`, the same spec `make regen` executes)
/// once, in memory, over a temp cache, and return every stage product.
fn run_the_dag(root: &Path) -> BTreeMap<String, StageProduct> {
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

/// Whether `item` is a segment header rather than a frame.
///
/// `unwrap_header` unwraps ANY map, so it cannot be used as the discriminator: a
/// payload frame is a map too, and treating one as a header would read a catalog
/// that is not there. A header is either the self-describe-tagged map the writer
/// mints by default or a bare map carrying the `"gts": "GTS1"` magic; a frame never
/// carries `"gts"`.
fn is_segment_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == 55799,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

/// The segment header of `bundle` — the map a consumer reads the codec catalog and
/// the in-band `"dct"` dictionary table out of.
fn header(bundle: &[u8]) -> Vec<(Value, Value)> {
    let (items, torn) = iter_items(bundle);
    assert!(torn.is_none(), "the emitted bundle is a torn CBOR sequence");
    unwrap_header(&items[0].1)
        .expect("the emitted bundle begins with a segment header")
        .to_vec()
}

/// One payload-bearing frame of the emitted pack: its `pub.rep` / `pub.digest` when
/// it carries public metadata (every blob frame does; the snapshot frame does not),
/// and the codec id its single transform names.
struct PayloadFrame {
    rep: Option<String>,
    digest: Option<String>,
    codec: i128,
}

fn payload_frames(bundle: &[u8]) -> Vec<PayloadFrame> {
    let (items, _) = iter_items(bundle);
    let mut out = Vec::new();
    for (_, item) in &items {
        let Value::Map(entries) = item else { continue };
        if map_get(entries, "gts").is_some() || map_get(entries, "d").is_none() {
            continue;
        }
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
        let codec = match map_get(entries, "x") {
            Some(Value::Array(chain)) if chain.len() == 1 => match &chain[0] {
                Value::Integer(id) => i128::from(*id),
                other => panic!("a transform id must be a CBOR integer, got {other:?}"),
            },
            other => panic!("a payload frame must carry exactly one transform, got {other:?}"),
        };
        out.push(PayloadFrame { rep, digest, codec });
    }
    out
}

/// `catalog id → the header `"dct"` key that entry binds`, for every catalog row
/// that binds one.
fn catalog_dictionaries(header: &[(Value, Value)]) -> BTreeMap<i128, String> {
    let Some(Value::Map(catalog)) = map_get(header, "cat") else {
        panic!("the segment header carries no codec catalog");
    };
    let mut out = BTreeMap::new();
    for (id, descriptor) in catalog {
        let (Value::Integer(id), Value::Map(fields)) = (id, descriptor) else {
            continue;
        };
        if let Some(Value::Text(dict)) = map_get(fields, "dct") {
            out.insert(i128::from(*id), dict.clone());
        }
    }
    out
}

/// The `(subject, predicate, object)` rows of `graph`, as plain strings, for the
/// subjects typed `class`.
fn subjects_of_type(quads: &[RdfQuad], class: &str) -> BTreeSet<String> {
    quads
        .iter()
        .filter(|quad| quad.predicate == RDF_TYPE && quad.object == RdfTerm::iri(class))
        .filter_map(|quad| match &quad.subject {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect()
}

/// The single literal value of `subject predicate ?o` in `quads`.
fn literal_of(quads: &[RdfQuad], subject: &str, predicate: &str) -> String {
    let mut found: Vec<String> = quads
        .iter()
        .filter(|quad| {
            quad.subject == RdfTerm::iri(subject) && quad.predicate == format!("{GMEOW}{predicate}")
        })
        .filter_map(|quad| match &quad.object {
            RdfTerm::Literal(literal) => Some(literal.lexical_form.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "<{subject}> must carry exactly one gmeow:{predicate}"
    );
    found.pop().expect("length checked")
}

/// The single IRI value of `subject predicate ?o` in `quads`.
fn iri_of(quads: &[RdfQuad], subject: &str, predicate: &str) -> String {
    let mut found: Vec<String> = quads
        .iter()
        .filter(|quad| {
            quad.subject == RdfTerm::iri(subject) && quad.predicate == format!("{GMEOW}{predicate}")
        })
        .filter_map(|quad| match &quad.object {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "<{subject}> must carry exactly one gmeow:{predicate}"
    );
    found.pop().expect("length checked")
}

/// The medium-envelope subgraph of an emitted snapshot payload, recomputed HERE
/// rather than borrowed from the producer: every quad in `graph/medium-registry`
/// whose subject is typed `gmeow:MediumEnvelope`.
///
/// Reimplemented in the test on purpose. Calling the production splitter would make
/// the stratum check a tautology — it would compare the producer's answer with
/// itself — and the whole point of a stratified digest is that a READER can
/// reconstruct the region independently from the declaration alone.
fn split_envelope_subgraph(payload: &[RdfQuad]) -> (Vec<RdfQuad>, Vec<RdfQuad>) {
    let registry_graph = Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH));
    let envelope_class = RdfTerm::iri(format!("{GMEOW}MediumEnvelope"));
    let subjects: BTreeSet<String> = payload
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
    let is_envelope = |quad: &RdfQuad| -> bool {
        quad.graph_name == registry_graph
            && matches!(&quad.subject, RdfTerm::Iri(iri) if subjects.contains(iri))
    };
    let stratum = payload
        .iter()
        .filter(|q| !is_envelope(q))
        .cloned()
        .collect();
    let envelopes = payload.iter().filter(|q| is_envelope(q)).cloned().collect();
    (stratum, envelopes)
}

/// The RDFC-1.0 canonical N-Quads of a quad set — the serialization the stratum
/// digest commits to.
fn canonical(quads: &[RdfQuad]) -> String {
    let frozen = purrdf::flat_dataset_from_quads(quads).expect("the quad set freezes");
    purrdf::canonical_flat_nquads(&frozen).expect("the quad set canonicalizes")
}

fn blake3(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[test]
fn the_emitted_bundle_ships_its_declared_medium() {
    let root = repo_root();
    let products = run_the_dag(&root);

    let bundle = products
        .get("stage-gts-sink")
        .expect("the terminal sink produced a product")
        .artifact(gmeow_pipeline::stages::gts_sink::GTS_PATH)
        .expect("the sink product carries the gmeow.gts artifact")
        .to_vec();
    assert!(
        bundle.len() > 1024,
        "the emitted bundle is implausibly small: {} bytes",
        bundle.len()
    );
    // Whatever else the medium changed, the mandated frame profile still holds on
    // every frame: one zstd-rsyncable transform, at the declared level.
    gmeow_pipeline::validate_mandated_frames(&bundle)
        .expect("the dictionary-primed bundle still uses the mandated frame profile");

    let head = header(&bundle);

    // ── (a) the header pins the eight declared dictionaries, in band ──
    let pinned = gmeow_gts_profile::segment_dictionaries(&bundle)
        .expect("the emitted bundle's header reads back");
    let names: Vec<&str> = pinned.keys().map(String::as_str).collect();
    assert_eq!(
        names, SHIPPED_DICTIONARIES,
        "the pack's in-band \"dct\" map must pin exactly the eight declared dictionaries"
    );
    for (name, bytes) in &pinned {
        assert!(
            !bytes.is_empty(),
            "dictionary {name:?} is pinned with no bytes"
        );
    }

    // ── (b) graph/medium-registry: eight realizations + one envelope per frame ──
    let folded = purrdf::import_gts_events(&bundle).expect("the emitted bundle folds back");
    let payload: Vec<RdfQuad> = purrdf::flat_rdf_quads_from_dataset(folded.dataset.as_ref());
    let registry_quads: Vec<RdfQuad> = payload
        .iter()
        .filter(|quad| quad.graph_name == Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH)))
        .cloned()
        .collect();
    assert!(
        !registry_quads.is_empty(),
        "the shipped bundle must carry graph/medium-registry"
    );

    let module = MediumRegistry::from_dataset(folded.dataset.as_ref())
        .expect("the shipped bundle carries a readable medium axis");

    let realizations = subjects_of_type(
        &registry_quads,
        &format!("{GMEOW}CompressionDictionaryRealization"),
    );
    assert_eq!(
        realizations.len(),
        SHIPPED_DICTIONARIES.len(),
        "one gmeow:CompressionDictionaryRealization per declared dictionary; got {realizations:?}"
    );
    let realized_ids: BTreeSet<String> = realizations
        .iter()
        .map(|subject| {
            let definition = iri_of(&registry_quads, subject, "realizesDictionary");
            module
                .dictionaries()
                .get(&definition)
                .unwrap_or_else(|| panic!("<{definition}> is not a declared dictionary"))
                .id
                .clone()
        })
        .collect();
    assert_eq!(
        realized_ids.iter().map(String::as_str).collect::<Vec<_>>(),
        SHIPPED_DICTIONARIES,
        "every realization resolves back to a declared dictionary id"
    );

    let envelopes = subjects_of_type(&registry_quads, &format!("{GMEOW}MediumEnvelope"));
    let frames = payload_frames(&bundle);
    assert!(
        !envelopes.is_empty(),
        "the shipped bundle must carry at least one gmeow:MediumEnvelope"
    );
    assert_eq!(
        envelopes.len(),
        frames.len(),
        "one envelope per payload-bearing frame (blobs + the snapshot); \
         {} envelopes vs {} frames",
        envelopes.len(),
        frames.len()
    );

    // ── (c) every envelope projects its frame's OWN in-band identity ──
    let by_frame: BTreeMap<String, String> = envelopes
        .iter()
        .map(|subject| {
            (
                iri_of(&registry_quads, subject, "envelopePayloadFrame"),
                subject.clone(),
            )
        })
        .collect();
    assert_eq!(
        by_frame.len(),
        envelopes.len(),
        "two envelopes may not describe one frame"
    );

    let dict_of_codec = catalog_dictionaries(&head);
    let mut snapshot_envelope: Option<String> = None;
    for frame in &frames {
        let Some(rep) = &frame.rep else {
            // The one payload frame with no `pub` metadata is the snapshot.
            continue;
        };
        let digest = frame
            .digest
            .as_ref()
            .expect("a blob frame declares its in-band pub.digest");
        let subject = by_frame
            .get(&frame_iri(rep, digest))
            .unwrap_or_else(|| panic!("no gmeow:MediumEnvelope describes the {rep:?} frame"));
        assert_eq!(
            &literal_of(&registry_quads, subject, "contentDigest"),
            digest,
            "the {rep:?} envelope's gmeow:contentDigest must be the frame's own pub.digest"
        );
        // A whole-payload stratum digests the same bytes, so the two agree by
        // construction — the stratified pair only diverges where it must.
        assert_eq!(
            literal_of(&registry_quads, subject, "strataDigest"),
            *digest,
            "a whole-payload stratum commits to the frame's own bytes"
        );
        assert_eq!(
            iri_of(&registry_quads, subject, "envelopeDigestStratum"),
            format!("{GMEOW}stratumWholePayload")
        );
        // The envelope's declared dictionary is the one the frame's codec entry
        // actually binds — the projection is of the WIRE, not of an intention.
        let declared = iri_of(&registry_quads, subject, "envelopeDictionary");
        let in_band = dict_of_codec
            .get(&frame.codec)
            .unwrap_or_else(|| panic!("the {rep:?} frame's codec entry binds no dictionary"));
        assert_eq!(
            module
                .dictionary_by_id(in_band)
                .expect("the in-band dictionary resolves")
                .iri,
            declared,
            "the {rep:?} envelope must name the dictionary its frame was primed with"
        );
    }
    for subject in &envelopes {
        if iri_of(&registry_quads, subject, "envelopeDigestStratum")
            == format!("{GMEOW}stratumPayloadExcludingMediumEnvelope")
        {
            assert!(
                snapshot_envelope.replace(subject.clone()).is_none(),
                "exactly one envelope is stratified — the self-referential snapshot"
            );
        }
    }
    let snapshot_envelope = snapshot_envelope.expect("the snapshot frame carries an envelope");
    assert_eq!(
        iri_of(&registry_quads, &snapshot_envelope, "envelopeSchema"),
        format!("{GMEOW}payloadSchemaSnapshotWire")
    );

    // The declared stratum, recomputed from the emitted payload: the payload's quad
    // set MINUS the medium-envelope subgraph, in BOTH directions, and non-degenerate.
    let (stratum, envelope_quads) = split_envelope_subgraph(&payload);
    assert!(
        !envelope_quads.is_empty(),
        "the envelope subgraph must be non-empty, or the stratum is trivially the payload"
    );
    assert!(
        !stratum.is_empty(),
        "a degenerate (empty) stratum commits to nothing"
    );
    let payload_set: BTreeSet<String> = payload.iter().map(|q| format!("{q:?}")).collect();
    let stratum_set: BTreeSet<String> = stratum.iter().map(|q| format!("{q:?}")).collect();
    let envelope_set: BTreeSet<String> = envelope_quads.iter().map(|q| format!("{q:?}")).collect();
    assert_eq!(
        stratum_set,
        payload_set
            .difference(&envelope_set)
            .cloned()
            .collect::<BTreeSet<String>>(),
        "the stratum must be exactly payload − envelopes"
    );
    assert_eq!(
        stratum_set
            .union(&envelope_set)
            .cloned()
            .collect::<BTreeSet<String>>(),
        payload_set,
        "the stratum and the envelope subgraph must partition the payload"
    );
    assert!(
        stratum_set.is_disjoint(&envelope_set),
        "the stratum must EXCLUDE the envelope subgraph — that exclusion is why it converges"
    );

    // The stratum digest, recomputed independently over exactly that quad set.
    assert_eq!(
        literal_of(&registry_quads, &snapshot_envelope, "strataDigest"),
        blake3(canonical(&stratum).as_bytes()),
        "the snapshot envelope's gmeow:strataDigest must be the blake3 of its declared stratum"
    );
    // …and it is a genuine ADDITION: the content digest is the frame's own wire
    // identity over a different serialization, so the two are not one value twice.
    let content_digest = literal_of(&registry_quads, &snapshot_envelope, "contentDigest");
    assert_ne!(
        content_digest,
        literal_of(&registry_quads, &snapshot_envelope, "strataDigest"),
        "the stratum digest must be an addition to the witness, not a rename of it"
    );
    // The content digest is `snapshot_content_id()` VERBATIM, and the snapshot frame's
    // identity is DERIVED from it — so a digest that were not the payload's own id
    // would address a frame nothing describes. That derivation is what makes the reuse
    // checkable from the artifact alone. The payload's CBOR itself cannot be
    // re-derived by a reader (folding the graph back re-interns its blank nodes),
    // which is precisely why the reader-checkable commitment is the blank-node-
    // canonical STRATUM asserted above rather than the frame's own byte identity.
    assert!(
        content_digest.starts_with("blake3:") && content_digest.len() == 71,
        "the snapshot content digest must be canonical: {content_digest}"
    );
    assert_eq!(
        iri_of(&registry_quads, &snapshot_envelope, "envelopePayloadFrame"),
        frame_iri(gmeow_pipeline::medium::SNAPSHOT_WIRE_REP, &content_digest),
        "the snapshot frame identity is derived from the content digest the envelope \
         carries, so the digest is the payload's own id rather than a free value"
    );

    // ── (d) convergence: the emission is a fixed point ──
    let again = gmeow_pipeline::stages::gts_sink::GtsSinkStage::new()
        .run(StageInput {
            root: &root,
            upstream: &products,
        })
        .expect("the terminal re-emits")
        .product
        .artifact(gmeow_pipeline::stages::gts_sink::GTS_PATH)
        .expect("the re-emission carries the bundle")
        .to_vec();
    assert_eq!(
        blake3(&again),
        blake3(&bundle),
        "a further pass over the same carrier must reproduce the bundle byte for byte — the \
         envelopes are derived from a stratum that excludes them, so adding them cannot move it"
    );

    // ── (e)/(f) the runtime stores, primed from THIS bundle ──
    runtime_stores_are_primed_from(&bundle);
}

/// The store lane, driven through the PRODUCTION paths over the freshly emitted
/// bundle: a claim stored through `Memory::store`, an audit segment written beside
/// it, a conjecture appended to its own library, and the compaction lane over a
/// store that already held a dictionary-less segment.
///
/// "Recall still succeeds" would prove nothing — it succeeds with the medium wiring
/// deleted. What is asserted instead is that the store's HEADER pins the declared
/// dictionary and that the frames the write actually appended reference the catalog
/// entry bound to it: the record is dict-primed, not merely still readable.
fn runtime_stores_are_primed_from(bundle: &[u8]) {
    use gmeow_pipeline::mcp::{
        MEMORY_COMPACT_DICTIONARY, MEMORY_HOT_DICTIONARY, McpMode, McpServer,
    };

    let home = tempfile::tempdir().expect("tempdir");
    let memory_path = home.path().join("memory.gts");
    let conjecture_path = home.path().join("conjectures.gts");
    // SAFETY: this test binary runs one test, single-threaded, and restores nothing
    // because the process exits with it.
    unsafe {
        std::env::set_var("GMEOW_MEMORY_PATH", &memory_path);
        std::env::set_var("GMEOW_CONJECTURE_PATH", &conjecture_path);
        std::env::remove_var("GMEOW_LANG");
    }

    // A PRE-EXISTING dictionary-less store: the state every store upgraded from an
    // earlier build is in. It is written through purrdf's own default `Memory`, so
    // this is genuinely the shape produced before the medium axis existed.
    let legacy = purrdf::gts::examples::agent_memory::Memory::with_options(
        &memory_path,
        purrdf::gts::examples::agent_memory::MemoryOptions::default(),
    );
    legacy
        .store(
            "a claim written before the store had a medium",
            purrdf::gts::examples::agent_memory::StoreOptions::default(),
        )
        .expect("the legacy store accepts a claim");
    let legacy_len = std::fs::metadata(&memory_path).expect("legacy store").len();
    assert!(
        gmeow_gts_profile::segment_dictionaries(
            &std::fs::read(&memory_path).expect("read the legacy store")
        )
        .expect("the legacy store reads back")
        .is_empty(),
        "the pre-existing segment must genuinely be dictionary-less, or (f) is vacuous"
    );

    let server = McpServer::from_snapshot(bundle, None, McpMode::Consumer)
        .expect("the freshly emitted bundle serves an MCP session");

    // (e) a claim through the PRODUCTION `Memory::store` path.
    let stored = server
        .call_tool_result("store_claim", &serde_json::json!({"text": "primed claim"}))
        .to_string();
    assert!(
        stored.contains("\\\"ok\\\":true") || stored.contains("\"ok\":true"),
        "store_claim must commit: {stored}"
    );
    let store_bytes = std::fs::read(&memory_path).expect("read the store");
    assert!(
        store_bytes.len() as u64 > legacy_len,
        "the store grew by the appended record"
    );
    assert_primed(&store_bytes, MEMORY_HOT_DICTIONARY, 2);

    // Every claim — the pre-existing dictionary-less one AND the new dict-primed one
    // — is still recalled from the mixed file: each segment decodes under its own
    // declared medium.
    let recalled = server
        .call_tool_result(
            "recall",
            &serde_json::json!({"query": "claim", "limit": 10}),
        )
        .to_string();
    assert!(
        recalled.contains("primed claim"),
        "the dict-primed claim recalls: {recalled}"
    );
    assert!(
        recalled.contains("before the store had a medium"),
        "the pre-existing dictionary-less claim still recalls from the mixed file: {recalled}"
    );

    // (e) a conjecture append: its own append-only library, its own header.
    let conjecture = server.call_tool_result(
        "store_conjecture",
        &serde_json::json!({
            "formula": CONJECTURE_FORMULA,
            "kb": CONJECTURE_KB,
            "standpoint": "https://blackcatinformatics.ca/gmeow/standpoint/medium-gate",
        }),
    );
    assert!(
        conjecture.to_string().contains("\"ok\":true")
            || conjecture.to_string().contains("\\\"ok\\\":true"),
        "store_conjecture must commit: {conjecture}"
    );
    let library = std::fs::read(&conjecture_path).expect("read the conjecture library");
    assert_primed(&library, MEMORY_HOT_DICTIONARY, 2);

    // (f) the compaction lane repacks a PRE-EXISTING BASELINE pack under the compact
    // dictionary. The subject is a pack authored through the unprimed door — the exact
    // shape everything produced before the medium axis has — and compaction moves it
    // onto a declared medium without touching a content claim.
    let baseline_pack = {
        let dataset = purrdf::parse_dataset(
            b"<https://e/claim> <https://e/text> \"a claim written before the medium\" .\n",
            "application/n-triples",
            None,
        )
        .expect("the baseline pack's graph parses");
        let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
        builder
            .add_dataset(&dataset)
            .expect("fold the baseline graph");
        gmeow_gts_profile::emit_gmeow_gts(
            &builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"a content blob the compaction dictionary is derived from".repeat(64),
                media_type: "text/plain".to_string(),
                rep: "compaction-fixture".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("the baseline pack emits through the unprimed door")
    };
    assert!(
        gmeow_gts_profile::segment_dictionaries(&baseline_pack)
            .expect("the baseline pack reads back")
            .is_empty(),
        "the subject of compaction must genuinely be an unprimed pack, or (f) is vacuous"
    );
    let pack_path = home.path().join("baseline-pack.gts");
    std::fs::write(&pack_path, &baseline_pack).expect("write the baseline pack");
    let signer = (
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]),
        "medium-gate".to_string(),
    );
    gmeow_pipeline::mcp::compact_store(&pack_path, "1970-01-01T00:00:00Z", signer)
        .expect("the compaction lane repacks the baseline pack");
    let compacted = std::fs::read(&pack_path).expect("read the compacted pack");
    let pinned =
        gmeow_gts_profile::segment_dictionaries(&compacted).expect("the compacted pack reads back");
    assert!(
        pinned.contains_key(MEMORY_COMPACT_DICTIONARY),
        "the compacted pack pins {MEMORY_COMPACT_DICTIONARY}; got {:?}",
        pinned.keys().collect::<Vec<_>>()
    );
    assert_primed(&compacted, MEMORY_COMPACT_DICTIONARY, 1);
}

/// Assert that `store` pins `dictionary` in a segment header AND that at least
/// `frames` payload frames reference the catalog entry bound to it.
///
/// The second half is the load-bearing one: a header that pins a dictionary no frame
/// names would satisfy "the file carries the dictionary" while every record stayed
/// unprimed.
fn assert_primed(store: &[u8], dictionary: &str, frames: usize) {
    let (items, torn) = iter_items(store);
    assert!(torn.is_none(), "the store is a torn CBOR sequence");
    let mut primed_ids: BTreeSet<i128> = BTreeSet::new();
    let mut pinned_anywhere = false;
    let mut primed_frames = 0usize;
    for (_, item) in &items {
        if is_segment_header(item) {
            // A new segment resets the catalog; only the ids of a header that pins
            // this dictionary count as primed.
            let head = unwrap_header(item).expect("a segment header unwraps");
            primed_ids = catalog_dictionaries(head)
                .into_iter()
                .filter(|(_, name)| name == dictionary)
                .map(|(id, _)| id)
                .collect();
            pinned_anywhere |= !primed_ids.is_empty();
            continue;
        }
        let Value::Map(entries) = item else { continue };
        if map_get(entries, "d").is_none() {
            continue;
        }
        if let Some(Value::Array(chain)) = map_get(entries, "x")
            && let Some(Value::Integer(id)) = chain.first()
            && primed_ids.contains(&i128::from(*id))
        {
            primed_frames += 1;
        }
    }
    assert!(
        pinned_anywhere,
        "no segment header pins {dictionary:?} in its \"dct\" map"
    );
    assert!(
        primed_frames >= frames,
        "expected at least {frames} payload frame(s) primed with {dictionary:?}; found \
         {primed_frames} — a pinned-but-unused dictionary is dead weight, not priming"
    );
}

/// A minimal `logic:` candidate: a ground atom the KB below entails, so the
/// conjecture is corroborated and the library append actually happens.
const CONJECTURE_FORMULA: &str = concat!(
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
    "@prefix ex: <https://example.org/> .\n",
    "ex:cand a logic:Formula ; logic:relation ex:p ;\n",
    "    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .\n",
);

/// The KB the candidate is tested against: it asserts the atom, so the isolated
/// world entails it.
const CONJECTURE_KB: &str = "@prefix ex: <https://example.org/> .\nex:a ex:p ex:a .\n";
