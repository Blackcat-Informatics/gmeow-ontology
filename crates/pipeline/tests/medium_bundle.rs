// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The whole-bundle MEDIUM gate: the real DAG runs once, in memory, and the bundle
//! it emits is audited against the medium axis `slices/core/gts/module.ttl` declares.
//!
//! Every assertion here is about the SHIPPED artifact rather than about a component:
//! the five declared dictionaries are pinned in the segment header a consumer
//! actually reads, one `gmeow:MediumEnvelope` describes each payload-bearing frame
//! the pack actually carries, each declared dictionary primes a NON-EMPTY set of those
//! frames, and the self-referential snapshot envelope's stratified digest is recomputed
//! FROM the emitted bytes rather than trusted. A unit test over the sealing code could
//! pass with none of that true.
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

/// The five dictionaries `slices/core/gts/module.ttl` declares. Spelled out rather
/// than read back off the same registry the producer used, so a dictionary silently
/// dropped from the declaration is a FAILURE here instead of a smaller expectation.
///
/// FIVE, not eight. Three slice-shaped drafts were retired by MEASUREMENT against the
/// bundle's frame layout — see [`RETIRED_DICTIONARIES`].
const SHIPPED_DICTIONARIES: [&str; 5] = [
    "gmeow-core-v1",
    "gmeow-logic-v1",
    "gmeow-memory-compact-v1",
    "gmeow-memory-hot-v1",
    "gmeow-prooftrace-v1",
];

/// The dictionary ids the bundle once declared and RETIRED, spelled out as wire labels.
///
/// They are named rather than merely deleted because the failure mode retirement can
/// leave behind is silent: a `gmeow:PayloadSchema` still selecting a retired id, or a
/// segment header still pinning one, would be an artifact primed by bytes the bundle no
/// longer trains. [`no_rep_is_primed_by_a_retired_dictionary`] asserts the absence in
/// every direction a retired id could survive in.
///
/// Each lost the SAME criterion — a `gmeow:CompressionDictionary` is justified by the
/// FRAME SET it primes and must pay for its own in-band bytes on that set — in a
/// different way: `gmeow-math-v1` primed zero frames (the mathematical graphs are
/// unioned into the snapshot payload, one frame already primed in full by
/// `gmeow-core-v1`); `gmeow-claims-v1` primed one ~9 KB frame whose best grid cell coded
/// 12,020 B against an 8,953 B no-dictionary baseline; `gmeow-lang-ast-v1` lost by
/// 3,684 B over three frames. All of their reps are now primed by `gmeow-core-v1`, so no
/// frame lost compression and nothing is orphaned.
const RETIRED_DICTIONARIES: [&str; 3] = ["gmeow-claims-v1", "gmeow-lang-ast-v1", "gmeow-math-v1"];

/// The archive rep the `lang:` projection deliverables ride, as a WIRE label (the test
/// may not borrow the crate-private Rust constant — the point is that the two agree).
const REP_LANG_PROJECTIONS: &str = "lang-projections-archive";

/// The document-scale English surface rep. It and [`REP_LANG_PROJECTIONS`] were the
/// entire population of the retired `gmeow-lang-ast-v1`; both now ride `gmeow-core-v1`.
const REP_LANG_SURFACE: &str = "lang-surface-blob";

/// The archive rep the CLAIM CORPUS's JSON-LD-family projections ride, as a WIRE label
/// (same reason as [`REP_LANG_PROJECTIONS`]: the point is that the crate-private Rust
/// constant and the shipped bytes agree).
const REP_YAMLLD: &str = "yaml-ld-archive";

/// The two members [`REP_YAMLLD`] carries, as wire labels.
const YAMLLD_MEMBERS: [&str; 2] = ["gmeow.rdf12.jsonld", "gmeow.rdf12.yamlld"];

/// The INTERNAL dataflow prefix the [`REP_YAMLLD`] members ride from `stage-statements`
/// into the fold. No archive may carry it: it names no committed file, so a member under
/// it would be an orphan in the superset reverse sweep as well as a double carry.
const INTERNAL_LANE_PREFIX: &str = "pipeline/";

/// The committed prefix every [`REP_LANG_PROJECTIONS`] member reconstructs under.
const LANG_PROJECTION_PREFIX: &str = "generated/projections/lang/";

