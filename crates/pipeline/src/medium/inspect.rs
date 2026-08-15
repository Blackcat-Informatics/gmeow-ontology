// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The CONSUMER-facing read of the medium axis: what `gmeow medium list|verify|explain`,
//! the `gmeow://ontology/medium` MCP resource, and `gmeow-dev medium-gate` all report.
//!
//! Every function here answers a question about ONE artifact from that artifact's own
//! bytes. The registry, the realizations, the envelopes and the measured two-part codes
//! are read back out of the graphs the bundle SHIPS — never recomputed, never re-trained,
//! never re-measured. A consumer holding only `gmeow.gts` has no corpus to re-resolve and
//! no trainer to re-run, so a surface that re-derived any of this would be answering a
//! different question from the one the shipped bytes settle.
//!
//! # Why this is one module rather than two CLI implementations
//!
//! `gmeow medium verify` (the consumer verb) and `gmeow-dev medium-gate` (the repo gate)
//! make overlapping claims about the same artifact. Written twice they would drift, and
//! the drift would be invisible: both would pass, on different definitions of "verified".
//! [`verify`] is therefore the shared core and [`gate`] is exactly that core plus the
//! clauses a gate can additionally afford — the measured MDL win, registry completeness,
//! and the declared-vs-actual reader-capability comparison.
//!
//! # The decode is real
//!
//! purrdf's reader is TOTAL (GTS spec §7.6) and stores a blob frame LAZILY: it accepts
//! the frame's in-band `pub.digest` on the frame's word and never decompresses the
//! payload unless something asks for it. So "the bundle folded without diagnostics" does
//! NOT establish that a single blob frame is decodable, let alone that its bytes are the
//! bytes it claims. [`verify`] therefore decodes every payload frame through the codec
//! chain the segment header declares, primed with the dictionary that header pins, and
//! re-derives the digest. A frame that will not decode is reported as
//! [`crate::error::MediumOpaqueFrame`] — that IS what a reader gets when it finally
//! reaches for the blob — and one that decodes to different bytes is
//! [`crate::error::MediumDigestMismatch`]. Neither ever falls back to a dictionary-less
//! decode: a payload written through a primed medium is not readable at lower fidelity
//! without its dictionary, it is not readable AT ALL.

use std::collections::{BTreeMap, BTreeSet};

use super::audit::{
    self, DIST_BUNDLE_PRODUCER, MediumDeclaration, declared_medium_of, validate_declared_media,
};
use super::envelope::{DigestStratum, MediumEnvelope, ReaderCapabilities};
use super::measure::DictionaryEffect;
use super::registry::{
    DictSelection, DictionaryStrategy, MediumRegistry, MediumSourceKind, RepAssignment,
};
use super::{
    MEDIUM_REGISTRY_GRAPH, SNAPSHOT_WIRE_REP, blake3_digest, digest_mismatch, invalid_declaration,
    opaque_frame, undeclared_dictionary, unknown_dictionary,
};

/// The reader capabilities THIS build holds: it links purrdf, so it can apply
/// rsyncable framing and prime a decode with a dictionary.
///
/// Spelled as data rather than assumed, because [`super::envelope::open`] compares a
/// medium's `gmeow:requiresReaderCapability` against what the reader actually holds, and
/// a reader that silently claimed every capability would turn that comparison into a
/// tautology.
#[must_use]
pub fn native_reader_capabilities() -> ReaderCapabilities {
    ["zstd-dictionary", "zstd-rsyncable"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// One declared `gmeow:Medium`, flattened for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediumRow {
    /// The medium individual's IRI.
    pub iri: String,
    /// `gmeow:mediumCodec`, as the wire name the artifact actually carries.
    pub codec: String,
    /// `gmeow:mediumZstdLevel`.
    pub zstd_level: i32,
    /// `gmeow:mediumSourceKind` — which audit branch governs artifacts written
    /// through it.
    pub source_kind: MediumSourceKind,
    /// The `gmeow:dictionaryId`s of its `gmeow:mediumDictionary` bound, in canonical
    /// order. Empty is the explicit no-dictionary SELECTION, never an omission.
    pub dictionaries: Vec<String>,
    /// `gmeow:requiresReaderCapability` — the reader contract this medium raises.
    pub reader_capabilities: BTreeSet<String>,
}

/// One shipped dictionary: its AUTHORED declaration joined to the GENERATED
/// realization the same artifact carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryRow {
    /// `gmeow:dictionaryId` — the name a frame's catalog entry cites.
    pub id: String,
    /// The authored `gmeow:CompressionDictionary` IRI.
    pub iri: String,
    /// `gmeow:dictionaryVersion`.
    pub version: String,
    /// The AUTHORED `gmeow:dictionaryStrategy`.
    pub strategy: DictionaryStrategy,
    /// The MEASURED `gmeow:measuredDictionaryStrategy` — what the trainer actually ran.
    pub measured_strategy: DictionaryStrategy,
    /// The authored `gmeow:dictionaryTargetLength`.
    pub target_length: usize,
    /// The declared `gmeow:DictionaryCorpus` IRI.
    pub corpus: String,
    /// `gmeow:dictionaryContentDigest`, canonical `blake3:<64 lowercase hex>`.
    pub content_digest: String,
    /// `gmeow:dictionaryByteLength` — what the trainer actually returned.
    pub byte_length: usize,
    /// `gmeow:zstdDictionaryId` — the `Dictionary_ID` every primed frame cites.
    pub zstd_dictionary_id: u32,
    /// `gmeow:measuredCorpusSampleCount` for the build that emitted the artifact.
    pub corpus_sample_count: u64,
    /// The byte length of the entry the artifact's own segment header pins under this
    /// id, when it pins one. A dictionary declared but not pinned is nameable and
    /// unobtainable.
    pub in_band_bytes: Option<usize>,
    /// The blob representations a registered `gmeow:PayloadSchema` primes with it, in
    /// canonical order.
    pub primes: Vec<String>,
}

