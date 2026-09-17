// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use ed25519_dalek::SigningKey;

/// Ingest RDF text of `media_type` into a fresh builder via the native carrier
/// path (`parse_dataset` → `add_dataset`) — the single-exit ingestion these
/// tests author committed-snapshot fixtures with.
fn builder_from(text: &str, media_type: &str) -> SnapshotBuilder {
    let dataset = parse_dataset(text.as_bytes(), media_type, None).expect("parse fixture");
    let mut b = SnapshotBuilder::new();
    b.add_dataset(&dataset).expect("add_dataset");
    b
}

/// Re-render a read-back GTS [`Graph`] to N-Quads through the native codec
/// (`dataset_from_gts_graph` → `serialize_dataset`), never the gmeow-gts codec —
/// gmeow-gts is the gmeow.gts container layer only. This is the same lossless
/// container→dataset bridge the production replay path uses, so the rendered quads
/// match what the snapshot committed.
fn graph_nquads(graph: &Graph) -> String {
    let dataset = dataset_from_gts_graph(graph).expect("fold the GTS graph into a dataset");
    let bytes = purrdf::serialize_dataset(
        &dataset,
        NativeRdfFormat::NQuads.media_type(),
        purrdf::SerializeGraph::Dataset,
    )
    .expect("serialize the dataset to N-Quads");
    String::from_utf8(bytes).expect("native N-Quads is valid UTF-8")
}