/// The two dictionaries whose frames a CONSUMER writes into its own runtime store out of
/// the shipped header, rather than this emission writing them into the bundle. They are
/// bound by a `gmeow:mediumSourceHeaderDict` medium and named by no bundle rep, which is
/// the legitimate second home the registry-level totality check recognizes.
const RUNTIME_STORE_DICTIONARIES: [&str; 2] = ["gmeow-memory-compact-v1", "gmeow-memory-hot-v1"];

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

    // ── (a) the header pins the five declared dictionaries, in band ──
    let pinned = gmeow_gts_profile::segment_dictionaries(&bundle)
        .expect("the emitted bundle's header reads back");
    let names: Vec<&str> = pinned.keys().map(String::as_str).collect();
    assert_eq!(
        names, SHIPPED_DICTIONARIES,
        "the pack's in-band \"dct\" map must pin exactly the five declared dictionaries"
    );
    for (name, bytes) in &pinned {
        assert!(
            !bytes.is_empty(),
            "dictionary {name:?} is pinned with no bytes"
        );
    }

    // ── (b) graph/medium-registry: five realizations + one envelope per frame ──
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

    // ── the dictionary-EFFECT measurement, on the shipped artifact ──
    the_shipped_bundle_proves_every_dictionary_pays_for_itself(&bundle, &module);
    the_runtime_store_dictionary_pays_for_itself(&bundle, &pinned);

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
    // …and specifically the MEASUREMENT artifact, named rather than left implied by the
    // whole-bundle digest above: a measurement whose numbers moved between two passes
    // over one carrier would drift the committed generated/ tree on every build.
    let effect_of = |bytes: &[u8]| -> Vec<u8> {
        gmeow_pipeline::stages::superset::project_bundle(bytes)
            .expect("the bundle projects")
            .files
            .get(gmeow_pipeline::medium::measure::MEDIUM_EFFECT_PATH)
            .unwrap_or_else(|| {
                panic!(
                    "the bundle projects no {}",
                    gmeow_pipeline::medium::measure::MEDIUM_EFFECT_PATH
                )
            })
            .clone()
    };
    let first_effect = effect_of(&bundle);
    assert!(
        !first_effect.is_empty(),
        "the dictionary-effect projection is empty"
    );
    assert_eq!(
        effect_of(&again),
        first_effect,
        "the dictionary-effect measurement must be byte-identical across two emissions over the \
         same carrier — it is a committed generated/ artifact under the strict sync drift gate"
    );

    // ── (g) the dictionaries project onto generated/medium/*.zdict, EXACTLY ONCE ──
    the_dictionaries_project_exactly_once(&bundle, &pinned, &registry_quads, &module);

    // ── (h) the three REASSIGNED reps are real, emitted frames primed by
    //        gmeow-core-v1 — retiring a dictionary must not quietly stop compressing
    //        the frames it used to prime — and NO rep, header entry or projected file
    //        is primed by a retired dictionary id ──
    the_lang_reps_are_real_frames_primed_by_core(&bundle, &module);
    the_claim_corpus_archive_is_a_real_frame_primed_by_core(&bundle, &module);
    the_split_out_archive_members_are_carried_by_exactly_one_rep(&bundle);
    no_rep_is_primed_by_a_retired_dictionary(&bundle, &module, &pinned);

    // ── (e)/(f) the runtime stores, primed from THIS bundle ──
    let runtime = runtime_stores_are_primed_from(&bundle);

    // ── (i) the generalization both (h) clauses are instances of ──
    every_declared_dictionary_primes_an_emitted_frame(&bundle, &module, &pinned, &runtime);
}

/// The generated-opaque archive representation label. Spelled out because it is a WIRE
/// label a consumer reads off the bundle, not a Rust symbol the test may borrow — and
/// the whole point of the assertion below is that the gate's own crate-private constant
/// and the shipped bytes agree.
const REP_GENERATED: &str = "generated-opaque-archive";

/// The fourth fanout family, proved on the SHIPPED artifact: each of the five
/// dictionaries reconstructs from the segment header's in-band `"dct"` map onto
/// `generated/medium/<dict-id>.zdict`, its bytes are byte-equal to both the header entry
/// and the recorded `gmeow:dictionaryContentDigest`, and NO generated-opaque archive
/// member carries the same bytes a second time.
///
/// The last clause is the load-bearing one. Routing a `.zdict` through the archive as
/// well would satisfy every other assertion here while shipping the same high-entropy
/// bytes twice — re-folding a blob the snapshot already carries (Constitution §18) and
/// inflating the archive it rode in.
fn the_dictionaries_project_exactly_once(
    bundle: &[u8],
    pinned: &BTreeMap<String, Vec<u8>>,
    registry_quads: &[RdfQuad],
    module: &MediumRegistry,
) {
    let projection = gmeow_pipeline::stages::superset::project_bundle(bundle)
        .expect("the shipped bundle projects (header-dict bijection + expected completeness)");

    // The realization records, keyed by the dictionary id they realize.
    let digest_by_id: BTreeMap<String, String> = subjects_of_type(
        registry_quads,
        &format!("{GMEOW}CompressionDictionaryRealization"),
    )
    .iter()
    .map(|subject| {
        let definition = iri_of(registry_quads, subject, "realizesDictionary");
        let id = module
            .dictionaries()
            .get(&definition)
            .unwrap_or_else(|| panic!("<{definition}> is not a declared dictionary"))
            .id
            .clone();
        (
            id,
            literal_of(registry_quads, subject, "dictionaryContentDigest"),
        )
    })
    .collect();

    for id in SHIPPED_DICTIONARIES {
        let path = format!("generated/medium/{id}.zdict");
        let projected = projection
            .files
            .get(&path)
            .unwrap_or_else(|| panic!("the bundle projects no {path}"));
        let in_band = pinned
            .get(id)
            .unwrap_or_else(|| panic!("the header pins no {id:?} dictionary"));
        assert_eq!(
            projected, in_band,
            "{path} must be the header \"dct\" entry byte for byte"
        );
        assert_eq!(
            digest_by_id.get(id).map(String::as_str),
            Some(blake3(projected).as_str()),
            "{path} must match the gmeow:dictionaryContentDigest its realization records"
        );
    }
    // EXACTLY ONE `.zdict` per declared dictionary — no more (a stale projection for a
    // RETIRED dictionary would land here) and no fewer.
    let projected_dicts: BTreeSet<&String> = projection
        .files
        .keys()
        .filter(|p| p.starts_with("generated/medium/") && p.ends_with(".zdict"))
        .collect();
    assert_eq!(
        projected_dicts.len(),
        SHIPPED_DICTIONARIES.len(),
        "the projection carries exactly one generated/medium/*.zdict per declared dictionary; \
         got {projected_dicts:?}"
    );
    // …and the rest of the `generated/medium/` family is exactly the ONE measurement
    // projection, named rather than tolerated: it is RDF and travels as RDF (the
    // `rdf-fanout` family), which is why the header-dict family keys on the `.zdict`
    // suffix. An unnamed extra member here would be an unaccounted-for reconstruction.
    let non_dicts: BTreeSet<&String> = projection
        .files
        .keys()
        .filter(|p| p.starts_with("generated/medium/") && !p.ends_with(".zdict"))
        .collect();
    assert_eq!(
        non_dicts,
        [&gmeow_pipeline::medium::measure::MEDIUM_EFFECT_PATH.to_string()]
            .into_iter()
            .collect::<BTreeSet<&String>>(),
        "the only non-dictionary member of the generated/medium/ family is the measurement \
         projection"
    );

    // EXACTLY ONCE: no generated-opaque archive member is a dictionary.
    let graph = purrdf::gts::read_graph(bundle, true).expect("the bundle's blob lane reads");
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);
    let mut archives_seen = 0usize;
    for record in &lookaside.blobs {
        if record.representation.as_deref() != Some(REP_GENERATED) {
            continue;
        }
        archives_seen += 1;
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == &record.digest) else {
            panic!("the {REP_GENERATED} lookaside record names no inline blob");
        };
        let bytes = entry.decoded_vec().expect("the archive decodes");
        for (name, _) in purrdf::ustar::read_archive(&bytes).expect("the archive unpacks") {
            assert!(
                !name.starts_with("generated/medium/") && !name.ends_with(".zdict"),
                "the {REP_GENERATED} archive carries {name} — a trained dictionary's ONE home \
                 is the segment header's \"dct\" map, so an archive copy would ship the same \
                 high-entropy bytes twice"
            );
        }
    }
    assert_eq!(
        archives_seen, 1,
        "the shipped bundle carries exactly one {REP_GENERATED} archive, or the clause above \
         is vacuous"
    );
}