/// One row of the TOTAL rep→medium assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentRow {
    /// The `gmeow:payloadSchemaId` wire label.
    pub rep: String,
    /// The `gmeow:Medium` the rep's payloads are written through.
    pub medium: String,
    /// The `gmeow:dictionaryId` the rep primes with — `None` is the declared
    /// no-dictionary selection of a medium whose bound is empty.
    pub dictionary: Option<String>,
}

/// Everything `gmeow medium list` reports, read from ONE artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediumInventory {
    /// Every declared `gmeow:Medium`, by IRI order.
    pub media: Vec<MediumRow>,
    /// Every declared dictionary joined to its realization.
    pub dictionaries: Vec<DictionaryRow>,
    /// The TOTAL rep→medium assignment.
    pub assignment: Vec<AssignmentRow>,
    /// How many `gmeow:MediumEnvelope` records the artifact carries.
    pub envelope_count: usize,
    /// How many payload-bearing frames the artifact's wire actually carries. Equal to
    /// [`Self::envelope_count`] on a self-describing artifact — one envelope per frame
    /// is the whole claim.
    pub payload_frame_count: usize,
    /// The `(dictionary id, byte length)` pairs the artifact pins in band, in the order
    /// its segment headers pin them.
    pub pinned: Vec<(String, usize)>,
}

/// How an artifact's audit branch is DERIVED from the artifact itself.
///
/// A total, three-way classification with no residual arm: the audit is a split on
/// `gmeow:mediumSourceKind`, and a split is a total function only when every artifact is
/// in its domain. An artifact that matched none of these would be audited by nothing
/// while the split still looked complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactClass {
    /// The artifact folds to its OWN `graph/medium-registry` and declares the terminal
    /// producer's `gmeow:producerMedium`: the distribution bundle. Audited per-rep
    /// against itself.
    SelfDescribing,
    /// No registry of its own, but its segment headers pin dictionaries in band: a
    /// runtime store. Audited against the bundle whose dictionaries primed it.
    HeaderDict,
    /// No registry and no pinned dictionary: one medium governs the whole artifact.
    WholeArtifact,
}

impl ArtifactClass {
    /// The `gmeow:MediumSourceKind` this class is audited under.
    #[must_use]
    pub fn source_kind(self) -> MediumSourceKind {
        match self {
            Self::SelfDescribing => MediumSourceKind::PerRep,
            Self::HeaderDict => MediumSourceKind::HeaderDict,
            Self::WholeArtifact => MediumSourceKind::WholeArtifact,
        }
    }
}

/// One payload frame, after it was actually decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFrame {
    /// Byte offset of the frame's CBOR item.
    pub offset: usize,
    /// The rep the frame is registered under (`gmeow:snapshot/wire` for the one frame
    /// that carries no public metadata).
    pub rep: String,
    /// The `gmeow:dictionaryId` the frame's catalog entry binds, when it is primed.
    pub dictionary: Option<String>,
    /// The frame's in-band `pub.digest`, when it states one.
    pub declared_digest: Option<String>,
    /// The decoded payload's byte length.
    pub decoded_bytes: usize,
}

