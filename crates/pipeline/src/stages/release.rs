// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Release-as-evidence: fold a SIGNED full-release `gmeow.gts` (
//! CONSTITUTION.md §18).
//!
//! This is a STANDALONE fold — NOT a regenerate pipeline DAG stage. The
//! `release-bundle` CLI command reads the committed *unsigned* snapshot
//! (`generated/dist/gmeow.gts`, never mutated), augments it with
//!
//! 1. a `graph/attestations` named graph of `gmeow:Attestation` frames (one
//!    top-level release-manifest attestation over the bundle plus one child
//!    attestation per evidence artifact), and
//! 2. the evidence artifacts themselves as content-addressed report blobs,
//!
//! then signs the whole thing Ed25519 and writes the bytes to a SEPARATE
//! `--out` path. The attestations vouch that a given check RAN over given
//! bytes — never that the ontology is "true" (Principle 9).
//!
//! # Determinism (§18)
//!
//! The release timestamp is INJECTED (`issued_at`); the fold core never samples
//! a clock. Evidence inputs are sorted by content digest before minting, and the
//! attestation IRIs are derived from the content digest, so re-running with the
//! same inputs + same `issued_at` is byte-identical.
//!
//! # No-optionality (§18)
//!
//! The CLI reads every evidence file up front and hard-fails on a missing one;
//! this core never silently skips. Signing here is unconditional (the release
//! bundle is, by definition, signed): all three signer fields are passed to
//! [`crate::gts_profile::emit_gmeow_gts`], which itself hard-fails any partial
//! signing config.

use std::collections::BTreeSet;

use gmeow_errors::Diag;
use purrdf::gts::dataset_from_gts_graph;
use purrdf::gts::model::Graph;
use purrdf::gts::reader::read;
use purrdf::gts::writer::digest_string;
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};
#[cfg(test)]
use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, emit_gts};
use purrdf::{NativeRdfFormat, PROJECTION_CODECS, pair_loss_ledger, parse_dataset};

use crate::error::Release;

/// The named graph the release-manifest + per-artifact attestations ride in.
pub const GRAPH_ATTESTATIONS: &str = "https://blackcatinformatics.ca/gmeow/graph/attestations";

use gmeow_ns::GMEOW_NS;
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

/// The `rep` tag every release-evidence report blob carries, so a repo-free
/// consumer can recover each artifact by digest from this single channel.
const EVIDENCE_REP_PREFIX: &str = "release-evidence:";

/// One evidence artifact to fold into the release bundle.
///
/// The bytes are read by the (thin) CLI layer — a missing or unreadable file is
/// a hard failure there, never an `Option` skip here. `attestation_type_iri` is
/// the `gmeow:attestationType*` individual naming the KIND of check this artifact
/// records (e.g. `gmeow:attestationTypeConformanceVerdict`).
pub struct EvidenceInput {
    /// The decoded artifact bytes (the check result document).
    pub data: Vec<u8>,
    /// The artifact's declared media type (`gmeow:artifactMediaType`).
    pub media_type: String,
    /// The `gmeow:attestationType*` individual IRI for this evidence's KIND.
    pub attestation_type_iri: String,
    /// The blob `rep` discriminator (a short stable label, e.g. `cross-check`).
    pub rep: String,
    /// A human label recorded as the artifact's `rdfs:label` for listings.
    pub subject_label: String,
}

