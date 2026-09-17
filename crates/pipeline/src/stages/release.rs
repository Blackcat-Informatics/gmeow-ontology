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
//! [`gmeow_gts_profile::emit_gmeow_gts`], which itself hard-fails any partial
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

/// The named graph the release-manifest + per-artifact attestations ride in. DEFINED
/// ONCE in [`gmeow_bundle_view::graph_iris`] — a release verifier reads this graph
/// back out of the signed bundle, so signer and verifier share ONE constant.
pub use gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;

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

/// One release input admitted against its complete mandatory ingestion companion.
/// The immutable borrow keeps the selected bytes fixed through the fold; no second
/// whole-bundle hash is required merely to defend against argument substitution.
pub struct ReleaseSource<'a> {
    bytes: &'a [u8],
    receipt: gmeow_gts_profile::GmeowGtsSourceReceipt,
}

impl<'a> ReleaseSource<'a> {
    /// Admit the exact source output and retain its complete receipt chain.
    ///
    /// # Errors
    /// Rejects missing, malformed, oversized, or wrong-output receipt evidence.
    pub fn admit(bytes: &'a [u8], receipt: &[u8]) -> gmeow_errors::Result<Self> {
        Ok(Self {
            bytes,
            receipt: gmeow_gts_profile::GmeowGtsSourceReceipt::admit(bytes, receipt)?,
        })
    }
}

/// Fold release evidence into a SIGNED `gmeow.gts` bundle (§18).
///
/// `source` binds the committed snapshot to its verified input receipt; it is read back and
/// replayed faithfully (default graph + every named graph + the RDF 1.2
/// statement layer + every existing content-addressed blob) so the release
/// bundle's snapshot equals the committed snapshot content, PLUS the
/// `graph/attestations` named graph and the evidence blobs. The result is signed
/// with the supplied Ed25519 key material. The returned emission retains its
/// exact current native report and complete original receipt chain; the caller
/// publishes both bytes and the outside-payload companion.
#[allow(clippy::too_many_arguments)]
pub fn fold_release_bundle(
    source: ReleaseSource<'_>,
    evidence: Vec<EvidenceInput>,
    attester_iri: &str,
    issued_at: &str,
    release_subject_iri: &str,
    signer_secret: [u8; 32],
    signer_kid: &str,
    public_key_armor: &str,
) -> gmeow_errors::Result<gmeow_gts_profile::GmeowGtsEmission> {
    // 1. Read the committed unsigned snapshot back into a folded graph and
    //    replay it into a fresh builder so we emit the SAME snapshot content.
    let ReleaseSource {
        bytes: snapshot_bytes,
        receipt: source_receipt,
    } = source;
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
    let _ingestion = builder.add_view(&att_dataset).map_err(|e| {
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
    //    exactly one blob frame AND one attestation envelope per artifact.
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

    // 4. Emit under one complete signing selection and retain the source receipt.
    let mut emission = gmeow_gts_profile::emit_gmeow_gts(
        builder,
        doc_blobs,
        report_blobs,
        Some(gmeow_gts_profile::GmeowGtsSigning {
            secret: signer_secret,
            key_id: signer_kid.to_owned(),
            public_key_armor: public_key_armor.to_owned(),
        }),
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .map_err(|e| {
        Diag::of_kind(Release {
            message: format!("emitting signed release bundle: {e}"),
        })
    })?;
    emission.source_receipts.push(source_receipt);
    Ok(emission)
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
    let edb = gmeow_logic::reasoning_graphs::project_object_level_edb(bundle.dataset.as_ref())?;
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(&edb)?,
        &gmeow_logic::reasoning_graphs::object_level_domains()?,
    )
    .map_err(|e| {
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
    let _ingestion = builder.add_view(&dataset).map_err(|e| {
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

#[path = "release.tests.rs"]
#[cfg(test)]
mod tests;