/// The blob-representation labels the emitted pack actually carries frames for. Read off
/// the lookaside alone — no payload is decoded, because the question is only WHICH reps
/// were emitted.
fn emitted_reps(bundle: &[u8]) -> BTreeSet<String> {
    let graph = purrdf::gts::read_graph(bundle, true).expect("the bundle's blob lane reads");
    purrdf::gts::lookaside_from_graph(&graph)
        .blobs
        .iter()
        .filter_map(|record| record.representation.clone())
        .collect()
}

/// `(frame count, total decoded payload bytes)` for ONE representation — a rep may back
/// several frames (`lang-surface-blob` backs one per document-scale literal).
///
/// Scoped to a single rep on purpose: the big `ontology-docs` / `okf-export` payloads trip
/// the zstd decode safety bound, so a sweep that decoded every blob would be fatal as well
/// as wasteful (the same reason `carrier::archive_rep_carries_generated` excludes them).
fn decoded_frames_for_rep(bundle: &[u8], rep: &str) -> (usize, usize) {
    let graph = purrdf::gts::read_graph(bundle, true).expect("the bundle's blob lane reads");
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);
    let mut frames = 0usize;
    let mut bytes = 0usize;
    for record in &lookaside.blobs {
        if record.representation.as_deref() != Some(rep) {
            continue;
        }
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == &record.digest) else {
            panic!("the {rep:?} lookaside record names no inline blob");
        };
        frames += 1;
        bytes += entry
            .decoded_vec()
            .unwrap_or_else(|err| panic!("the {rep:?} blob decodes: {err:?}"))
            .len();
    }
    (frames, bytes)
}

/// The reps a dictionary primes, read off the SHIPPED bundle's own payload-schema
/// registry (never a repo re-parse), keyed by `gmeow:dictionaryId`.
fn primed_reps_by_dictionary(module: &MediumRegistry) -> BTreeMap<String, BTreeSet<String>> {
    use gmeow_pipeline::medium::registry::DictSelection;
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for schema in module.schemas().values() {
        let row = module
            .assignment_for(&schema.rep)
            .unwrap_or_else(|err| panic!("rep {:?} is unassigned: {err}", schema.rep));
        if let DictSelection::Named(iri) = &row.dictionary {
            let def = module
                .dictionaries()
                .get(iri)
                .unwrap_or_else(|| panic!("rep {:?} selects unregistered <{iri}>", schema.rep));
            out.entry(def.id.clone())
                .or_default()
                .insert(schema.rep.clone());
        }
    }
    out
}

/// The two `lang:` reps are REAL, emitted frames, and both are primed by
/// `gmeow-core-v1` after `gmeow-lang-ast-v1`'s retirement.
///
/// This is the clause that proves, on the SHIPPED artifact, that retiring a dictionary
/// did not quietly stop compressing the frames it used to prime. It is quantitative on
/// purpose: `lang-surface-blob` alone is a ~12 KB population of `@x-gmeow-english`
/// literals over the document-scale threshold, while the ~150 KB of grammar / CoNLL-U /
/// TEI / GMN1 bytes ride `lang-projections-archive`. Asserting only "the reps are primed"
/// would pass with the projection archive folded back into the general opaque archive, so
/// what is asserted is that the projection frame exists, dominates the surface blobs, and
/// is plausibly complete.
///
/// The dictionary that used to prime them was measured over exactly this population and
/// LOST by 3,684 B net of its own in-band bytes — a real but far too small saving. Both
/// reps therefore select `gmeow:dictGmeowCoreV1`, which wins over the widened population
/// that includes them, so every one of these bytes is still dictionary-compressed.
///
/// The generalization — EVERY declared dictionary primes an emitted frame — is enforced
/// in [`every_declared_dictionary_primes_an_emitted_frame`]; this clause is the
/// QUANTITATIVE one that the generalization cannot make (it would pass on a one-literal
/// population). The registry-level half is enforced a third time, against the declaration
/// alone, in `medium::registry::tests::the_live_gts_slice_reads_as_a_complete_registry`.
fn the_lang_reps_are_real_frames_primed_by_core(bundle: &[u8], module: &MediumRegistry) {
    let emitted_reps = emitted_reps(bundle);
    let primed = primed_reps_by_dictionary(module);

    let core_reps = primed
        .get("gmeow-core-v1")
        .expect("gmeow-core-v1 primes at least one rep");
    assert!(
        core_reps.contains(REP_LANG_PROJECTIONS) && core_reps.contains(REP_LANG_SURFACE),
        "both lang: reps must be primed by gmeow-core-v1 after gmeow-lang-ast-v1's \
         retirement — a retired dictionary must never leave its frames unprimed; \
         got {core_reps:?}"
    );
    assert!(
        emitted_reps.contains(REP_LANG_PROJECTIONS),
        "the bundle emits no {REP_LANG_PROJECTIONS} frame"
    );
    let (surface_frames, surface_bytes) = decoded_frames_for_rep(bundle, REP_LANG_SURFACE);
    let (projection_frames, projection_bytes) =
        decoded_frames_for_rep(bundle, REP_LANG_PROJECTIONS);
    assert!(
        surface_frames > 0,
        "the surface-blob half must be non-empty too, or the comparison below is vacuous"
    );
    // The measured population, printed so a future reader sees it rather than only the
    // inequality that guards it.
    println!(
        "lang: population (primed by gmeow-core-v1): {surface_frames} surface frame(s) / \
         {surface_bytes} B + {projection_frames} projection frame(s) / {projection_bytes} B"
    );
    assert_eq!(
        projection_frames, 1,
        "the lang projections ride ONE tar frame"
    );
    // A regression that folded the projections back into the generated-opaque archive
    // drops `projection_bytes` to zero and reds here.
    assert!(
        projection_bytes > 5 * surface_bytes.max(1),
        "the lang: population must be dominated by the projection archive, not by the \
         document-scale literals: {projection_frames} projection frame(s) / \
         {projection_bytes} B vs {surface_frames} surface frame(s) / {surface_bytes} B"
    );
    assert!(
        projection_bytes > 100_000,
        "the {REP_LANG_PROJECTIONS} archive is implausibly small ({projection_bytes} B) — the \
         committed generated/projections/lang/ tree is ~150 KB, so a smaller archive means \
         members were dropped"
    );
}