/// Fold release evidence into a SIGNED `gmeow.gts` bundle (§18).
///
/// `snapshot_bytes` is the committed unsigned snapshot; it is read back and
/// replayed faithfully (default graph + every named graph + the RDF 1.2
/// statement layer + every existing content-addressed blob) so the release
/// bundle's snapshot equals the committed snapshot content, PLUS the
/// `graph/attestations` named graph and the evidence blobs. The result is signed
/// with the supplied Ed25519 key material and returned as bytes — the caller
/// writes them to the `--out` path (NEVER over the committed snapshot).
#[allow(clippy::too_many_arguments)]
pub fn fold_release_bundle(
    snapshot_bytes: &[u8],
    evidence: Vec<EvidenceInput>,
    attester_iri: &str,
    issued_at: &str,
    release_subject_iri: &str,
    signer_secret: [u8; 32],
    signer_kid: &str,
    public_key_armor: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    // 1. Read the committed unsigned snapshot back into a folded graph and
    //    replay it into a fresh builder so we emit the SAME snapshot content.
    let graph = read(snapshot_bytes, true, None);
    let mut builder = SnapshotBuilder::new();
    replay_graph(&graph, &mut builder)?;

    // Re-add the snapshot's existing content-addressed blobs (decoded) as
    // doc_blobs so the release bundle carries the committed bundle's payloads.
    let doc_blobs = existing_blobs(&graph)?;

    // A8: auto-attest the packed documentation artifacts (the docs-book / docs-print
    // archives) carried by the committed snapshot. The blobs already ride in the bundle,
    // so this mints an `gmeow:AttestationArtifact` + blake3 `gmeow:contentDigest` per docs
    // archive WITHOUT re-folding the bytes (the dedup below suppresses the twin), and the
    // consumer half (`verify_release_bundle`) recomputes each attested digest against a
    // backing blob — so a drifted docs digest reds the gate.
    let mut evidence = evidence;
    evidence.extend(docs_artifact_evidence(&graph)?);

    // 2. Mint the attestation named graph. Sort evidence by content digest so the
    //    output is a pure function of the inputs (determinism, §18).
    let mut sorted: Vec<(String, EvidenceInput)> = evidence
        .into_iter()
        .map(|ev| (digest_string(&ev.data), ev))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.rep.cmp(&b.1.rep)));

    let attestations_nq =
        build_attestations_nquads(&sorted, attester_iri, issued_at, release_subject_iri);
    let att_dataset = parse_dataset(
        attestations_nq.as_bytes(),
        NativeRdfFormat::NQuads.media_type(),
        None,
    )
    .map_err(|e| {
        Diag::of_kind(Release {
            message: format!("parsing minted attestations graph: {e}"),
        })
    })?;
    builder.add_dataset(&att_dataset).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("adding minted attestations graph: {e}"),
        })
    })?;

    // 3. Fold each evidence artifact as a content-addressed report blob — but
    //    NEVER a second time for bytes the committed snapshot already carries.
    //    Some evidence (e.g. the SHACL/diagnostics SARIF) already rides in the
    //    snapshot as a report blob; re-folding it would emit a duplicate blob
    //    frame under a different `rep` for the same digest. The minted
    //    attestation binds its artifact to the bytes by `gmeow:contentDigest`
    //    only, which the committed blob already satisfies, so deduping here keeps
    //    exactly one blob frame AND one attestation envelope per artifact (GAP-2).
    let committed_digests: std::collections::HashSet<&str> =
        graph.blobs.iter().map(|(d, _)| d.as_str()).collect();
    let report_blobs: Vec<BlobRow> = sorted
        .iter()
        .filter(|(digest, _)| !committed_digests.contains(digest.as_str()))
        .map(|(_, ev)| BlobRow {
            data: ev.data.clone(),
            media_type: ev.media_type.clone(),
            rep: format!("{EVIDENCE_REP_PREFIX}{}", ev.rep),
        })
        .collect();

    // 4. Emit the signed bundle (emit_gts hard-fails any partial signer config).
    crate::gts_profile::emit_gmeow_gts(
        &builder,
        doc_blobs,
        report_blobs,
        Some(signer_secret),
        Some(signer_kid.to_string()),
        Some(public_key_armor.to_string()),
    )
    .map_err(|e| {
        Diag::of_kind(Release {
            message: format!("emitting signed release bundle: {e}"),
        })
    })
}

/// The `rep` discriminator the scoped coherence-certificate evidence rides under.
const COHERENCE_REP: &str = "coherence";