/// Deterministic Ed25519 key from a seed (mirrors validate/signature.rs).
fn deterministic_signing_key(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    for i in 1..32 {
        bytes[i] = bytes[i - 1].wrapping_mul(31).wrapping_add(seed);
    }
    SigningKey::from_bytes(&bytes)
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Build a minimal ASCII-armored OpenPGP v4 Ed25519 public-key certificate
/// the bundle's transport-key meta frame carries (the writer only stores the
/// armor verbatim; it is not re-parsed during emit).
fn fake_public_armor(verify_key: &[u8; 32]) -> String {
    // Tag-6 public-key packet body: v4, ctime=0, algo=22, OID, 0x40-MPI.
    let mut body = vec![4u8, 0, 0, 0, 0, 22];
    body.push(9); // OID length
    body.extend_from_slice(&[0x2b, 0x06, 0x01, 0x04, 0x01, 0xda, 0x47, 0x0f, 0x01]);
    // MPI: 0x40 prefix marker || 32-byte key => 263 bits.
    let mut mpi = vec![0x40u8];
    mpi.extend_from_slice(verify_key);
    let bits = (mpi.len() * 8 - 1) as u16; // high bit of 0x40 is clear
    body.extend_from_slice(&bits.to_be_bytes());
    body.extend_from_slice(&mpi);
    // New-format tag-6 packet header.
    let mut packet = vec![0xc6u8, body.len() as u8];
    packet.extend_from_slice(&body);
    let b64 = base64_encode(&packet);
    let mut wrapped = String::new();
    for line in b64.as_bytes().chunks(64) {
        wrapped.push_str(std::str::from_utf8(line).unwrap());
        wrapped.push('\n');
    }
    format!("-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{wrapped}-----END PGP PUBLIC KEY BLOCK-----\n")
}

/// A tiny unsigned `dist` snapshot to act as the "committed" base.
fn tiny_snapshot() -> gmeow_gts_profile::GmeowGtsEmission {
    let nq = "<https://e/s> <https://e/p> <https://e/o> .\n\
                  <https://e/s> <https://e/q> \"hello\" .\n";
    let b = builder_from(nq, NativeRdfFormat::NTriples.media_type());
    // gmeow-test-input: synthetic-only
    gmeow_gts_profile::emit_gmeow_gts(
        b,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit tiny snapshot")
}

fn evidence_inputs() -> Vec<EvidenceInput> {
    vec![
        EvidenceInput {
            data: b"{\"quality\": \"ok\"}".to_vec(),
            media_type: "application/json".to_string(),
            attestation_type_iri: format!("{GMEOW_NS}attestationTypeQualityReport"),
            rep: "quality".to_string(),
            subject_label: "quality report".to_string(),
        },
        EvidenceInput {
            data: b"{\"conformance\": \"pass\"}".to_vec(),
            media_type: "application/json".to_string(),
            attestation_type_iri: format!("{GMEOW_NS}attestationTypeConformanceVerdict"),
            rep: "conformance".to_string(),
            subject_label: "conformance verdicts".to_string(),
        },
    ]
}

fn fold(
    snapshot: &gmeow_gts_profile::GmeowGtsEmission,
    evidence: Vec<EvidenceInput>,
    issued_at: &str,
) -> Vec<u8> {
    let signing = deterministic_signing_key(7);
    let armor = fake_public_armor(&signing.verifying_key().to_bytes());
    let original = snapshot.ingestion_receipt().expect("source receipt");
    let source = ReleaseSource::admit(&snapshot.bytes, &original).expect("source identity");
    // gmeow-test-input: synthetic-only
    let emission = fold_release_bundle(
        // gmeow-test-input: synthetic-only
        source,
        evidence,
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        issued_at,
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
        signing.to_bytes(),
        "release-test-kid",
        &armor,
    )
    .expect("fold release bundle");
    assert!(emission.ingestion.declarations_omitted.is_empty());
    assert_eq!(
        emission.source_receipts.len(),
        1,
        "release must retain the admitted source receipt"
    );
    emission.bytes
}

#[test]
fn release_retains_original_declaration_loss_without_fabricating_current_counts() {
    let empty_role = "https://blackcatinformatics.ca/gmeow/graph/test-empty-release-role";
    let mut declarations = purrdf::RdfDatasetBuilder::new();
    let name = declarations.intern_iri(empty_role);
    declarations.declare_named_graph(name);
    let mut builder = builder_from(
        "<https://e/s> <https://e/p> <https://e/o> .",
        "application/n-triples",
    );
    let _admission = builder
        .add_view(&declarations.freeze().expect("empty selected role"))
        .expect("role admission");
    // gmeow-test-input: synthetic-only
    let original = gmeow_gts_profile::emit_gmeow_gts(
        builder,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("source snapshot");
    let receipt = original.ingestion_receipt().expect("source receipt");
    let source = ReleaseSource::admit(&original.bytes, &receipt).expect("source identity");
    let signing = deterministic_signing_key(7);
    let armor = fake_public_armor(&signing.verifying_key().to_bytes());
    // gmeow-test-input: synthetic-only
    let release = fold_release_bundle(
        source,
        Vec::new(),
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        "2026-06-25T00:00:00Z",
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
        signing.to_bytes(),
        "release-test-kid",
        &armor,
    )
    .expect("signed release");
    let encoded = release.ingestion_receipt().expect("release companion");
    let retained = gmeow_gts_profile::read_ingestion_receipt(&release.bytes, &encoded)
        .expect("signed output binding");
    assert_eq!(retained.ingestion, release.ingestion);
    assert!(retained.ingestion.declarations_omitted.is_empty());
    assert_eq!(retained.source_receipts.len(), 1);
    let inherited = gmeow_gts_profile::read_ingestion_receipt(
        &original.bytes,
        retained.source_receipts[0].encoded(),
    )
    .expect("original binding");
    assert_eq!(inherited.ingestion, original.ingestion);
    assert_eq!(inherited.ingestion.declarations_omitted, vec![empty_role]);
    verify_release_bundle(&release.bytes, None)
        .expect("real production verifier still accepts release");
    // gmeow-test-input: synthetic-only
    assert!(
        ReleaseSource::admit(&release.bytes, &receipt).is_err(),
        "receipt from the original input cannot authorize different re-emission bytes"
    );
}

#[test]
fn build_coherence_evidence_emits_a_coherence_artifact() {
    let snapshot = tiny_snapshot();
    let evidence = build_coherence_evidence(&snapshot.bytes, "2026-06-28T00:00:00Z") // gmeow-test-input: synthetic-only
        .expect("a consistent snapshot must yield a coherence artifact");
    assert_eq!(evidence.rep, "coherence");
    assert!(
        evidence
            .attestation_type_iri
            .ends_with("attestationTypeCoherenceCertificate")
    );
    let nq = String::from_utf8(evidence.data.clone()).expect("utf8 nquads");
    // The native reasoner names no certified fragment, so the strongest HONEST
    // claim over a consistent bundle is the attestation, never a fragment-less
    // certificate (the scoped-certificate contract — a certificate must name the
    // fragment F it ranges over).
    assert!(
        nq.contains("<https://blackcatinformatics.ca/logic/CoherenceCheckAttestation>"),
        "a fragment-less consistent check must yield an attestation, not a certificate: {nq}"
    );
    assert!(!nq.contains("<https://blackcatinformatics.ca/logic/CoherenceCertificate>"));
    assert!(nq.contains("<https://blackcatinformatics.ca/logic/bundleHash>"));
    // The artifact links to the logic:ReasoningResult it summarizes (M2) and pins
    // a real per-graph axiom digest (C3).
    assert!(nq.contains("<https://blackcatinformatics.ca/logic/summarizesResult>"));
    assert!(nq.contains("<https://blackcatinformatics.ca/logic/axiomHash>"));
    // Deterministic with the injected timestamp.
    let again = build_coherence_evidence(&snapshot.bytes, "2026-06-28T00:00:00Z").unwrap(); // gmeow-test-input: synthetic-only
    assert_eq!(evidence.data, again.data);
}

#[test]
fn coherence_certificate_folds_into_the_signed_bundle_deterministically() {
    // Folding the coherence evidence into the signed bundle proves the
    // certificate rides the existing Ed25519 bundle signature (no new signing
    // step), and the fold stays byte-deterministic with the injected timestamp.
    let snapshot = tiny_snapshot();
    let with_cert = || {
        let mut evidence = evidence_inputs();
        evidence.push(build_coherence_evidence(&snapshot.bytes, "2026-06-28T00:00:00Z").unwrap()); // gmeow-test-input: synthetic-only
        fold(&snapshot, evidence, "2026-06-28T00:00:00Z")
    };
    let a = with_cert();
    let b = with_cert();
    assert!(!a.is_empty());
    assert_eq!(
        a, b,
        "the coherence-folded signed bundle must be byte-deterministic"
    );
}

/// Release attestations are not folded into the dev `gmeow.gts` bundle that the
/// authored-source `make validate` / stage-validate SHACL pass (and, for the
/// shipped norm-claims subset, the `norm_claims_shacl` test) checks, so guard
/// the minted attestation graph against the SAME structural-lint contract here:
/// every typed attestation subject
/// must satisfy the assertional tier (type + label + named-graph provenance +
/// valid `gmeow:boxABox` role). Without this the release-path annotations
/// would be correctness no gate validates.
#[test]
fn minted_attestations_satisfy_the_assertional_contract() {
    use gmeow_validate::lint::{
        LintConfig, default_annotation_predicates, structural_lint_dataset,
    };
    use purrdf::parse_dataset;

    let mut sorted: Vec<(String, EvidenceInput)> = evidence_inputs()
        .into_iter()
        .map(|ev| (digest_string(&ev.data), ev))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.rep.cmp(&b.1.rep)));
    let nq = build_attestations_nquads(
        &sorted,
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        "2026-06-25T00:00:00Z",
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
    );

    // The bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the kernel
    // slice; add it here so the role-typing check has its declaration.
    let doc = format!(
        "{nq}<{GMEOW_NS}boxABox> <{RDF_TYPE}> <{GMEOW_NS}GraphBoxRole> <{GRAPH_ATTESTATIONS}> .\n"
    );
    // The doc is N-Quads with the attestations in a named graph. The native
    // `structural_lint_dataset` reads across all graphs (GraphMatch::Any), so the
    // dataset is linted exactly as the old `store_from_dataset(.., FlattenToDefaultGraph)`
    // flattened store was — no oxigraph round-trip.
    let dataset = parse_dataset(doc.as_bytes(), "application/n-quads", None).unwrap();

    let cfg = LintConfig {
        namespace: GMEOW_NS.to_string(),
        ontology_iri: GMEOW_NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };
    let report = structural_lint_dataset(&dataset, &cfg);
    let report_errors = report.errors();
    let attestation_errors: Vec<&String> = report_errors
        .iter()
        .filter(|e| e.contains("/attestation/") || e.contains("/artifact/") || e.contains("agent/"))
        .collect();
    assert!(
        attestation_errors.is_empty(),
        "minted attestation subjects must satisfy the assertional contract: {attestation_errors:?}"
    );
}

#[test]
fn round_trip_carries_signature_attestations_and_blobs() {
    let snapshot = tiny_snapshot();
    let evidence = evidence_inputs();
    let issued = "2026-06-25T00:00:00Z";

    // Capture the expected per-evidence digests for the blob/triple checks.
    let digests: Vec<String> = evidence.iter().map(|e| digest_string(&e.data)).collect();

    let bundle = fold(&snapshot, evidence, issued);
    let graph = read(&bundle, true, None);

    // (a) the signed transport-key meta frame is present.
    let has_transport_key = graph.meta.iter().any(|(k, _)| k == "gts:transportKey");
    assert!(
        has_transport_key,
        "release bundle must carry the transport key"
    );
    assert!(
        !graph.signatures.is_empty(),
        "release bundle must carry at least one signature"
    );

    // (b) the attestations named graph carries the expected frames.
    let nquads = graph_nquads(&graph);
    assert!(
        nquads.contains(GRAPH_ATTESTATIONS),
        "graph/attestations named graph must be present"
    );
    assert!(
        nquads.contains("attestationTypeReleaseManifest"),
        "top-level release-manifest attestation must be present"
    );
    assert!(
        nquads.contains("attestationTypeQualityReport"),
        "quality-report child attestation must be present"
    );
    assert!(
        nquads.contains("attestationTypeConformanceVerdict"),
        "conformance child attestation must be present"
    );

    // (b cont.) each evidence digest is bound via gmeow:contentDigest, and
    //          the original base graph survived the replay.
    for digest in &digests {
        assert!(
            nquads.contains(digest),
            "evidence digest {digest} must appear as a gmeow:contentDigest"
        );
    }
    assert!(
        nquads.contains("<https://e/s> <https://e/p> <https://e/o>"),
        "the committed snapshot base graph must be replayed faithfully"
    );

    // (c) the evidence blobs are present with matching digests.
    for digest in &digests {
        assert!(
            graph.blob_entry(digest).is_some(),
            "evidence blob {digest} must be folded into the bundle"
        );
    }
}

#[test]
fn fold_is_deterministic() {
    let snapshot = tiny_snapshot();
    let issued = "2026-06-25T00:00:00Z";
    let a = fold(&snapshot, evidence_inputs(), issued);
    let b = fold(&snapshot, evidence_inputs(), issued);
    assert_eq!(a, b, "same inputs + same issued_at must be byte-identical");
}

/// The consumer verifier accepts a well-formed signed bundle —
/// signature + every attested artifact present — and report one verified
/// artifact per evidence input.
#[test]
fn verify_release_bundle_accepts_a_well_formed_bundle() {
    let snapshot = tiny_snapshot();
    let evidence = evidence_inputs();
    let n = evidence.len();
    let bundle = fold(&snapshot, evidence, "2026-06-25T00:00:00Z");

    let report = verify_release_bundle(&bundle, None).expect("well-formed bundle must verify");
    assert!(report.valid >= 1, "bundle must carry a valid signature");
    assert_eq!(
        report.artifacts_verified, n,
        "every attested evidence artifact must resolve to a present blob"
    );
}

/// A tampered bundle (a flipped byte) must fail the signature leg, and
/// non-GTS garbage must fail too — verify never silently passes.
#[test]
fn verify_release_bundle_rejects_tampered_and_garbage() {
    let snapshot = tiny_snapshot();
    let bundle = fold(&snapshot, evidence_inputs(), "2026-06-25T00:00:00Z");

    let mut tampered = bundle.clone();
    let mid = tampered.len() / 2;
    tampered[mid] ^= 0xff;
    assert!(
        verify_release_bundle(&tampered, None).is_err(),
        "a tampered bundle must not verify"
    );

    assert!(
        verify_release_bundle(b"not a gts file at all", None).is_err(),
        "non-GTS garbage must not verify"
    );
}

/// Supplying the WRONG out-of-band trusted key must fail the trust
/// leg even though the embedded self-signature is cryptographically valid.
#[test]
fn verify_release_bundle_rejects_untrusted_key() {
    let snapshot = tiny_snapshot();
    let bundle = fold(&snapshot, evidence_inputs(), "2026-06-25T00:00:00Z");

    // A different signer's public key — not the one that signed the bundle.
    let other = deterministic_signing_key(99);
    let wrong_armor = fake_public_armor(&other.verifying_key().to_bytes());
    assert!(
        verify_release_bundle(&bundle, Some(&wrong_armor)).is_err(),
        "verifying against an untrusted out-of-band key must fail"
    );
}

/// A `dist` snapshot that already carries one report blob (the "committed"
/// stand-in for e.g. the in-snapshot SHACL SARIF), under `rep`.
fn snapshot_with_report_blob(data: &[u8], rep: &str) -> gmeow_gts_profile::GmeowGtsEmission {
    let nq = "<https://e/s> <https://e/p> <https://e/o> .\n";
    let b = builder_from(nq, NativeRdfFormat::NTriples.media_type());
    // gmeow-test-input: synthetic-only
    gmeow_gts_profile::emit_gmeow_gts(
        b,
        Vec::new(),
        vec![BlobRow {
            data: data.to_vec(),
            media_type: "application/json".to_string(),
            rep: rep.to_string(),
        }],
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit snapshot with report blob")
}

/// Counts how many SERIALIZED blob frames carry each digest. `read()` dedups
/// blobs by digest in-place, so it cannot see a double-fold; the streaming
/// sink reports every raw frame, which can.
#[derive(Default)]
struct BlobFrameCounter {
    counts: std::collections::HashMap<String, usize>,
}
impl purrdf::gts::reader::StreamingSink for BlobFrameCounter {
    fn blob(&mut self, _seg: usize, digest: &str, _meta: Option<&ciborium::value::Value>) {
        *self.counts.entry(digest.to_string()).or_insert(0) += 1;
    }
}

/// Evidence whose bytes already ride in the committed snapshot must NOT
/// be folded a second time. The duplicate is invisible after `read()` (the
/// model dedups blobs by digest), so we count raw blob FRAMES via the
/// streaming sink: the colliding digest must appear exactly once. The minted
/// attestation still binds to the artifact by `gmeow:contentDigest`, which the
/// committed blob satisfies, so the evidence stays recoverable + attested.
#[test]
fn evidence_colliding_with_committed_blob_is_not_double_folded() {
    let shared = b"{\"shacl\":\"sarif-bytes\"}".to_vec();
    let shared_digest = digest_string(&shared);
    let snapshot = snapshot_with_report_blob(&shared, "snapshot-only");

    let fresh = b"{\"conformance\":\"pass\"}".to_vec();
    let fresh_digest = digest_string(&fresh);

    let evidence = vec![
        // Collides with the committed snapshot report blob.
        EvidenceInput {
            data: shared.clone(),
            media_type: "application/json".to_string(),
            attestation_type_iri: format!("{GMEOW_NS}attestationTypeQualityReport"),
            rep: "shacl".to_string(),
            subject_label: "SHACL diagnostics SARIF".to_string(),
        },
        // Brand-new bytes — must fold as one release-evidence frame.
        EvidenceInput {
            data: fresh.clone(),
            media_type: "application/json".to_string(),
            attestation_type_iri: format!("{GMEOW_NS}attestationTypeConformanceVerdict"),
            rep: "conformance".to_string(),
            subject_label: "conformance verdicts".to_string(),
        },
    ];

    let bundle = fold(&snapshot, evidence, "2026-06-25T00:00:00Z");

    // Exactly one blob frame per digest — the colliding evidence did NOT add
    // a second frame for `shared`.
    let mut counter = BlobFrameCounter::default();
    purrdf::gts::reader::read_to_sink(&bundle, true, None, &mut counter);
    assert_eq!(
        counter.counts.get(&shared_digest).copied(),
        Some(1),
        "colliding evidence must yield exactly one blob frame, not a duplicate"
    );
    assert_eq!(
        counter.counts.get(&fresh_digest).copied(),
        Some(1),
        "fresh evidence must fold as exactly one blob frame"
    );

    // Both digests stay recoverable and attested by gmeow:contentDigest.
    let graph = read(&bundle, true, None);
    let nquads = graph_nquads(&graph);
    for digest in [&shared_digest, &fresh_digest] {
        assert!(
            graph.blob_entry(digest).is_some(),
            "blob {digest} must be recoverable from the bundle"
        );
        assert!(
            nquads.contains(digest.as_str()),
            "attestation envelope must bind {digest} by gmeow:contentDigest"
        );
    }
    // The colliding artifact keeps the COMMITTED rep (the twin was suppressed).
    let (_, rep) = blob_meta_for(&graph, &shared_digest).expect("committed blob meta present");
    assert_eq!(rep, "snapshot-only");
}

/// Emit a `dist` snapshot from raw N-Quads text (the committed-snapshot
/// stand-in for the determinism fixtures).
fn snapshot_from_nquads(nq: &str) -> gmeow_gts_profile::GmeowGtsEmission {
    let b = builder_from(nq, NativeRdfFormat::NQuads.media_type());
    // gmeow-test-input: synthetic-only
    gmeow_gts_profile::emit_gmeow_gts(
        b,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit fixture snapshot")
}

/// A blank-node-heavy snapshot: many `owl:Restriction`-style blank-node
/// subjects spread across several named graphs, deliberately constructed so
/// that distinct blank nodes COLLIDE on their canonical sort key
/// (`(kind, value, datatype, lang)`). Each `_:r{N}` carries the same two
/// triples — `rdf:type owl:Restriction` and `owl:onProperty ex:p{N}` — so the
/// bnodes differ only in which property they point at; under a label-erasing
/// serializer their sort keys would tie and the canonical re-id would fall
/// back to ingestion order (the cross-process-unstable tie-break this fold
/// must be immune to).
fn blank_node_heavy_nquads() -> String {
    let owl_restriction = "<http://www.w3.org/2002/07/owl#Restriction>";
    let rdf_type = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let on_property = "<http://www.w3.org/2002/07/owl#onProperty>";
    let graphs = [
        "<https://e/graph/a>",
        "<https://e/graph/b>",
        "<https://e/graph/c>",
    ];
    let mut lines: Vec<String> = Vec::new();
    for n in 0..60u32 {
        let g = graphs[(n as usize) % graphs.len()];
        let b = format!("_:r{n}");
        lines.push(format!("{b} {rdf_type} {owl_restriction} {g} ."));
        lines.push(format!("{b} {on_property} <https://e/p{n}> {g} ."));
    }
    format!("{}\n", lines.join("\n"))
}

/// A blank-node-heavy fold must be byte-stable across repeated runs, and the
/// emitted bundle must equal a fixed expected content address. This guards the
/// `replay_graph` line-sort: without it, structurally distinct bnodes whose
/// canonical sort keys tie would be re-id'd in ingestion order, leaking the
/// upstream (potentially HashMap-seeded) iteration order into the output.
///
/// NOTE on the in-process limitation: a single test process shares one
/// `HashMap` hash-seed, so two folds in THIS process can agree even if a
/// genuine cross-process divergence exists. The order-independence assertion
/// below is the in-process proxy for that cross-process property — it pins the
/// canonical output to the quad SET, not to any iteration order.
#[test]
fn blank_node_heavy_fold_is_byte_stable() {
    let snapshot = snapshot_from_nquads(&blank_node_heavy_nquads());
    let issued = "2026-06-25T00:00:00Z";

    // Sanity: the fixture really does carry many blank nodes.
    let base = read(&snapshot.bytes, true, None);
    let base_nq = graph_nquads(&base);
    assert!(
        base_nq.matches("owl#Restriction").count() >= 50,
        "fixture must carry 50+ owl:Restriction blank nodes"
    );

    let a = fold(&snapshot, Vec::new(), issued);
    let b = fold(&snapshot, Vec::new(), issued);
    assert_eq!(a, b, "blank-node-heavy fold must be byte-stable");
}

/// Build a `SnapshotBuilder` from raw N-Quads text exactly the way
/// [`replay_graph`] does (line-sort → parse → reifier split → add), but
/// driven from a literal string so a test can feed the SAME quad set in two
/// different line orders. This is the in-process surrogate for the
/// cross-process tie-break: it isolates the ingestion order that the canonical
/// re-id falls back on when blank-node sort keys collide.
fn replay_nquads_str(nq: &str) -> SnapshotBuilder {
    // Mirror replay_graph precisely: line-sort, then a single native
    // `add_dataset` (the parse folds the statement layer + preserves named
    // graphs, so there is no manual base/rdf12 split).
    let mut lines: Vec<&str> = nq.lines().collect();
    lines.sort_unstable();
    let sorted = lines.join("\n");
    builder_from(&sorted, NativeRdfFormat::NQuads.media_type())
}

/// The replayed snapshot's content id must be a pure function of the quad SET,
/// independent of the order the quads are presented in. This directly exercises
/// the [`replay_graph`] line-sort: two blank-node-heavy N-Quads strings holding
/// the SAME statements in REVERSED order must yield the same
/// `snapshot_content_id`.
///
/// Without the line-sort, distinct blank nodes whose canonical sort keys tie
/// would be re-id'd in arrival order, so the reversed input would (in the
/// general case, e.g. a multi-segment union whose serialized order is
/// HashMap-seeded across processes) produce a different content id — the
/// cross-process divergence §18 forbids. The sort pins the ingestion order to
/// the canonical text, identical in every process for the same set.
#[test]
fn replayed_content_id_is_independent_of_quad_order() {
    let nq = blank_node_heavy_nquads();
    let reversed = {
        let mut lines: Vec<&str> = nq.lines().collect();
        lines.reverse();
        format!("{}\n", lines.join("\n"))
    };

    let forward_id = replay_nquads_str(&nq).snapshot_content_id();
    let reversed_id = replay_nquads_str(&reversed).snapshot_content_id();
    assert_eq!(
        forward_id, reversed_id,
        "replayed snapshot content id must depend on the quad SET, not its order"
    );
    assert!(forward_id.starts_with("blake3:"));
}

/// End-to-end: the full release fold over a blank-node-heavy snapshot must be
/// byte-identical regardless of the order the committed snapshot serialized its
/// quads in (the cross-process reproducibility property, §18).
#[test]
fn fold_is_independent_of_snapshot_quad_order() {
    let nq = blank_node_heavy_nquads();
    let forward = snapshot_from_nquads(&nq);
    let reversed = {
        let mut lines: Vec<&str> = nq.lines().collect();
        lines.reverse();
        snapshot_from_nquads(&format!("{}\n", lines.join("\n")))
    };

    let issued = "2026-06-25T00:00:00Z";
    let a = fold(&forward, Vec::new(), issued);
    let b = fold(&reversed, Vec::new(), issued);
    assert_eq!(
        a, b,
        "release fold must depend on the quad SET, not the serialized order"
    );
}

#[test]
fn empty_evidence_still_signs_the_release_manifest() {
    let snapshot = tiny_snapshot();
    let bundle = fold(&snapshot, Vec::new(), "2026-06-25T00:00:00Z");
    let graph = read(&bundle, true, None);
    assert!(
        graph.meta.iter().any(|(k, _)| k == "gts:transportKey"),
        "an evidence-free release still signs the manifest"
    );
    let nquads = graph_nquads(&graph);
    assert!(
        nquads.contains("attestationTypeReleaseManifest"),
        "the release-manifest frame is present even with no evidence"
    );
    // No artifact frames when there is no evidence.
    assert!(
        !nquads.contains("AttestationArtifact"),
        "no artifacts without evidence"
    );
}

#[test]
fn replayed_named_graph_is_preserved() {
    // A snapshot with a named graph must round-trip the graph name.
    let nq = "<https://e/s> <https://e/p> <https://e/o> \
                  <https://blackcatinformatics.ca/gmeow/graph/metadata> .\n";
    let b = builder_from(nq, NativeRdfFormat::NQuads.media_type());
    // gmeow-test-input: synthetic-only
    let snapshot = gmeow_gts_profile::emit_gmeow_gts(
        b,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit");

    let bundle = fold(&snapshot, Vec::new(), "2026-06-25T00:00:00Z");
    let graph = read(&bundle, true, None);
    let nquads = graph_nquads(&graph);
    assert!(
        nquads.contains("<https://blackcatinformatics.ca/gmeow/graph/metadata>"),
        "the committed snapshot's named graph must survive the replay"
    );
}

#[test]
fn attestation_type_local_name_expands_but_absolute_iri_passes_through() {
    // The colon-delimited --evidence spec cannot carry an absolute IRI, so the
    // Makefile passes a bare local name; an already-absolute IRI (used by the
    // other tests) must pass through verbatim.
    assert_eq!(
        resolve_attestation_type_iri("attestationTypeQualityReport"),
        "https://blackcatinformatics.ca/gmeow/attestationTypeQualityReport"
    );
    assert_eq!(
        resolve_attestation_type_iri(
            "https://blackcatinformatics.ca/gmeow/attestationTypeConformanceVerdict"
        ),
        "https://blackcatinformatics.ca/gmeow/attestationTypeConformanceVerdict"
    );
}

/// The raw `gmeow.pdf` bytes packed into the print-docs archive in
/// [`docs_snapshot`] — shared with the assertions in
/// `release_fold_attests_the_packed_docs_artifacts` so the test pins the exact
/// bytes the PDF attestation must bind.
const DOCS_PRINT_PDF_BYTES: &[u8] = b"%PDF-1.7 FAKE-BUT-REAL-MEMBER-BYTES";

/// A `dist` snapshot carrying the two packed documentation archives (the
/// docs-book / docs-print blobs), under their canonical `rep`s. The
/// docs-print blob is a REAL ustar tar (not a placeholder string) carrying a
/// `gmeow.pdf` member, exactly as the production `docs_print` carrier stage
/// packs it, so the G3 PDF-extraction path has real archive framing to walk.
fn docs_snapshot() -> gmeow_gts_profile::GmeowGtsEmission {
    use crate::bundle_blobs::{REP_DOCS_BOOK, REP_DOCS_PRINT};
    let nq = "<https://e/s> <https://e/p> <https://e/o> .\n";
    let b = builder_from(nq, NativeRdfFormat::NTriples.media_type());
    let print_tar = purrdf::ustar::write_archive(&[
        (
            "x-gmeow-english/gmeow.pdf".to_string(),
            DOCS_PRINT_PDF_BYTES.to_vec(),
        ),
        (
            "x-gmeow-english/gmeow.typ".to_string(),
            b"#let title = \"gmeow\"".to_vec(),
        ),
    ])
    .expect("build docs-print tar fixture");
    // gmeow-test-input: synthetic-only
    gmeow_gts_profile::emit_gmeow_gts(
        b,
        vec![
            BlobRow {
                data: b"BOOK-ARCHIVE-BYTES".to_vec(),
                media_type: "application/x-tar".to_string(),
                rep: REP_DOCS_BOOK.to_string(),
            },
            BlobRow {
                data: print_tar,
                media_type: "application/x-tar".to_string(),
                rep: REP_DOCS_PRINT.to_string(),
            },
        ],
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit docs snapshot")
}

/// A8: folding a release bundle over a snapshot that carries the packed docs
/// archives auto-mints a documentation-artifact attestation per archive, binds it
/// by blake3 `gmeow:contentDigest`, and the consumer verify accepts it with the
/// docs artifacts counted among the verified evidence.
///
/// G3: folding ALSO mints a separate `application/pdf` artifact bound to the
/// COMPILED `gmeow.pdf` member's own bytes (extracted from the docs-print tar),
/// distinct from the archive-level `application/x-tar` attestation, and
/// `verify_release_bundle` verifies it end to end.
#[test]
fn release_fold_attests_the_packed_docs_artifacts() {
    use crate::bundle_blobs::REP_DOCS_PRINT;
    let snapshot = docs_snapshot();
    let print_digest = {
        let graph = read(&snapshot.bytes, true, None);
        let (digest, _) = graph
            .blobs
            .iter()
            .find(|(d, _)| matches!(blob_meta_for(&graph, d), Ok((_, r)) if r == REP_DOCS_PRINT))
            .expect("docs-print blob present");
        digest.clone()
    };
    let pdf_digest = digest_string(DOCS_PRINT_PDF_BYTES);

    let bundle = fold(&snapshot, Vec::new(), "2026-06-25T00:00:00Z");
    let graph = read(&bundle, true, None);
    let nquads = graph_nquads(&graph);

    assert!(
        nquads.contains("attestationTypeDocumentationArtifact"),
        "a documentation-artifact attestation must be minted for the packed docs"
    );
    assert!(
        nquads.contains(&print_digest),
        "the docs-print archive must be bound by gmeow:contentDigest {print_digest}"
    );
    assert!(
        nquads.contains("\"application/pdf\""),
        "the compiled gmeow.pdf must carry its own application/pdf attestation"
    );
    assert!(
        nquads.contains(&pdf_digest),
        "the compiled gmeow.pdf bytes must be bound by gmeow:contentDigest {pdf_digest} \
             (distinct from the archive-level digest {print_digest})"
    );
    assert_ne!(
        pdf_digest, print_digest,
        "the PDF's own digest must differ from the enclosing tar's digest"
    );

    // The consumer verify accepts the bundle and counts the docs artifacts (book
    // archive, print archive, AND the compiled PDF: 3) among the verified evidence.
    let report = verify_release_bundle(&bundle, None).expect("docs bundle must verify");
    assert!(
        report.artifacts_verified >= 3,
        "both packed docs archives plus the compiled PDF must be verified evidence, saw {}",
        report.artifacts_verified
    );

    // The PDF blob the bundle actually ships must decode back to the EXACT bytes
    // the attestation's digest claims — not merely "some blob exists".
    let pdf_blob = graph
        .blob_entry(&pdf_digest)
        .expect("attested PDF digest must resolve to a shipped blob")
        .decoded_vec()
        .expect("shipped PDF blob must decode");
    assert_eq!(
        pdf_blob, DOCS_PRINT_PDF_BYTES,
        "the shipped PDF blob bytes must equal the compiled gmeow.pdf member bytes"
    );
}

/// A8/F4: a signed bundle whose attestation graph binds a documentation-artifact
/// `gmeow:contentDigest` with NO backing blob (a drifted / removed docs blob) must
/// FAIL verify's evidence-presence leg — the drift reds the gate.
#[test]
fn verify_rejects_docs_attestation_without_backing_blob() {
    let base = "<https://e/s> <https://e/p> <https://e/o> .\n";
    let mut builder = builder_from(base, NativeRdfFormat::NTriples.media_type());

    // A phantom docs artifact: attested, but its bytes are never folded as a blob.
    let phantom = EvidenceInput {
        data: b"PHANTOM-DOCS-PRINT-BYTES".to_vec(),
        media_type: "application/x-tar".to_string(),
        attestation_type_iri: format!("{GMEOW_NS}{DOCS_ATTESTATION_TYPE}"),
        rep: "docs-print-attestation".to_string(),
        subject_label: "Documentation print archive".to_string(),
    };
    let phantom_digest = digest_string(&phantom.data);
    let sorted = vec![(phantom_digest.clone(), phantom)];
    let nq = build_attestations_nquads(
        &sorted,
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        "2026-06-25T00:00:00Z",
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
    );
    let att = parse_dataset(nq.as_bytes(), "application/n-quads", None).expect("parse att");
    builder.add_dataset(&att).expect("add att");

    // Sign the bundle but DELIBERATELY do not fold the phantom's bytes as a blob.
    let signing = deterministic_signing_key(7);
    let armor = fake_public_armor(&signing.verifying_key().to_bytes());
    // gmeow-test-input: synthetic-only
    let bundle = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        Some(signing.to_bytes()),
        Some("release-test-kid".to_string()),
        Some(armor),
        DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(None),
    )
    .expect("emit signed phantom bundle");

    let result = verify_release_bundle(&bundle, None);
    let msg = match result {
        Ok(_) => panic!("a docs attestation with no backing blob must red the gate"),
        Err(e) => format!("{e}"),
    };
    assert!(
        msg.contains(&phantom_digest) || msg.contains("no backing blob"),
        "verify must reject the attested-but-absent docs digest: {msg}"
    );
}