/// The claim corpus's JSON-LD-family frame is REAL and emitted, and it is primed by
/// `gmeow-core-v1` after `gmeow-claims-v1`'s retirement.
///
/// The rep shipped for a long time with no live producer at all: its writer was a
/// `#[cfg(test)]` twin of the sink's folds, so the production terminal authored no such
/// frame and the dictionary selecting it primed nothing. Building the frame fixed that —
/// and then MEASURING the frame retired the dictionary anyway, for the other half of the
/// same criterion: one ~9 KB frame is too small a population for any grid cell to pay for
/// a dictionary's own in-band bytes (best cell 12,020 B vs an 8,953 B no-dictionary
/// baseline). The frame stays, and stays dictionary-compressed, on `gmeow-core-v1`.
///
/// So the assertions are about the frame EXISTING with the claim corpus in it: the archive
/// is emitted, it carries exactly the two declared members, its decoded population is the
/// statement layer rather than a placeholder, and the dictionary priming it is one the
/// bundle still ships.
fn the_claim_corpus_archive_is_a_real_frame_primed_by_core(bundle: &[u8], module: &MediumRegistry) {
    let primed = primed_reps_by_dictionary(module);
    let core_reps = primed
        .get("gmeow-core-v1")
        .expect("gmeow-core-v1 primes at least one rep");
    assert!(
        core_reps.contains(REP_YAMLLD),
        "the claim-corpus archive must be primed by gmeow-core-v1 after gmeow-claims-v1's \
         retirement; got {core_reps:?}"
    );
    assert!(
        emitted_reps(bundle).contains(REP_YAMLLD),
        "the bundle emits no {REP_YAMLLD} frame — the rep would be registered and primed \
         while no payload cites it"
    );

    let (frames, bytes) = decoded_frames_for_rep(bundle, REP_YAMLLD);
    assert_eq!(frames, 1, "the claim serializations ride ONE tar frame");
    println!("{REP_YAMLLD} population (primed by gmeow-core-v1): {frames} frame(s) / {bytes} B");

    let members = archive_members(bundle, REP_YAMLLD);
    assert_eq!(
        members.keys().map(String::as_str).collect::<Vec<_>>(),
        YAMLLD_MEMBERS.to_vec(),
        "the claim archive carries exactly its two declared members"
    );
    // NON-VACUITY, on the content rather than the count: the members must be the reified
    // statement layer in JSON-LD-family syntax. An empty render, a placeholder, or a
    // whole-carrier serialization each fail one of these.
    for (name, payload) in &members {
        let text = std::str::from_utf8(payload)
            .unwrap_or_else(|err| panic!("the {name} member is UTF-8: {err}"));
        assert!(
            text.contains("@annotation"),
            "the {name} member carries no RDF-1.2 reification coat — it is not the claim \
             corpus"
        );
        assert!(
            text.contains("examples/claim-crimea-in-russia-per-ru"),
            "the {name} member is missing a statement-layer claim token the DSL authors"
        );
    }
    // The committed RDF 1.2 lead is ~100 KB of Turtle; its JSON-LD-family renderings
    // expand every prefixed name to a full IRI, so a materially smaller archive means
    // members were dropped rather than that the corpus shrank.
    assert!(
        bytes > 100_000,
        "the {REP_YAMLLD} archive is implausibly small ({bytes} B) for the ~100 KB \
         statement layer it projects"
    );
}