/// Build the scoped coherence-certificate evidence over `snapshot_bytes` — reason
/// over the bundle, build the [`gmeow_logic::certificate::CoherenceOutcome`], and emit it as N-Quads typed
/// `logic:CoherenceCertificate` / `logic:CoherenceCheckAttestation`. The result is
/// folded as one more signed evidence artifact, so the certificate rides the
/// bundle's Ed25519 signature (Principle 18) — there is NO new signing step and no
/// key handling here.
///
/// The release bundle is reasoned under classical native DL semantics, where a glut
/// is a forbidden integrity violation. `issued_at` is INJECTED (mirrors the release
/// timestamp) so the fold stays deterministic.
///
/// # Errors
/// Returns `Err` if the snapshot cannot be read, native reasoning fails, or coherence
/// is REFUSED — a bundle carrying a forbidden integrity violation must never be signed
/// as coherent (no-optionality / hard-fail).
/// Build the scoped [`CoherenceOutcome`](gmeow_logic::certificate::CoherenceOutcome) over
/// a `dataset` and its already-computed `bundle_hash` — the SINGLE certificate-construction
/// site the pipeline has. It resolves the governing contradiction policy from the dataset's
/// declared `logic:ReasoningContract`, pins the real per-axiom-bearing-graph digests, folds
/// the static projection-loss ledger, and runs the completeness gate. Both the release lane
/// ([`build_coherence_evidence`], over the serialized snapshot bytes) and the carrier spine
/// (over the assembled in-memory carrier) call THIS one function — the outcome construction
/// is never duplicated, only fed different bundle identities.
///
/// `result` is the reasoning result whose provenance the certificate summarizes; the caller
/// supplies it (the release lane reasons over the read-back bundle; the carrier reuses
/// `stage-reason`'s single pass — no second reasoning). `issued_at` is INJECTED so the fold
/// is deterministic.
///
/// The contradiction policy is READ from the bundle's declared `logic:ReasoningContract`
/// (`logic:admissibleValuation`), not pinned: no contract / no valuation ⇒ conservative
/// classical DEFAULT, multiple conflicting valuations ⇒ the MOST CONSERVATIVE governs, a
/// garbled valuation HARD-FAILS.
pub(crate) fn build_coherence_outcome(
    dataset: &purrdf::RdfDataset,
    result: &gmeow_logic::result::ReasoningResult,
    bundle_hash: String,
    issued_at: &str,
) -> gmeow_errors::Result<gmeow_logic::certificate::CoherenceOutcome> {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

    let policy = ContradictionPolicy::resolve_from_dataset(dataset).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("coherence certificate: contract resolution failed: {e}"),
        })
    })?;
    // Real per-axiom-bearing-graph digests, computed with the SAME digest primitive
    // as the bundle hash and sorted for determinism, so the certificate pins exactly
    // which axiom sets it ranged over. Shared with the validate `--deep` lane.
    let axiom_hashes = gmeow_logic::certificate::per_graph_axiom_hashes(dataset, digest_string);
    // Compute genuine projection-loss codes from the static loss ledger: for each
    // canonical projection target, fold `pair_loss_ledger("gts", to).entries()`
    // into a sorted set of unique loss codes. This is what actually belongs in
    // `projection_losses` — NOT the DL-reasoner's unsupported_constructs.
    let projection_loss_codes: BTreeSet<String> = PROJECTION_CODECS
        .iter()
        .flat_map(|&to| {
            pair_loss_ledger("gts", to)
                .entries()
                .iter()
                .map(|e| e.code.to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    CoherenceOutcome::from_reasoning_result(
        result,
        bundle_hash,
        axiom_hashes,
        policy,
        issued_at,
        projection_loss_codes,
    )
    .map_err(|e| {
        Diag::of_kind(Release {
            message: format!("coherence certificate: build failed: {e}"),
        })
    })
}

pub fn build_coherence_evidence(
    snapshot_bytes: &[u8],
    issued_at: &str,
) -> gmeow_errors::Result<EvidenceInput> {
    let bundle = purrdf::import_gts_events(snapshot_bytes).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("coherence certificate: GTS read error: {e}"),
        })
    })?;
    let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("coherence certificate: native reasoning failed: {e}"),
        })
    })?;
    // The release lane pins the certificate's bundle identity to the SERIALIZED snapshot
    // bytes it is folding over.
    let bundle_hash = digest_string(snapshot_bytes);
    let outcome =
        build_coherence_outcome(bundle.dataset.as_ref(), &result, bundle_hash, issued_at)?;
    if outcome.is_refused() {
        return Err(Diag::of_kind(Release {
            message: "coherence certificate: the bundle being released carries a forbidden \
                      integrity violation; an incoherent bundle must not be signed as coherent"
                .to_owned(),
        }));
    }
    let label = if outcome.issues_certificate() {
        "Scoped coherence certificate"
    } else {
        "Coherence check attestation"
    };
    Ok(EvidenceInput {
        data: outcome.to_nquads(GRAPH_ATTESTATIONS).into_bytes(),
        media_type: "application/n-quads".to_owned(),
        attestation_type_iri: resolve_attestation_type_iri("attestationTypeCoherenceCertificate"),
        rep: COHERENCE_REP.to_owned(),
        subject_label: label.to_owned(),
    })
}

/// Consumer-side outcome of verifying a signed release-evidence bundle (
/// §18). The `artifacts_verified` count is the number of per-artifact
/// attestations whose attested bytes were actually found in the bundle.
pub struct ReleaseVerifyReport {
    /// COSE_Sign1 frame signatures present.
    pub signed: usize,
    /// Signatures cryptographically valid under the resolved key.
    pub valid: usize,
    /// The signer key id recovered during verification.
    pub kid: Option<String>,
    /// Uppercase OpenPGP fingerprint of the resolved transport key.
    pub fingerprint: Option<String>,
    /// Per-artifact attestations whose `gmeow:contentDigest` blob is present.
    pub artifacts_verified: usize,
}