/// The result of a successful [`verify`] / [`gate`] — what the artifact was proved to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediumVerification {
    /// The branch the artifact was audited under, derived from the artifact.
    pub class: ArtifactClass,
    /// The `gmeow:Medium` the artifact was audited against.
    pub medium: String,
    /// Every payload frame, decoded.
    pub frames: Vec<VerifiedFrame>,
    /// How many `gmeow:MediumEnvelope` records were opened and re-derived.
    pub envelopes_verified: usize,
    /// The reader capabilities the artifact's wire actually demands.
    pub actual_capabilities: BTreeSet<String>,
    /// The reader capabilities its declared medium says it demands.
    pub declared_capabilities: BTreeSet<String>,
    /// The `gmeow:dictionaryId`s that actually prime a frame of this artifact.
    pub dictionaries: BTreeSet<String>,
}

/// Everything `gmeow medium explain <dict-id>` reports about one shipped dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryExplanation {
    /// The dictionary's declaration joined to its realization.
    pub row: DictionaryRow,
    /// The rendered `gmeow:DictionaryCorpus` selectors, in canonical order.
    pub selectors: Vec<String>,
    /// Its measured two-part codes, one per declared population.
    pub effects: Vec<DictionaryEffect>,
}

/// Fold an artifact and read its medium registry back out of it.
///
/// # Errors
/// The artifact does not fold, or its registry carries a declaration defect.
pub fn registry_of(bytes: &[u8]) -> Result<MediumRegistry, gmeow_errors::Diag> {
    MediumRegistry::from_dataset(audit::fold_leniently(bytes)?.as_ref())
}