/// NO artifact of the shipped bundle is primed by a RETIRED dictionary id, in any of the
/// four directions a retired id could survive in.
///
/// Retiring a dictionary is the one operation in this axis that can leave something
/// ORPHANED — an artifact primed with bytes the bundle no longer trains, ships or
/// projects. The declaration, the header, the projection and the envelope stratum each
/// carry the id independently, so each is checked independently rather than inferred from
/// the registry alone:
///
/// * no `gmeow:CompressionDictionary` is DECLARED with a retired id;
/// * no registered `gmeow:PayloadSchema` SELECTS one (which is what "no rep is primed by a
///   retired id" means at the declaration);
/// * the segment header's in-band `"dct"` map PINS none, so a consumer cannot even obtain
///   one;
/// * the header-dict fanout projects no `generated/medium/<retired-id>.zdict`.
fn no_rep_is_primed_by_a_retired_dictionary(
    bundle: &[u8],
    module: &MediumRegistry,
    pinned: &BTreeMap<String, Vec<u8>>,
) {
    let declared: BTreeSet<&str> = module
        .dictionaries()
        .values()
        .map(|def| def.id.as_str())
        .collect();
    let primed = primed_reps_by_dictionary(module);
    let projection = gmeow_pipeline::stages::superset::project_bundle(bundle)
        .expect("the shipped bundle projects");

    for id in RETIRED_DICTIONARIES {
        assert!(
            !declared.contains(id),
            "{id} is retired but still declared as a gmeow:CompressionDictionary"
        );
        assert!(
            !primed.contains_key(id),
            "a registered gmeow:PayloadSchema still selects the RETIRED dictionary {id} — the \
             frames it names would be primed with bytes this bundle no longer trains, ships or \
             projects. Repoint the schema at a shipped dictionary; do NOT weaken this"
        );
        assert!(
            !pinned.contains_key(id),
            "the segment header still pins the RETIRED dictionary {id} in its in-band \"dct\" \
             map — dead high-entropy bytes every consumer downloads"
        );
        assert!(
            !projection
                .files
                .contains_key(&format!("generated/medium/{id}.zdict")),
            "the header-dict fanout still projects generated/medium/{id}.zdict for a retired \
             dictionary"
        );
    }

    // NON-VACUITY: the reps the retired dictionaries used to prime are still primed —
    // by a dictionary the bundle SHIPS. Retirement removed dictionaries, never
    // compression.
    for rep in [REP_YAMLLD, REP_LANG_PROJECTIONS, REP_LANG_SURFACE] {
        let assignment = module
            .assignment_for(rep)
            .unwrap_or_else(|err| panic!("rep {rep:?} has no medium assignment: {err}"));
        let gmeow_pipeline::medium::registry::DictSelection::Named(iri) = &assignment.dictionary
        else {
            panic!(
                "rep {rep:?} fell back to the dictionary-less baseline medium after its \
                 dictionary was retired — retiring a dictionary must reassign its frames, not \
                 stop compressing them"
            );
        };
        let def = module
            .dictionaries()
            .get(iri)
            .unwrap_or_else(|| panic!("rep {rep:?} selects unregistered <{iri}>"));
        assert!(
            SHIPPED_DICTIONARIES.contains(&def.id.as_str()),
            "rep {rep:?} is primed by {}, which is not a shipped dictionary",
            def.id
        );
    }
}

/// EVERY declared dictionary primes at least one EMITTED frame — the invariant every
/// retirement in [`RETIRED_DICTIONARIES`] was an instance of, stated once so the next one
/// is caught by the gate instead of by a person measuring.
///
/// A dictionary has exactly two ways to satisfy it, matching the two legitimate homes the
/// registry-level check recognizes:
///
/// * a registered `gmeow:PayloadSchema` selects it AND the bundle emits that rep, so a
///   frame of THIS emission is primed with it; or
/// * it is a runtime-store dictionary ([`RUNTIME_STORE_DICTIONARIES`]) whose frames a
///   CONSUMER writes, in which case the frames it primes are in the runtime artifact —
///   and the bytes doing the priming must be the ones the bundle SHIPPED, or the shipped
///   copy is still dead weight and the runtime merely happens to use the same id.
///
/// # The one remaining exception, named rather than allowlisted
///
/// `gmeow-memory-compact-v1` fails the second form: [`gmeow_pipeline::mcp::compact_store`]
/// passes `purrdf::gts::compact::DictStrategy::RawContent`, so purrdf BUILDS a dictionary
/// from the pack's own content blobs and labels it with that id — the compacted pack is
/// genuinely primed, but with bytes it derived, not with the bytes this bundle trained,
/// measured, pinned and projected onto `generated/medium/gmeow-memory-compact-v1.zdict`.
/// Priming it with the shipped bytes needs a `DictStrategy::Pinned` (feed the dictionary
/// in rather than derive it) that the pinned purrdf does not expose; there is no
/// GMEOW-side edit that closes it. It is pinned as a NEGATIVE below rather than skipped,
/// so the day purrdf gains that variant and the lane is wired, this assertion reds and
/// whoever wires it must delete the exception instead of leaving it to rot.
fn every_declared_dictionary_primes_an_emitted_frame(
    bundle: &[u8],
    module: &MediumRegistry,
    pinned: &BTreeMap<String, Vec<u8>>,
    runtime: &RuntimePriming,
) {
    let emitted = emitted_reps(bundle);
    let primed = primed_reps_by_dictionary(module);

    for id in SHIPPED_DICTIONARIES {
        if RUNTIME_STORE_DICTIONARIES.contains(&id) {
            assert!(
                !primed.contains_key(id),
                "{id} is a runtime-store dictionary, so no BUNDLE rep may select it — if one \
                 now does, it belongs in the first form of this invariant, not the second"
            );
            continue;
        }
        let reps = primed.get(id).unwrap_or_else(|| {
            panic!(
                "{id} primes no registered gmeow:PayloadSchema — it would be trained, \
                 measured, pinned and projected while no frame cites it. Assign it to a rep \
                 or retire it; do NOT weaken this assertion"
            )
        });
        let live: BTreeSet<&String> = reps.iter().filter(|rep| emitted.contains(*rep)).collect();
        assert!(
            !live.is_empty(),
            "{id} primes {reps:?}, none of which the shipped bundle actually emits — a \
             declared-but-unwritten rep is the `yaml-ld-archive` state this invariant was \
             tightened to catch. Emitted: {emitted:?}"
        );
    }

    // The runtime half: `gmeow-memory-hot-v1` primes a consumer's store with the bundle's
    // OWN header entry, which is the whole reason the bundle is that dictionary's
    // distribution channel.
    let shipped_hot = pinned
        .get("gmeow-memory-hot-v1")
        .expect("the header pins gmeow-memory-hot-v1");
    assert_eq!(
        &runtime.hot, shipped_hot,
        "the runtime store must be primed with the SHIPPED gmeow-memory-hot-v1 bytes, not \
         with a re-derivation that merely reuses the id"
    );

    // The exception, pinned as a negative (see this function's docs).
    let shipped_compact = pinned
        .get("gmeow-memory-compact-v1")
        .expect("the header pins gmeow-memory-compact-v1");
    assert_ne!(
        &runtime.compact, shipped_compact,
        "the compaction lane now primes with the SHIPPED gmeow-memory-compact-v1 bytes — the \
         upstream DictStrategy::Pinned gap this exception documents is CLOSED. Fold \
         gmeow-memory-compact-v1 into the runtime clause above and delete this assertion"
    );
}