/// Verify a signed release bundle the way the §18 prose promises a consumer can:
/// not just the signature, but the *evidence*. This is the consumer half of the
/// fold and the body of `make verify-release`.
///
/// Three legs, all hard-failing (no silent skip):
/// 1. **Signature + trust policy** — native COSE_Sign1 verification against the
///    embedded `gts:transportKey` (or, when `expected_public_armor` is supplied,
///    that out-of-band trusted key). Subsumes `gts verify`.
/// 2. **Attestation frames** — the `graph/attestations` named graph must carry
///    the top-level release-manifest attestation and at least one per-artifact
///    `gmeow:contentDigest`.
/// 3. **Evidence presence** — every attested `gmeow:contentDigest` must resolve
///    to a blob actually carried by the bundle, so "which checks ran over which
///    bytes" is verifiable end to end.
pub fn verify_release_bundle(
    bundle_bytes: &[u8],
    expected_public_armor: Option<&str>,
) -> gmeow_errors::Result<ReleaseVerifyReport> {
    use purrdf::RdfTerm;
    use purrdf::gts::verify::{VerifyOptions, verify_file_with_options};

    // --- 1. Cryptographic signature + trust policy (native, subsumes gts verify).
    let mut opts = VerifyOptions::default().require_signatures(true);
    if let Some(armor) = expected_public_armor {
        opts = opts.with_armored_key(armor);
    }
    let result = verify_file_with_options(bundle_bytes, &opts);
    if !result.ok || result.valid == 0 {
        let detail = if result.errors.is_empty() {
            "no cryptographically valid, trusted signature".to_string()
        } else {
            result.errors.join("; ")
        };
        return Err(Diag::of_kind(Release {
            message: format!("release bundle signature/trust verification failed: {detail}"),
        }));
    }

    // --- 2 + 3. Walk the attestation frames and confirm each attested digest is
    //            backed by a blob actually present in the bundle.
    let graph = read(bundle_bytes, true, None);
    let dataset = dataset_from_gts_graph(&graph).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("folding bundle into a dataset for the attestation walk: {e}"),
        })
    })?;

    let content_digest_pred = format!("{GMEOW_NS}contentDigest");
    let attestation_type_pred = format!("{GMEOW_NS}attestationType");
    let manifest_type = format!("{GMEOW_NS}attestationTypeReleaseManifest");

    let mut saw_manifest = false;
    let mut digests: Vec<String> = Vec::new();
    for q in dataset.owned_quads() {
        let in_attestations = matches!(
            &q.graph_name,
            Some(RdfTerm::Iri(g)) if g == GRAPH_ATTESTATIONS
        );
        if !in_attestations {
            continue;
        }
        if q.predicate == attestation_type_pred {
            if let RdfTerm::Iri(o) = &q.object
                && o == &manifest_type
            {
                saw_manifest = true;
            }
        } else if q.predicate == content_digest_pred
            && let RdfTerm::Literal(lit) = &q.object
        {
            digests.push(lit.lexical_form.clone());
        }
    }

    if !saw_manifest {
        return Err(Diag::of_kind(Release {
            message: "release bundle graph/attestations carries no release-manifest attestation"
                .to_owned(),
        }));
    }
    if digests.is_empty() {
        return Err(Diag::of_kind(Release {
            message: "release bundle carries no per-artifact gmeow:contentDigest attestation"
                .to_owned(),
        }));
    }

    let mut artifacts_verified = 0usize;
    for digest in &digests {
        if graph.blob_entry(digest).is_none() {
            return Err(Diag::of_kind(Release {
                message: format!(
                    "attested artifact {digest} has no backing blob in the bundle \
                     (attestation references bytes that are not present)"
                ),
            }));
        }
        artifacts_verified += 1;
    }

    Ok(ReleaseVerifyReport {
        signed: result.signed,
        valid: result.valid,
        kid: result.kid,
        fingerprint: result.fingerprint,
        artifacts_verified,
    })
}