/// The bundle whose dictionaries primed an artifact that carries no registry of its own.
///
/// A named type rather than a bare second `&[u8]` parameter: the artifact under audit and
/// the bundle it is audited against are both GTS byte slices, both plausible at either
/// position, and transposing them would audit the bundle against the store — which would
/// pass, for the wrong reason, on a healthy pair.
///
/// It carries BYTES rather than a built [`MediumRegistry`] so the fold stays lazy: a
/// self-describing artifact is audited against the registry it carries, and folding a
/// second thirty-megabyte bundle to build a registry nothing then reads is work with no
/// claim attached.
#[derive(Debug, Clone, Copy)]
pub enum PrimingBundle<'a> {
    /// The priming bundle's bytes, at hand.
    Bytes(&'a [u8]),
    /// No priming bundle could be obtained, and WHY.
    ///
    /// Not an optional input dressed up: a SELF-DESCRIBING artifact never reads a priming
    /// bundle at all, so requiring one there would refuse a bundle that is complete on its
    /// own terms. Anything else HARD-FAILS with this reason attached, so an absent priming
    /// bundle can never become a skipped dictionary resolution — which is the one outcome
    /// the medium axis exists to forbid.
    Absent(&'a str),
}

/// The whole medium inventory of an artifact, read from that artifact ALONE.
///
/// # Errors
/// The wire is unreadable or violates Rule 6, the artifact carries no medium registry,
/// or a realization/assignment row is malformed.
pub fn inventory(bytes: &[u8]) -> Result<MediumInventory, gmeow_errors::Diag> {
    let (headers, frames) = audit::check_wire(bytes)?;
    let dataset = audit::fold_leniently(bytes)?;
    let registry = MediumRegistry::from_dataset(dataset.as_ref())?;
    if registry.media().is_empty() || registry.dictionaries().is_empty() {
        return Err(invalid_declaration(
            "the artifact carries no gmeow:Medium / gmeow:CompressionDictionary declarations — \
             only a self-describing artifact (the distribution bundle) carries the registry \
             `gmeow medium list` reports, and there is nothing to list without it",
        ));
    }
    let realizations = super::rdf::realizations(dataset.as_ref())?;
    let envelopes = super::rdf::envelopes(dataset.as_ref())?;

    let mut pinned: Vec<(String, usize)> = Vec::new();
    let mut pinned_len: BTreeMap<&str, usize> = BTreeMap::new();
    for header in &headers {
        for (name, dict) in &header.pinned_bytes {
            if pinned_len.insert(name.as_str(), dict.len()).is_none() {
                pinned.push((name.clone(), dict.len()));
            }
        }
    }

    let mut primes: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (rep, row) in registry.assignment() {
        if let DictSelection::Named(iri) = &row.dictionary {
            primes.entry(iri.as_str()).or_default().push(rep.clone());
        }
    }

    let mut dictionaries = Vec::with_capacity(registry.dictionaries().len());
    for (iri, def) in registry.dictionaries() {
        let realization = realizations.get(iri).ok_or_else(|| {
            invalid_declaration(format!(
                "dictionary {:?} is declared but the artifact carries no \
                 gmeow:CompressionDictionaryRealization for it — a declared dictionary with no \
                 measured realization names bytes nobody can obtain",
                def.id
            ))
        })?;
        dictionaries.push(DictionaryRow {
            id: def.id.clone(),
            iri: iri.clone(),
            version: def.version.clone(),
            strategy: def.strategy,
            measured_strategy: realization.strategy,
            target_length: def.target_length,
            corpus: def.corpus.clone(),
            content_digest: realization.content_digest.clone(),
            byte_length: realization.byte_length,
            zstd_dictionary_id: realization.zstd_dictionary_id,
            corpus_sample_count: realization.corpus_sample_count,
            in_band_bytes: pinned_len.get(def.id.as_str()).copied(),
            primes: primes.get(iri.as_str()).cloned().unwrap_or_default(),
        });
    }
    dictionaries.sort_by(|a, b| a.id.cmp(&b.id));

    let media = registry
        .media()
        .values()
        .map(|medium| {
            Ok(MediumRow {
                iri: medium.iri.clone(),
                codec: medium.codec_wire_name()?.to_string(),
                zstd_level: medium.zstd_level,
                source_kind: medium.source_kind,
                dictionaries: medium
                    .dictionaries
                    .iter()
                    .filter_map(|iri| registry.dictionaries().get(iri).map(|def| def.id.clone()))
                    .collect(),
                reader_capabilities: medium.reader_capabilities.clone(),
            })
        })
        .collect::<Result<Vec<_>, gmeow_errors::Diag>>()?;

    let assignment = registry
        .assignment()
        .iter()
        .map(|(rep, row)| AssignmentRow {
            rep: rep.clone(),
            medium: row.medium.clone(),
            dictionary: dictionary_id_of(&registry, row),
        })
        .collect();

    Ok(MediumInventory {
        media,
        dictionaries,
        assignment,
        envelope_count: envelopes.len(),
        payload_frame_count: frames.len(),
        pinned,
    })
}

/// The `gmeow:dictionaryId` an assignment row selects, or `None` for the declared
/// no-dictionary selection.
fn dictionary_id_of(registry: &MediumRegistry, row: &RepAssignment) -> Option<String> {
    match &row.dictionary {
        DictSelection::Named(iri) => registry.dictionaries().get(iri).map(|def| def.id.clone()),
        DictSelection::Baseline => None,
    }
}

/// Explain one shipped dictionary: what it is, what corpus it was trained over, which
/// reps it primes, and what its measured two-part code bought.
///
/// # Errors
/// The artifact carries no registry, or no dictionary with that `gmeow:dictionaryId`.
pub fn explain(
    bytes: &[u8],
    dictionary_id: &str,
) -> Result<DictionaryExplanation, gmeow_errors::Diag> {
    let inventory = inventory(bytes)?;
    let row = inventory
        .dictionaries
        .iter()
        .find(|row| row.id == dictionary_id)
        .cloned()
        .ok_or_else(|| {
            unknown_dictionary(format!(
                "dictionary id {dictionary_id:?} resolves to no gmeow:CompressionDictionary the \
                 artifact declares (declared: {:?})",
                inventory
                    .dictionaries
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>()
            ))
        })?;

    let dataset = audit::fold_leniently(bytes)?;
    let registry = MediumRegistry::from_dataset(dataset.as_ref())?;
    let corpus = registry.corpora().get(&row.corpus).ok_or_else(|| {
        invalid_declaration(format!(
            "dictionary {dictionary_id:?} trains over <{}>, which the artifact does not declare \
             as a gmeow:DictionaryCorpus — a corpus that is not declared names no samples",
            row.corpus
        ))
    })?;
    let selectors = corpus.selectors.iter().map(ToString::to_string).collect();
    let effects = super::measure::effects(&registry, dataset.as_ref())?
        .into_iter()
        .filter(|effect| effect.dictionary_id == dictionary_id)
        .collect();

    Ok(DictionaryExplanation {
        row,
        selectors,
        effects,
    })
}

/// Derive which audit branch governs `bytes`, from `bytes`.
fn classify(dataset: &purrdf::RdfDataset, headers: &[audit::SegmentHeader]) -> ArtifactClass {
    let carried = MediumRegistry::from_dataset(dataset).ok();
    let self_describing = carried
        .as_ref()
        .is_some_and(|registry| !registry.media().is_empty() && !registry.schemas().is_empty());
    if self_describing {
        return ArtifactClass::SelfDescribing;
    }
    if headers.iter().any(|header| !header.pinned.is_empty()) {
        return ArtifactClass::HeaderDict;
    }
    ArtifactClass::WholeArtifact
}

/// The one declared medium of `kind`, resolved out of `registry`.
///
/// Exactly-one rather than first-match: the branch a classified artifact is audited
/// under has no answer when two media claim the kind, and no audit at all when none
/// does.
fn medium_of_kind(
    registry: &MediumRegistry,
    kind: MediumSourceKind,
) -> Result<String, gmeow_errors::Diag> {
    let candidates: Vec<&String> = registry
        .media()
        .values()
        .filter(|medium| medium.source_kind == kind)
        .map(|medium| &medium.iri)
        .collect();
    match candidates.as_slice() {
        [only] => Ok((*only).clone()),
        other => Err(invalid_declaration(format!(
            "the registry declares {} gmeow:Medium value(s) with gmeow:mediumSourceKind {kind:?} \
             ({other:?}) — an artifact classified into that branch has no derivable medium unless \
             exactly one medium claims it",
            other.len()
        ))),
    }
}

/// The reader capabilities the WIRE actually demands, as opposed to the ones its
/// declared medium says it demands.
fn actual_capabilities(
    headers: &[audit::SegmentHeader],
    frames: &[audit::PayloadFrame],
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let mut out = BTreeSet::new();
    for frame in frames {
        let (header, in_band) = audit::entry_for(headers, frame)?;
        if let Some(spec) = header.codec_spec.get(&frame.codec) {
            // GTS §8.4 classifies rsyncable framing as NON-baseline, so an artifact that
            // uses it demands a capability of its reader whether or not it also primes.
            if spec.name == "zstd-rsyncable" {
                out.insert("zstd-rsyncable".to_string());
            }
        }
        if in_band.is_some() {
            out.insert("zstd-dictionary".to_string());
        }
    }
    Ok(out)
}

/// Decode one payload frame through the codec chain its OWN segment header declares.
fn decode_frame(
    header: &audit::SegmentHeader,
    frame: &audit::PayloadFrame,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let spec = header.codec_spec.get(&frame.codec).ok_or_else(|| {
        invalid_declaration(format!(
            "the payload frame at byte offset {} references codec id {}, whose catalog entry \
             carries no descriptor",
            frame.offset, frame.codec
        ))
    })?;
    let dict = match header.dict_of_codec.get(&frame.codec) {
        Some(name) => Some(header.pinned_bytes.get(name).cloned().ok_or_else(|| {
            unknown_dictionary(format!(
                "the frame at byte offset {} is primed with {name:?}, which the segment header at \
                 byte offset {} does not pin in its \"dct\" map",
                frame.offset, header.offset
            ))
        })?),
        None => None,
    };
    let chain = [purrdf::gts::codec::Codec {
        name: spec.name.clone(),
        cls: spec.cls.clone(),
        dct: dict,
        level: spec.level,
    }];
    purrdf::gts::codec::decode_chain(&chain, &frame.payload).map_err(|err| {
        opaque_frame(format!(
            "the payload frame at byte offset {} (rep {:?}) does not decode through its declared \
             chain [{}]: {err} — purrdf's reader stores a blob frame LAZILY, so a fold that \
             reported no diagnostic never touched these bytes; a reader that reaches for the blob \
             gets an opaque node with no content, and there is no dictionary-less retry",
            frame.offset,
            audit::rep_of(frame),
            spec.name
        ))
    })
}

/// Verify one artifact end to end: the wire, the declared-media branch, every frame's
/// decode, every envelope's digests, and the absence of opaque nodes.
///
/// `priming` is the bundle an artifact that carries NONE of its own registry is audited
/// against. A self-describing artifact never touches it and is audited against the
/// registry it carries, because the whole point of a self-describing artifact is that the
/// claim travels with the bytes.
///
/// # Errors
/// Every named medium failure class: `Transform` (the universal Rule 6 rule),
/// `MediumUnknownDictionary`, `MediumUndeclaredDictionary`, `MediumUnknownSchema`,
/// `MediumDigestMismatch`, `MediumOpaqueFrame`, or `InvalidDeclaration`.
pub fn verify(
    bytes: &[u8],
    priming: PrimingBundle<'_>,
) -> Result<MediumVerification, gmeow_errors::Diag> {
    // 1. The wire clauses FIRST — see `audit::check_wire`: every fault they name also
    //    breaks the fold, so a fold-first order would report a dozen different defects
    //    with one useless message.
    let (headers, frames) = audit::check_wire(bytes)?;
    let dataset = audit::fold_leniently(bytes)?;
    let class = classify(dataset.as_ref(), &headers);

    let resolved;
    let (registry, medium) = match class {
        ArtifactClass::SelfDescribing => {
            resolved = MediumRegistry::from_dataset(dataset.as_ref())?;
            let medium = declared_medium_of(dataset.as_ref(), DIST_BUNDLE_PRODUCER)?;
            (&resolved, medium)
        }
        other => {
            let PrimingBundle::Bytes(bundle) = priming else {
                let PrimingBundle::Absent(why) = priming else {
                    unreachable!("PrimingBundle is Bytes or Absent")
                };
                return Err(undeclared_dictionary(format!(
                    "this artifact carries no gmeow:medium registry of its own, so it is audited \
                     against the bundle whose dictionaries primed it — and that bundle is not at \
                     hand: {why}. There is no registry-less branch: resolving the dictionary its \
                     header cites is the whole check, and skipping it would accept a store primed \
                     with an id no shipped bundle declares"
                )));
            };
            resolved = registry_of(bundle)?;
            let medium = medium_of_kind(&resolved, other.source_kind())?;
            (&resolved, medium)
        }
    };

    // 2. The DECLARED-MEDIA branch, which also runs the universal Rule 6 rule again and
    //    ends with the zero-opaque-node clause.
    validate_declared_media(
        bytes,
        &MediumDeclaration {
            medium: &medium,
            registry,
        },
    )?;

    // 3. Every envelope, and 4. every frame's decode, in ONE pass. A self-describing
    //    artifact is the only one that carries envelopes at all — a runtime store has no
    //    `graph/medium-registry` for them to live in — so an artifact without them
    //    verifies its frames and nothing more.
    //
    //    The two are interleaved rather than sequenced deliberately: an envelope is opened
    //    against its own frame's DECODED bytes, and holding every decoded payload until a
    //    second pass would mean materializing the whole bundle uncompressed at once. The
    //    decoded bytes are dropped as soon as the frame they belong to is settled.
    let envelopes = if class == ArtifactClass::SelfDescribing {
        envelope_index(dataset.as_ref(), &frames)?
    } else {
        BTreeMap::new()
    };
    let capabilities = native_reader_capabilities();

    let mut verified = Vec::with_capacity(frames.len());
    let mut dictionaries = BTreeSet::new();
    let mut opened = 0usize;
    for frame in &frames {
        let (header, in_band) = audit::entry_for(&headers, frame)?;
        let decoded = decode_frame(header, frame)?;
        if let Some(name) = in_band {
            dictionaries.insert(name.clone());
        }
        if let Some(declared) = &frame.digest {
            let actual = blake3_digest(&decoded);
            if &actual != declared {
                return Err(digest_mismatch(format!(
                    "the frame at byte offset {} (rep {:?}) states in-band pub.digest {declared}, \
                     but its {} decoded byte(s) digest to {actual} — the frame's own identity \
                     claim is false, so every envelope, projection and archive member derived \
                     from it addresses content the artifact does not carry",
                    frame.offset,
                    audit::rep_of(frame),
                    decoded.len()
                )));
            }
            let frame_id =
                crate::stages::medium_dictionaries::frame_iri(audit::rep_of(frame), declared);
            if let Some(envelope) = envelopes.get(frame_id.as_str()) {
                super::envelope::open(envelope, registry, &capabilities, &decoded, &decoded)?;
                opened += 1;
            } else if !envelopes.is_empty() {
                return Err(invalid_declaration(format!(
                    "no gmeow:MediumEnvelope describes the frame at byte offset {} (rep {:?}, \
                     identity <{frame_id}>) — the projection is one envelope per frame, so a \
                     frame nothing describes is a frame whose medium the artifact never states",
                    frame.offset,
                    audit::rep_of(frame)
                )));
            }
        }
        verified.push(VerifiedFrame {
            offset: frame.offset,
            rep: audit::rep_of(frame).to_string(),
            dictionary: in_band.cloned(),
            declared_digest: frame.digest.clone(),
            decoded_bytes: decoded.len(),
        });
    }

    // The self-referential snapshot envelope is the one that cannot be checked against a
    // decoded frame, and the reason is structural: its `gmeow:contentDigest` is the
    // builder's `snapshot_content_id()` over a representation a reader cannot re-derive
    // (folding the graph back re-interns its blank nodes). That is exactly why it carries
    // a STRATIFIED digest as well.
    let envelopes_verified = if envelopes.is_empty() {
        0
    } else {
        for envelope in envelopes.values() {
            if envelope.stratum == DigestStratum::PayloadExcludingMediumEnvelope {
                verify_snapshot_envelope(dataset.as_ref(), envelope)?;
                opened += 1;
            }
        }
        opened
    };
    if !envelopes.is_empty() && envelopes_verified != envelopes.len() {
        return Err(invalid_declaration(format!(
            "{} of the artifact's {} gmeow:MediumEnvelope record(s) were re-derived — an \
             envelope nothing re-derives is an unchecked claim riding in the shipped bytes",
            envelopes_verified,
            envelopes.len()
        )));
    }

    let declared_capabilities = registry
        .media()
        .get(&medium)
        .map(|declared| declared.reader_capabilities.clone())
        .unwrap_or_default();

    Ok(MediumVerification {
        class,
        medium,
        frames: verified,
        envelopes_verified,
        actual_capabilities: actual_capabilities(&headers, &frames)?,
        declared_capabilities,
        dictionaries,
    })
}

/// Every `gmeow:MediumEnvelope` the artifact carries, keyed by the frame identity it
/// addresses, after proving the count matches the wire and that no two collide.
///
/// One envelope per payload-bearing frame IS the projection's whole claim, so the count
/// is checked here, before any decode: a mismatch means either a frame whose medium
/// nothing states, or an envelope describing a frame the artifact does not carry, and
/// both are cheaper to report than to discover halfway through a decode.
fn envelope_index(
    dataset: &purrdf::RdfDataset,
    frames: &[audit::PayloadFrame],
) -> Result<BTreeMap<String, MediumEnvelope>, gmeow_errors::Diag> {
    let envelopes = super::rdf::envelopes(dataset)?;
    if envelopes.len() != frames.len() {
        return Err(invalid_declaration(format!(
            "the artifact carries {} gmeow:MediumEnvelope record(s) for {} payload-bearing \
             frame(s) — the projection is one envelope per frame, so any other count leaves a \
             frame whose medium nothing states (or an envelope describing a frame nothing carries)",
            envelopes.len(),
            frames.len()
        )));
    }
    let count = envelopes.len();
    let indexed: BTreeMap<String, MediumEnvelope> = envelopes
        .into_iter()
        .map(|envelope| (envelope.frame.clone(), envelope))
        .collect();
    if indexed.len() != count {
        return Err(invalid_declaration(
            "two gmeow:MediumEnvelope records describe one frame — an envelope is addressed BY \
             the frame it describes, so a collision means one of the two claims is unreachable",
        ));
    }
    Ok(indexed)
}

/// The self-referential snapshot envelope, checked through the one commitment a reader
/// CAN recompute.
fn verify_snapshot_envelope(
    dataset: &purrdf::RdfDataset,
    envelope: &MediumEnvelope,
) -> Result<(), gmeow_errors::Diag> {
    let stratum = crate::stages::carrier::snapshot_stratum_nquads(dataset)?;
    let expected = blake3_digest(stratum.as_bytes());
    if expected != envelope.strata_digest {
        return Err(digest_mismatch(format!(
            "the snapshot envelope carries gmeow:strataDigest {}, but the payload MINUS its \
             medium-envelope subgraph canonicalizes to {expected} — the one commitment a reader \
             can independently recompute over the snapshot frame does not hold",
            envelope.strata_digest
        )));
    }
    let derived =
        crate::stages::medium_dictionaries::frame_iri(SNAPSHOT_WIRE_REP, &envelope.content_digest);
    if derived != envelope.frame {
        return Err(digest_mismatch(format!(
            "the snapshot envelope describes frame <{}>, but the identity DERIVED from its own \
             gmeow:contentDigest is <{derived}> — the digest is then a free value rather than the \
             payload's own id, and nothing addresses the frame it claims to describe",
            envelope.frame
        )));
    }
    Ok(())
}

/// [`verify`] plus every clause a repository GATE can additionally afford.
///
/// The extra clauses all need something a consumer verb does not: the artifact's own
/// measurement graph, its realization records, and the declared reader contract to
/// compare the wire against. Where the artifact carries no registry of its own (a runtime
/// store), the registry-scoped clauses have no subject and only the capability comparison
/// is added — a store's dictionaries are the BUNDLE's, and the bundle is where their cost
/// is priced.
///
/// # Errors
/// Everything [`verify`] raises, plus `MediumDictionaryRegression` (a dictionary that
/// does not pay for itself, or one with no measured row), `MediumDigestMismatch` (a
/// pinned dictionary whose bytes disagree with its recorded realization), or
/// `MediumOpaqueFrame` (the declared reader contract does not match what the wire
/// demands).
pub fn gate(
    bytes: &[u8],
    priming: PrimingBundle<'_>,
) -> Result<MediumVerification, gmeow_errors::Diag> {
    let verification = verify(bytes, priming)?;

    // The reader contract is a property of the DELIVERABLE (Principle 13): a medium that
    // under-declares hides a demand a consumer discovers mid-decode, and one that
    // over-declares refuses readers the artifact would in fact serve.
    if verification.declared_capabilities != verification.actual_capabilities {
        return Err(opaque_frame(format!(
            "the artifact's wire demands reader capabilit(y/ies) {:?}, but its declared medium <{}> \
             declares {:?} — raising (or lowering) the reader contract is a declared property of \
             the deliverable, never something a consumer discovers mid-decode",
            verification.actual_capabilities,
            verification.medium,
            verification.declared_capabilities
        )));
    }

    if verification.class != ArtifactClass::SelfDescribing {
        return Ok(verification);
    }

    let dataset = audit::fold_leniently(bytes)?;
    let registry = MediumRegistry::from_dataset(dataset.as_ref())?;

    // Registry completeness: every declared dictionary is realized, every realization's
    // version is still declared, and every pinned entry's BYTES are the ones the
    // realization records.
    let realizations = super::rdf::realizations(dataset.as_ref())?;
    let ordered: Vec<super::rdf::DictionaryRealization> = realizations.values().cloned().collect();
    super::rdf::check_dictionary_retention(&registry, &ordered)?;
    let (headers, _) = audit::check_wire(bytes)?;
    let mut pinned: BTreeMap<&str, &Vec<u8>> = BTreeMap::new();
    for header in &headers {
        for (name, dict) in &header.pinned_bytes {
            pinned.insert(name.as_str(), dict);
        }
    }
    for (iri, def) in registry.dictionaries() {
        let realization = realizations.get(iri).ok_or_else(|| {
            invalid_declaration(format!(
                "dictionary {:?} is declared but the bundle carries no \
                 gmeow:CompressionDictionaryRealization for it",
                def.id
            ))
        })?;
        let bytes = pinned.get(def.id.as_str()).ok_or_else(|| {
            unknown_dictionary(format!(
                "dictionary {:?} is declared and realized but no segment header pins it in band — \
                 the shipped header is the ONLY channel a consumer can obtain it from, so a \
                 declared-but-unpinned dictionary is nameable and unobtainable",
                def.id
            ))
        })?;
        let actual = blake3_digest(bytes);
        if actual != realization.content_digest {
            return Err(digest_mismatch(format!(
                "the header pins {} byte(s) under {:?}, which digest to {actual}, but its \
                 realization records gmeow:dictionaryContentDigest {} — a decoder compares that \
                 literal against the dictionary it holds BEFORE priming, so the disagreement makes \
                 every primed decode in this artifact unsafe",
                bytes.len(),
                def.id,
                realization.content_digest
            )));
        }
        if bytes.len() != realization.byte_length {
            return Err(digest_mismatch(format!(
                "the header pins {} byte(s) under {:?}, but its realization records \
                 gmeow:dictionaryByteLength {}",
                bytes.len(),
                def.id,
                realization.byte_length
            )));
        }
    }

    // The MDL win gate, over the measured rows the bundle SHIPS.
    let effects = super::measure::effects(&registry, dataset.as_ref())?;
    super::measure::check(&effects, &super::measure::required_measurements(&registry))?;

    Ok(verification)
}

/// The artifact's medium inventory as the JSON envelope the MCP resource serves.
///
/// # Errors
/// Everything [`inventory`] raises.
pub fn inventory_json(bytes: &[u8]) -> Result<String, gmeow_errors::Diag> {
    let inventory = inventory(bytes)?;
    let media: Vec<serde_json::Value> = inventory
        .media
        .iter()
        .map(|medium| {
            serde_json::json!({
                "medium": medium.iri,
                "codec": medium.codec,
                "zstdLevel": medium.zstd_level,
                "sourceKind": format!("{:?}", medium.source_kind),
                "dictionaries": medium.dictionaries,
                "requiresReaderCapability": medium.reader_capabilities,
            })
        })
        .collect();
    let dictionaries: Vec<serde_json::Value> = inventory
        .dictionaries
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "dictionary": row.iri,
                "version": row.version,
                "strategy": row.strategy.to_string(),
                "measuredStrategy": row.measured_strategy.to_string(),
                "targetLength": row.target_length,
                "corpus": row.corpus,
                "contentDigest": row.content_digest,
                "byteLength": row.byte_length,
                "zstdDictionaryId": row.zstd_dictionary_id,
                "corpusSampleCount": row.corpus_sample_count,
                "inBandBytes": row.in_band_bytes,
                "primes": row.primes,
            })
        })
        .collect();
    let assignment: Vec<serde_json::Value> = inventory
        .assignment
        .iter()
        .map(|row| {
            serde_json::json!({
                "rep": row.rep,
                "medium": row.medium,
                "dictionary": row.dictionary,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "graph": MEDIUM_REGISTRY_GRAPH,
        "media": media,
        "dictionaries": dictionaries,
        "assignment": assignment,
        "envelopes": inventory.envelope_count,
        "payloadFrames": inventory.payload_frame_count,
        "pinned": inventory
            .pinned
            .iter()
            .map(|(id, len)| serde_json::json!({"id": id, "bytes": len}))
            .collect::<Vec<_>>(),
    })
    .to_string())
}