/// The members of ONE tar archive rep of the emitted bundle, by member name.
fn archive_members(bundle: &[u8], rep: &str) -> BTreeMap<String, Vec<u8>> {
    let graph = purrdf::gts::read_graph(bundle, true).expect("the bundle's blob lane reads");
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for record in &lookaside.blobs {
        if record.representation.as_deref() != Some(rep) {
            continue;
        }
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == &record.digest) else {
            panic!("the {rep:?} lookaside record names no inline blob");
        };
        let bytes = entry
            .decoded_vec()
            .unwrap_or_else(|err| panic!("the {rep:?} blob decodes: {err:?}"));
        for (name, member) in
            purrdf::ustar::read_archive(&bytes).expect("the {rep} archive unpacks")
        {
            assert!(
                out.insert(name.clone(), member).is_none(),
                "the {rep:?} archives carry {name} twice"
            );
        }
    }
    out
}

/// Every member of the two SPLIT-OUT archives is carried by EXACTLY ONE rep — every
/// `generated/projections/lang/**` path by [`REP_LANG_PROJECTIONS`], every
/// [`YAMLLD_MEMBERS`] entry by [`REP_YAMLLD`] — and no archive carries the INTERNAL lane
/// at all.
///
/// `project_bundle` already hard-fails on a path two representatives both carry, but
/// that is a NEGATIVE guard: it would stay silent if the members quietly rode the
/// generated-opaque archive alone (the pre-split state) and the dictionary that was
/// supposed to prime them primed nothing again. So the positive half is asserted here,
/// over the shipped tars. Both families are checked in ONE sweep because the sweep is
/// what costs — it decodes every non-oversized tar in the bundle.
fn the_split_out_archive_members_are_carried_by_exactly_one_rep(bundle: &[u8]) {
    let graph = purrdf::gts::read_graph(bundle, true).expect("the bundle's blob lane reads");
    let lookaside = purrdf::gts::lookaside_from_graph(&graph);

    // The documentation/export payloads are large enough to trip the zstd decode safety
    // bound (the same set `carrier::archive_rep_carries_generated` excludes), so they are
    // NOT decoded. The skip is DECLARED rather than silent: any other rep that failed to
    // decode would leave a blind spot in this bijection, so the assertion below pins the
    // skipped set instead of swallowing the error.
    const OVERSIZED: [&str; 4] = ["ontology-docs", "okf-export", "docs-book", "docs-print"];

    let mut carriers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut scanned: BTreeSet<String> = BTreeSet::new();
    let mut undecodable: BTreeSet<String> = BTreeSet::new();
    for record in &lookaside.blobs {
        if record.media_type.as_deref() != Some("application/x-tar") {
            continue;
        }
        let rep = record
            .representation
            .as_deref()
            .unwrap_or_default()
            .to_string();
        if OVERSIZED.contains(&rep.as_str()) {
            continue;
        }
        let Some((_, entry)) = graph.blobs.iter().find(|(d, _)| d == &record.digest) else {
            panic!("the {rep:?} lookaside record names no inline blob");
        };
        let Ok(bytes) = entry.decoded_vec() else {
            undecodable.insert(rep);
            continue;
        };
        let members = purrdf::ustar::read_archive(&bytes)
            .unwrap_or_else(|err| panic!("the {rep:?} archive unpacks: {err}"));
        scanned.insert(rep.clone());
        for (name, _) in members {
            // The INTERNAL lane is not a family with a rightful owner — it is a lane no
            // archive may carry at all, so it is checked here where every archive's
            // membership is already in hand rather than in a second sweep.
            assert!(
                !name.starts_with(INTERNAL_LANE_PREFIX),
                "the {rep:?} archive carries {name}, an INTERNAL dataflow artifact: it backs \
                 no committed file (an orphan for the superset reverse sweep) and reaching an \
                 archive at all means the same bytes rode two differently-primed frames"
            );
            if name.starts_with(LANG_PROJECTION_PREFIX) || YAMLLD_MEMBERS.contains(&name.as_str()) {
                carriers.entry(name).or_default().insert(rep.clone());
            }
        }
    }

    assert!(
        undecodable.is_empty(),
        "every tar archive outside the declared oversized set must decode, or this bijection \
         has an unexamined blind spot; undecodable: {undecodable:?}"
    );
    assert!(
        scanned.contains(REP_GENERATED)
            && scanned.contains(REP_LANG_PROJECTIONS)
            && scanned.contains(REP_YAMLLD),
        "the scan must cover the generated-opaque archive AND both split-out archives, or \
         'exactly one carrier' proves nothing; scanned {scanned:?}"
    );
    assert!(
        !carriers.is_empty(),
        "no archive carries a {LANG_PROJECTION_PREFIX}** member — the clause is vacuous"
    );
    let (claim_members, lang_members): (Vec<&String>, Vec<&String>) = carriers
        .keys()
        .partition(|name| YAMLLD_MEMBERS.contains(&name.as_str()));
    assert_eq!(
        lang_members.len(),
        35,
        "the lang-projection family membership drifted; got {lang_members:?}"
    );
    assert_eq!(
        claim_members.len(),
        YAMLLD_MEMBERS.len(),
        "the claim archive's member set drifted; got {claim_members:?}"
    );
    for (path, reps) in &carriers {
        let owner = if YAMLLD_MEMBERS.contains(&path.as_str()) {
            REP_YAMLLD
        } else {
            REP_LANG_PROJECTIONS
        };
        assert_eq!(
            reps.iter().map(String::as_str).collect::<Vec<_>>(),
            vec![owner],
            "{path} must be carried by exactly one rep, {owner}"
        );
    }
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
///
/// It RETURNS the dictionary bytes the two runtime artifacts actually pinned, because
/// "pinned an entry under that id" and "pinned the bytes this bundle shipped" are
/// different claims and only the second one makes the shipped copy load-bearing.
/// [`every_declared_dictionary_primes_an_emitted_frame`] is where they are compared.
fn runtime_stores_are_primed_from(bundle: &[u8]) -> RuntimePriming {
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

    let hot = gmeow_gts_profile::segment_dictionaries(&store_bytes)
        .expect("the primed store reads back")
        .remove(MEMORY_HOT_DICTIONARY)
        .expect("the primed store pins the hot dictionary");
    let compact = pinned
        .get(MEMORY_COMPACT_DICTIONARY)
        .expect("checked just above")
        .clone();
    RuntimePriming { hot, compact }
}

/// The dictionary bytes the RUNTIME artifacts pinned, returned by
/// [`runtime_stores_are_primed_from`] for the shipped-bytes comparison in
/// [`every_declared_dictionary_primes_an_emitted_frame`].
struct RuntimePriming {
    /// What the freshly written memory store pinned under `gmeow-memory-hot-v1`.
    hot: Vec<u8>,
    /// What the compaction lane pinned under `gmeow-memory-compact-v1`.
    compact: Vec<u8>,
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

/// The medium MEASUREMENT graph, proved on the SHIPPED artifact: every dictionary the
/// bundle primes publishes a reading, and every reading says the dictionary paid for
/// itself.
///
/// This is the clause that makes the medium axis's central claim checkable from the
/// artifact alone. Everything else about a dictionary — that it is declared, trained,
/// pinned, projected and cited by a frame — is true of a dictionary that costs more
/// than it saves. Only the two-part code separates the two, and only if the
/// dictionary's own in-band bytes are on the paying side.
fn the_shipped_bundle_proves_every_dictionary_pays_for_itself(
    bundle: &[u8],
    module: &MediumRegistry,
) {
    use gmeow_pipeline::medium::measure::{self, Population};

    let folded = purrdf::import_gts_events(bundle).expect("the emitted bundle folds back");
    let payload: Vec<RdfQuad> = purrdf::flat_rdf_quads_from_dataset(folded.dataset.as_ref());
    let measurement_graph = Some(RdfTerm::iri(
        gmeow_pipeline::medium::MEDIUM_MEASUREMENT_GRAPH,
    ));
    let quads: Vec<RdfQuad> = payload
        .iter()
        .filter(|quad| quad.graph_name == measurement_graph)
        .cloned()
        .collect();
    assert!(
        !quads.is_empty(),
        "the shipped bundle must carry graph/medium-measurement — a bundle that ships \
         dictionaries without publishing what they bought is asking to be trusted"
    );
    // The registry graph and the measurement graph are SEPARATE: the first says what the
    // dictionaries are, the second what they do, and folding them would tie two different
    // refresh cadences together.
    assert!(
        payload
            .iter()
            .any(|q| q.graph_name == Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH))),
        "the registry graph must still be its own graph"
    );

    let readings = subjects_of_type(&quads, &format!("{GMEOW}MediumDictionaryEffectMeasurement"));
    let required = measure::required_measurements(module);
    assert!(
        !required.is_empty(),
        "the derived required set must be non-empty, or the clause below is vacuous"
    );
    assert_eq!(
        readings.len(),
        required.len(),
        "one reading per dictionary the bundle primes; required {required:?}, got {readings:?}"
    );

    let mut measured: BTreeSet<String> = BTreeSet::new();
    for subject in &readings {
        // Typed as a gmeow:Measurement too, so the observation stack's own exactly-one
        // gmeow:observationMethod obligation applies to it rather than to a private class.
        assert!(
            quads
                .iter()
                .any(|q| q.subject == RdfTerm::iri(subject.as_str())
                    && q.predicate == RDF_TYPE
                    && q.object == RdfTerm::iri(format!("{GMEOW}Measurement"))),
            "<{subject}> must be a gmeow:Measurement"
        );
        let definition = iri_of(&quads, subject, "measuresDictionary");
        let def = module
            .dictionaries()
            .get(&definition)
            .unwrap_or_else(|| panic!("<{definition}> is not a declared dictionary"));
        assert_eq!(
            iri_of(&quads, subject, "observedFeature"),
            definition,
            "the generic observation role and the domain-specific one must name one individual"
        );
        assert_eq!(
            iri_of(&quads, subject, "observationMethod"),
            measure::METHOD_COMPUTATIONAL_MODEL
        );
        // A bundle publishes readings only over bytes IT WROTE.
        assert_eq!(
            iri_of(&quads, subject, "measurementPopulation"),
            Population::EmittedBlobFrames.iri(),
            "the shipped bundle may only publish the population it authored"
        );

        let count = |predicate: &str| -> u64 {
            literal_of(&quads, subject, predicate)
                .parse()
                .unwrap_or_else(|_| panic!("<{subject}> gmeow:{predicate} is not an integer"))
        };
        let on_disk = count("measurementBytesOnDisk");
        let in_band = count("measurementDictionaryInBandBytes");
        let two_part = count("measurementTwoPartCodeBytes");
        let baseline = count("measurementBytesOnDiskBaseline");
        assert_eq!(
            two_part,
            on_disk + in_band,
            "{}: the two-part code must be the sum of its published components",
            def.id
        );
        assert!(
            in_band > 0,
            "{}: a reading charging ZERO in-band bytes would be vacuous — the dictionary's own \
             bytes are the term the criterion turns on",
            def.id
        );
        // (c) the dictionary WINS on the population it primes, net of its own bytes.
        assert!(
            two_part < baseline,
            "{}: two-part code {two_part} B (= {on_disk} + {in_band}) is not strictly less than \
             the gmeow:mediumProfileBaselineL12 code {baseline} B — the shipped dictionary does \
             not pay for itself",
            def.id
        );
        assert!(
            count("measurementEvaluatedFrameCount") > 0,
            "{}: a reading over zero frames prices nothing",
            def.id
        );
        assert!(
            count("measurementCorpusSampleCount") > 0,
            "{}: a dictionary trained over zero samples could not have been built",
            def.id
        );
        // The bounded gain fraction rides a math:Quantity and stays inside [0, 1].
        let quantity = iri_of(&quads, subject, "observationResult");
        let gain: f64 = quads
            .iter()
            .find(|q| {
                q.subject == RdfTerm::iri(quantity.as_str())
                    && q.predicate == "https://blackcatinformatics.ca/math/quantityValue"
            })
            .and_then(|q| match &q.object {
                RdfTerm::Literal(literal) => literal.lexical_form.parse().ok(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("<{quantity}> carries no math:quantityValue"));
        assert!(
            (0.0..=1.0).contains(&gain),
            "{}: the gain fraction must be BOUNDED in [0, 1]; got {gain}",
            def.id
        );
        measured.insert(def.id.clone());
    }
    assert_eq!(
        measured, required,
        "the published readings must be exactly the dictionaries the bundle primes"
    );

    // (d) the chain the numbers were taken on is the MANDATED one, read off the shipped
    // registry rather than trusted: a plain-zstd proxy would have reported bytes this
    // build never writes.
    for medium in module.media().values() {
        assert_eq!(
            medium.codec_wire_name().unwrap_or_else(|err| panic!(
                "<{}> declares an unspellable codec: {err}",
                medium.iri
            )),
            "zstd-rsyncable",
            "<{}> must declare the mandated codec",
            medium.iri
        );
        assert_eq!(medium.zstd_level, 12);
    }
}

/// `gmeow-memory-hot-v1` pays for itself too — measured LIVE, over the bytes the bundle
/// actually shipped and a corpus derived from the bundle itself.
///
/// It is the one measurable dictionary whose population no bundle frame belongs to: its
/// frames are the ones a CONSUMER writes into a runtime store out of the shipped header.
/// So the reading cannot be published in the bundle (a store's records carry a wall
/// clock, and a bundle asserting numbers about a file it never saw would be speaking
/// past its evidence) — the committed table holds it, and THIS is where it is measured
/// against reality: real store files, written through the real `Memory::store` path,
/// primed with the header entry the pack ships, over a declared corpus taken from the
/// pack's own statement layer.
fn the_runtime_store_dictionary_pays_for_itself(bundle: &[u8], pinned: &BTreeMap<String, Vec<u8>>) {
    use gmeow_pipeline::medium::{measure, sweep};

    let folded = purrdf::import_gts_events(bundle).expect("the emitted bundle folds back");
    let corpus = sweep::replay_corpus(folded.dataset.as_ref())
        .expect("the bundle-derived replay corpus resolves");
    // The replay extent is part of the claim — whether a store dictionary wins is a pure
    // function of the record count — so it is pinned in BOTH directions rather than
    // merely bounded. [`sweep::REPLAY_RECORD_COUNT`] is the declared CEILING; the bundle's
    // own statement layer is shorter than it, so the operative number is the one the
    // committed evidence recorded, and the live corpus must be exactly that. Pinning it to
    // the ceiling instead would assert a corpus this bundle cannot produce; pinning it to
    // nothing would let the live and committed readings silently price different
    // populations.
    let committed = sweep::load(&repo_root()).expect("the committed winner table is readable");
    let recorded = committed
        .row("gmeow-memory-hot-v1")
        .expect("the committed table carries the runtime-store row")
        .evaluated_frame_count;
    assert!(
        corpus.len() as u64 <= sweep::REPLAY_RECORD_COUNT as u64,
        "the replay corpus ({}) exceeds the DECLARED ceiling {}",
        corpus.len(),
        sweep::REPLAY_RECORD_COUNT
    );
    assert_eq!(
        corpus.len() as u64,
        recorded,
        "the LIVE replay corpus and the COMMITTED runtime-store reading must price the SAME \
         population — a divergence means the committed evidence describes a corpus this bundle \
         does not produce"
    );
    assert!(
        corpus.len() >= 64,
        "a replay corpus of {} record(s) is too thin for the measurement to mean anything",
        corpus.len()
    );

    let dict = pinned
        .get("gmeow-memory-hot-v1")
        .expect("the header pins gmeow-memory-hot-v1");
    let dir = tempfile::tempdir().expect("tempdir");
    let (primed, baseline) =
        sweep::replay_runtime_store(dir.path(), "gmeow-memory-hot-v1", dict, &corpus)
            .expect("the replay writes both arms through the real store path");

    let effect = measure::population_b(
        "gmeow-memory-hot-v1",
        &primed,
        &baseline,
        corpus.len() as u64,
        corpus.len() as u64,
    )
    .expect("the replayed stores measure");
    assert_eq!(
        effect.dictionary_in_band_bytes,
        dict.len() as u64,
        "an append-only store pins the dictionary exactly ONCE — if this ever becomes a multiple, \
         the store stopped continuing its tail segment and the dictionary is being paid for per \
         record"
    );
    println!(
        "gmeow-memory-hot-v1 population B: {} records, two-part {} B vs baseline {} B (gain {})",
        corpus.len(),
        effect.two_part_code_bytes(),
        effect.bytes_on_disk_baseline,
        effect.gain_fraction_lexical()
    );
    measure::check(&[effect], &BTreeSet::new())
        .expect("the shipped gmeow-memory-hot-v1 must pay for itself on a real runtime store");
}