/// Replay a folded [`Graph`] into a fresh [`SnapshotBuilder`].
///
/// The committed snapshot is multi-named-graph and may carry an RDF 1.2 statement
/// layer. We fold the GTS graph straight into a native
/// [`RdfDataset`](purrdf::RdfDataset) via the oxigraph-free container→dataset
/// bridge ([`dataset_from_gts_graph`]) — no codec text in the middle. The bridge
/// re-binds the `rdf:reifies` statement layer into the dataset's reifier/annotation
/// side-tables AND preserves named graphs on the base quads, so a single
/// [`SnapshotBuilder::add_dataset`] rebuilds the base quads (with their graph names)
/// and the reifies/annot tables exactly. This is the lossless inverse of the old
/// `to_nquads(graph)` + re-parse round-trip, so the emitted snapshot is byte-identical.
///
/// Determinism (§18): the bridge interns terms directly from the GTS graph's own
/// content-canonical term table (the reader yields it in a process-independent
/// `(kind, value, datatype, lang)` order), and `SnapshotBuilder::canonical_tables`
/// re-ids by that same content sort key. The ingestion order is therefore a pure
/// function of the quad SET, never of any hash-seeded iteration order — exactly the
/// property the old N-Quads line-sort pinned, now intrinsic to the native fold.
fn replay_graph(graph: &Graph, builder: &mut SnapshotBuilder) -> gmeow_errors::Result<()> {
    let dataset = dataset_from_gts_graph(graph).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("folding committed snapshot into a dataset: {e}"),
        })
    })?;
    // `add_dataset` is a no-op for a wholly empty dataset, so no early-return guard
    // is needed: an empty snapshot contributes no base quads, reifiers, or annotations.
    builder.add_dataset(&dataset).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("replaying committed snapshot into the release builder: {e}"),
        })
    })?;
    Ok(())
}

/// The `gmeow:attestationType*` local name every packed-docs artifact attestation
/// carries (A8): a documentation-artifact byte-identity vouch.
const DOCS_ATTESTATION_TYPE: &str = "attestationTypeDocumentationArtifact";

/// Build one [`EvidenceInput`] per packed documentation artifact (the `docs-book` and
/// `docs-print` archives) carried by the committed snapshot, so the release fold mints a
/// `gmeow:AttestationArtifact` + blake3 `gmeow:contentDigest` binding each docs archive
/// to its bytes. The bytes already ride in the bundle (the dedup in
/// [`fold_release_bundle`] suppresses a twin blob frame), and the digest the attestation
/// records is exactly the blob's own content address, so [`verify_release_bundle`]'s
/// evidence-presence leg verifies the docs artifacts end to end. A missing docs blob is
/// NOT synthesized — a snapshot without the docs archives simply attests none (the docs
/// blobs are always present in a real regenerated bundle).
fn docs_artifact_evidence(graph: &Graph) -> gmeow_errors::Result<Vec<EvidenceInput>> {
    use crate::bundle_blobs::{REP_DOCS_BOOK, REP_DOCS_PRINT};
    let mut rows: Vec<EvidenceInput> = Vec::new();
    for (rep, label) in [
        (REP_DOCS_BOOK, "Documentation book archive"),
        (REP_DOCS_PRINT, "Documentation print archive"),
    ] {
        // Find the committed blob whose declared `rep` is this docs archive.
        let hit = graph
            .blobs
            .iter()
            .find(|(digest, _)| matches!(blob_meta_for(graph, digest), Ok((_, r)) if r == rep));
        let Some((digest, entry)) = hit else {
            continue;
        };
        let data = entry.decoded_vec().map_err(|e| {
            Diag::of_kind(Release {
                message: format!("decoding committed docs blob {digest} for attestation: {e}"),
            })
        })?;
        let (media_type, _) = blob_meta_for(graph, digest)?;
        if rep == REP_DOCS_PRINT {
            // G3: the print archive vouches for the whole tar; separately bind the
            // COMPILED PDF's own bytes under the exact media type a consumer expects.
            rows.push(docs_print_pdf_evidence(&data)?);
        }
        rows.push(EvidenceInput {
            data,
            media_type,
            attestation_type_iri: format!("{GMEOW_NS}{DOCS_ATTESTATION_TYPE}"),
            rep: format!("{rep}-attestation"),
            subject_label: label.to_owned(),
        });
    }
    Ok(rows)
}

/// Extract the byte-reproducible `gmeow.pdf` member from the decoded `docs-print`
/// tar and mint it as its own `application/pdf` [`EvidenceInput`] (G3). The
/// archive-level attestation binds the WHOLE print-docs tar (`application/x-tar`,
/// PDF + Typst source together); this one binds the compiled PDF's OWN bytes to a
/// blake3 `gmeow:contentDigest` under `application/pdf`, so
/// [`verify_release_bundle`]'s evidence-presence leg can verify the compiled PDF
/// end to end, independent of the archive framing. Since the extracted PDF bytes
/// are (by construction) never byte-identical to the enclosing tar, this artifact
/// is never deduped against the committed docs-print blob — `fold_release_bundle`
/// folds it as its own report blob, exactly like any other minted evidence. Hard
/// fails if the print-docs tar carries no `gmeow.pdf` member — a docs-print
/// archive without its PDF is a corrupt build, never silently skipped.
fn docs_print_pdf_evidence(tar_bytes: &[u8]) -> gmeow_errors::Result<EvidenceInput> {
    use crate::bundle_blobs::REP_DOCS_PRINT;
    let members = purrdf::ustar::read_archive(tar_bytes).map_err(|e| {
        Diag::of_kind(Release {
            message: format!("untarring docs-print for the PDF attestation: {e}"),
        })
    })?;
    let Some((_, pdf_bytes)) = members
        .into_iter()
        .find(|(name, _)| name.ends_with("gmeow.pdf"))
    else {
        return Err(Diag::of_kind(Release {
            message: "docs-print archive carries no gmeow.pdf member (corrupt docs-print build)"
                .to_owned(),
        }));
    };
    Ok(EvidenceInput {
        data: pdf_bytes,
        media_type: "application/pdf".to_owned(),
        attestation_type_iri: format!("{GMEOW_NS}{DOCS_ATTESTATION_TYPE}"),
        rep: format!("{REP_DOCS_PRINT}-pdf-attestation"),
        subject_label: "Documentation PDF".to_owned(),
    })
}

/// Decode every existing snapshot blob into a [`BlobRow`], preserving the
/// declared media type + `rep`. Hard-fails a lazy blob that cannot decode (a
/// damaged committed snapshot is a hard build failure, never a silent drop).
fn existing_blobs(graph: &Graph) -> gmeow_errors::Result<Vec<BlobRow>> {
    let mut rows = Vec::with_capacity(graph.blobs.len());
    for (digest, entry) in &graph.blobs {
        let data = entry.decoded_vec().map_err(|e| {
            Diag::of_kind(Release {
                message: format!("decoding committed snapshot blob {digest}: {e}"),
            })
        })?;
        let (media_type, rep) = blob_meta_for(graph, digest)?;
        rows.push(BlobRow {
            data,
            media_type,
            rep,
        });
    }
    Ok(rows)
}

/// Recover a blob's declared `(media_type, rep)` from the folded `blob_meta`
/// table. A blob frame's `pub` map carries `mt` + `rep`; both are required for a
/// committed snapshot blob, so a missing table entry or a missing `mt`/`rep` is
/// a hard failure (never a silent `application/octet-stream`/empty-`rep` default
/// that would lose the blob's declared identity on re-emit). A committed,
/// drift-gated snapshot always carries this metadata; its absence means a
/// corrupt snapshot, which must stop the release fold, not be papered over.
fn blob_meta_for(graph: &Graph, digest: &str) -> gmeow_errors::Result<(String, String)> {
    use ciborium::value::Value;
    let Some(Value::Map(entries)) = graph
        .blob_meta
        .iter()
        .find(|(d, _)| d == digest)
        .map(|(_, v)| v)
    else {
        return Err(Diag::of_kind(Release {
            message: format!(
                "committed snapshot blob {digest} has no blob_meta entry (corrupt snapshot)"
            ),
        }));
    };
    let mut media_type: Option<String> = None;
    let mut rep: Option<String> = None;
    for (k, v) in entries {
        if let (Value::Text(key), Value::Text(val)) = (k, v) {
            match key.as_str() {
                "mt" => media_type = Some(val.clone()),
                "rep" => rep = Some(val.clone()),
                _ => {}
            }
        }
    }
    match (media_type, rep) {
        (Some(mt), Some(rep)) => Ok((mt, rep)),
        (mt, rep) => Err(Diag::of_kind(Release {
            message: format!(
                "committed snapshot blob {digest} blob_meta missing {} (corrupt snapshot)",
                match (mt.is_none(), rep.is_none()) {
                    (true, true) => "both `mt` and `rep`",
                    (true, false) => "`mt`",
                    _ => "`rep`",
                }
            ),
        })),
    }
}

/// Author the `graph/attestations` named graph as N-Quads text.
///
/// One top-level release-manifest attestation over `release_subject_iri`
/// (`gmeow:attestationTypeReleaseManifest` + `gmeow:attestationTypeSignedRDF`),
/// plus one child attestation + artifact per evidence input, each bound to its
/// blob by `gmeow:contentDigest`. Every row carries the `GRAPH_ATTESTATIONS`
/// graph name. IRIs are derived from the content digest, so the output is stable
/// across runs with the same inputs (mirrors the worked example shape).
fn build_attestations_nquads(
    sorted: &[(String, EvidenceInput)],
    attester_iri: &str,
    issued_at: &str,
    release_subject_iri: &str,
) -> String {
    let g = format!("<{GRAPH_ATTESTATIONS}>");
    let mut lines: Vec<String> = Vec::new();

    let mut quad = |s: &str, p: &str, o: &str| {
        lines.push(format!("{s} {p} {o} {g} ."));
    };

    // Every minted attestation subject is generated A-Box instance data folded
    // into `graph/attestations`, not vocabulary surface: tag each typed subject
    // with a human label, its named-graph provenance anchor, and the assertional
    // `gmeow:boxABox` role so the bundle satisfies the assertional-tier
    // validation contract (no `skos:definition`).
    let isdefinedby = iri(GRAPH_ATTESTATIONS);
    let abox_role = gmeow("boxABox");
    let box_role_pred = gmeow("graphBoxRole");

    // The attester is a software agent (the full-release lane).
    let attester = iri(attester_iri);
    quad(
        &attester,
        &iri(RDF_TYPE),
        &iri(&format!("{GMEOW_NS}SoftwareAgent")),
    );
    quad(&attester, &iri(RDFS_LABEL), &literal("Release attester"));
    quad(&attester, &iri(RDFS_IS_DEFINED_BY), &isdefinedby);
    quad(&attester, &box_role_pred, &abox_role);

    // --- Top-level release-manifest attestation over the whole bundle. --------
    let manifest = iri(&format!(
        "{release_subject_iri}/attestation/release-manifest"
    ));
    quad(
        &manifest,
        &iri(RDF_TYPE),
        &iri(&format!("{GMEOW_NS}Attestation")),
    );
    quad(
        &manifest,
        &iri(RDFS_LABEL),
        &literal("Release manifest attestation"),
    );
    quad(&manifest, &iri(RDFS_IS_DEFINED_BY), &isdefinedby);
    quad(&manifest, &box_role_pred, &abox_role);
    quad(&manifest, &gmeow("attester"), &attester);
    quad(
        &manifest,
        &gmeow("attestedSubject"),
        &iri(release_subject_iri),
    );
    quad(
        &manifest,
        &gmeow("attestationType"),
        &iri(&format!("{GMEOW_NS}attestationTypeReleaseManifest")),
    );
    quad(
        &manifest,
        &gmeow("attestationType"),
        &iri(&format!("{GMEOW_NS}attestationTypeSignedRDF")),
    );
    quad(&manifest, &gmeow("issuedAt"), &dt(issued_at));

    // --- One child attestation + artifact per evidence input. -----------------
    for (digest, ev) in sorted {
        // Content-derived IRIs: stable across runs for identical bytes.
        let key = digest_iri_suffix(digest);
        let attestation = iri(&format!("{release_subject_iri}/attestation/{key}"));
        let artifact = iri(&format!("{release_subject_iri}/artifact/{key}"));

        quad(
            &artifact,
            &iri(RDF_TYPE),
            &iri(&format!("{GMEOW_NS}AttestationArtifact")),
        );
        quad(
            &artifact,
            &gmeow("artifactMediaType"),
            &literal(&ev.media_type),
        );
        quad(&artifact, &gmeow("contentDigest"), &literal(digest));
        // Always carry a label: the evidence's own subject label when present,
        // else a content-derived fallback, so the artifact is never an
        // under-specified A-Box subject.
        let artifact_label = if ev.subject_label.is_empty() {
            format!("Attestation artifact {key}")
        } else {
            ev.subject_label.clone()
        };
        quad(&artifact, &iri(RDFS_LABEL), &literal(&artifact_label));
        quad(&artifact, &iri(RDFS_IS_DEFINED_BY), &isdefinedby);
        quad(&artifact, &box_role_pred, &abox_role);

        quad(
            &attestation,
            &iri(RDF_TYPE),
            &iri(&format!("{GMEOW_NS}Attestation")),
        );
        quad(
            &attestation,
            &iri(RDFS_LABEL),
            &literal(&format!("Release evidence attestation {key}")),
        );
        quad(&attestation, &iri(RDFS_IS_DEFINED_BY), &isdefinedby);
        quad(&attestation, &box_role_pred, &abox_role);
        quad(&attestation, &gmeow("attester"), &attester);
        quad(
            &attestation,
            &gmeow("attestedSubject"),
            &iri(release_subject_iri),
        );
        quad(
            &attestation,
            &gmeow("attestationType"),
            &iri(&resolve_attestation_type_iri(&ev.attestation_type_iri)),
        );
        quad(&attestation, &gmeow("issuedAt"), &dt(issued_at));
        quad(&attestation, &gmeow("attestationArtifact"), &artifact);
    }

    if lines.is_empty() {
        String::new()
    } else {
        // Sort for byte-stability independent of authoring order; the builder
        // re-sorts by content anyway, but a stable text keeps the parse cheap.
        lines.sort();
        format!("{}\n", lines.join("\n"))
    }
}

/// Resolve an evidence row's `attestation_type` into a full IRI. A value that is
/// already absolute (`http://…`/`https://…`) is used verbatim; a bare local name
/// (e.g. `attestationTypeQualityReport`) is expanded against the gmeow namespace.
/// The colon-delimited `--evidence` CLI spec (`path:media_type:type:rep:label`)
/// cannot carry an absolute IRI without its `https:` colliding with a separator,
/// so the Makefile passes the bare local name and this expands it.
fn resolve_attestation_type_iri(value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        format!("{GMEOW_NS}{value}")
    }
}

/// The IRI suffix derived from a `blake3:<hex>` digest (the hex, no scheme).
fn digest_iri_suffix(digest: &str) -> String {
    digest.strip_prefix("blake3:").unwrap_or(digest).to_string()
}

fn iri(s: &str) -> String {
    format!("<{s}>")
}

fn gmeow(local: &str) -> String {
    format!("<{GMEOW_NS}{local}>")
}

/// Escape a literal lexical form for N-Triples (the `gmeow-gts` escaper is
/// `pub(crate)`, so we mirror it here for the minted attestation literals).
fn escape_literal(lex: &str) -> String {
    let mut out = String::with_capacity(lex.len());
    for ch in lex.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn literal(lex: &str) -> String {
    format!("\"{}\"", escape_literal(lex))
}

fn dt(lex: &str) -> String {
    format!("\"{}\"^^<{XSD_DATETIME}>", escape_literal(lex))
}

#[cfg(test)]
mod tests {
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
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
        format!(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{wrapped}-----END PGP PUBLIC KEY BLOCK-----\n"
        )
    }

    /// A tiny unsigned `dist` snapshot to act as the "committed" base.
    fn tiny_snapshot() -> Vec<u8> {
        let nq = "<https://e/s> <https://e/p> <https://e/o> .\n\
                  <https://e/s> <https://e/q> \"hello\" .\n";
        let b = builder_from(nq, NativeRdfFormat::NTriples.media_type());
        emit_gts(
            &b,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
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

    fn fold(snapshot: &[u8], evidence: Vec<EvidenceInput>, issued_at: &str) -> Vec<u8> {
        let signing = deterministic_signing_key(7);
        let secret = signing.to_bytes();
        let kid = "release-test-kid";
        let armor = fake_public_armor(&signing.verifying_key().to_bytes());
        fold_release_bundle(
            snapshot,
            evidence,
            "https://blackcatinformatics.ca/gmeow/agent/release-lane",
            issued_at,
            "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
            secret,
            kid,
            &armor,
        )
        .expect("fold release bundle")
    }

    #[test]
    fn build_coherence_evidence_emits_a_coherence_artifact() {
        let snapshot = tiny_snapshot();
        let evidence = build_coherence_evidence(&snapshot, "2026-06-28T00:00:00Z")
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
        let again = build_coherence_evidence(&snapshot, "2026-06-28T00:00:00Z").unwrap();
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
            evidence.push(build_coherence_evidence(&snapshot, "2026-06-28T00:00:00Z").unwrap());
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
            .filter(|e| {
                e.contains("/attestation/") || e.contains("/artifact/") || e.contains("agent/")
            })
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

    /// GAP-4/8: the consumer verify must accept a well-formed signed bundle —
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

    /// GAP-4: a tampered bundle (a flipped byte) must fail the signature leg, and
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

    /// GAP-8: supplying the WRONG out-of-band trusted key must fail the trust
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
    fn snapshot_with_report_blob(data: &[u8], rep: &str) -> Vec<u8> {
        let nq = "<https://e/s> <https://e/p> <https://e/o> .\n";
        let b = builder_from(nq, NativeRdfFormat::NTriples.media_type());
        emit_gts(
            &b,
            "dist",
            None,
            Vec::new(),
            vec![BlobRow {
                data: data.to_vec(),
                media_type: "application/json".to_string(),
                rep: rep.to_string(),
            }],
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
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

    /// GAP-2: evidence whose bytes already ride in the committed snapshot must NOT
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
    fn snapshot_from_nquads(nq: &str) -> Vec<u8> {
        let b = builder_from(nq, NativeRdfFormat::NQuads.media_type());
        emit_gts(
            &b,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
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
        let base = read(&snapshot, true, None);
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
        let snapshot = emit_gts(
            &b,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
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
    fn docs_snapshot() -> Vec<u8> {
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
        emit_gts(
            &b,
            "dist",
            None,
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
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
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
            let graph = read(&snapshot, true, None);
            let (digest, _) = graph
                .blobs
                .iter()
                .find(
                    |(d, _)| matches!(blob_meta_for(&graph, d), Ok((_, r)) if r == REP_DOCS_PRINT),
                )
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
}
