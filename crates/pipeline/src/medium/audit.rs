// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DECLARED-MEDIA audit: does an artifact's wire agree with the medium its
//! producer declared?
//!
//! [`gmeow_gts_profile::validate_mandated_frames`] is the UNIVERSAL Rule 6 codec
//! rule — one `zstd-rsyncable` transform at level 12 on every payload-bearing frame
//! — and it applies to every GMEOW-authored GTS artifact alike: the dist bundle, the
//! feedback / music / math bundles, `gmeow convert --to gts` output, and the
//! append-only runtime stores. It deliberately says NOTHING about dictionaries,
//! because most of those artifacts carry no medium registry at all. Making it
//! registry-dependent would leave exactly two escapes — ship a red gate, or carve out
//! "a registry-less bundle skips the medium check" — and the second is the silent
//! degradation the medium axis exists to forbid.
//!
//! So the dictionary half is a SECOND, separately-callable check, and it lives here
//! rather than in the leaf profile crate because it reads
//! [`MEDIUM_REGISTRY_GRAPH`](crate::medium::MEDIUM_REGISTRY_GRAPH) through
//! [`MediumRegistry`], which lives in this crate. Hosting it in `gmeow-gts-profile`
//! would recreate the cargo cycle that crate exists to break.
//!
//! # Which check applies is DECLARED, never inferred
//!
//! A producer declares the `gmeow:Medium` it writes through; that medium declares
//! exactly one `gmeow:mediumSourceKind`; and that individual selects the branch. All
//! three branches carry a real obligation — a branch that checked nothing would be an
//! exemption list wearing ontology clothing:
//!
//! * [`MediumSourceKind::PerRep`] — the dist bundle. Each frame's rep resolves to
//!   exactly one declared `gmeow:Medium` in the artifact's own
//!   `graph/medium-registry`, and the catalog entry the frame actually references is
//!   the one that medium names for that rep. A frame quietly riding an UNPRIMED
//!   catalog entry while its rep declares a dictionary is a hard fail, and so is a
//!   rep with no declared medium.
//! * [`MediumSourceKind::HeaderDict`] — the runtime stores. They do NOT carry
//!   `graph/medium-registry` (they cite bundle dictionaries by id), so every payload
//!   frame's catalog entry must name a dictionary present in THAT FILE's header
//!   `"dct"` map, and that name must match a registered `gmeow:dictionaryId`.
//! * [`MediumSourceKind::WholeArtifact`] — feedback / music / math / `convert --to
//!   gts`. Every payload frame references the SAME catalog entry, and that entry
//!   matches the producer's declared medium: under the explicitly dictionary-less
//!   `gmeow:mediumProfileBaselineL12` the entry carries no `dct`, and a producer
//!   declaring a primed medium must actually prime every frame rather than pin a
//!   dictionary nothing uses.
//!
//! # The zero-opaque gate
//!
//! Every branch additionally folds the artifact and requires ZERO opaque nodes and
//! ZERO reader diagnostics. Without it an unresolvable `"dct"` degrades silently:
//! purrdf's reader drops the catalog entry, the frame becomes an opaque node, and the
//! fold continues MINUS that frame's content. The bytes are intact, the fold
//! "succeeds", and a whole archive has vanished — exactly the degradation this module
//! exists to refuse.

use std::collections::{BTreeMap, BTreeSet};

use ciborium::value::Value;
use purrdf::gts::wire::{SELF_DESCRIBE_TAG, iter_items, map_get, unwrap_header};

use super::registry::{DictSelection, MediumDef, MediumRegistry, MediumSourceKind};
use super::{
    SNAPSHOT_WIRE_REP, invalid_declaration, opaque_frame, undeclared_dictionary, unknown_dictionary,
};

/// The declaration an artifact is audited against.
///
/// A plain pair rather than an `Option`-laden bag: an audit that could be asked to
/// check "some medium, or none" would be the unstated default the axis removes.
#[derive(Debug, Clone, Copy)]
pub struct MediumDeclaration<'a> {
    /// The `gmeow:Medium` IRI the producer declares it authors through.
    pub medium: &'a str,
    /// The registry the declaration is resolved against. For a SELF-DESCRIBING
    /// artifact (the dist bundle) this is the artifact's own folded registry; for an
    /// append-only store — which carries no `graph/medium-registry` — it is the
    /// bundle whose dictionaries primed it.
    pub registry: &'a MediumRegistry,
}

/// One payload-bearing frame of an artifact, as the audit reads it off the wire.
///
/// `pub(super)` rather than private because [`super::inspect`] — the surface the
/// `gmeow medium` verbs and the `gmeow-dev medium-gate` read through — decodes exactly
/// these frames. A second wire reader beside this one would be a second answer to
/// "which catalog entry does this frame ride", which is the question the whole audit
/// turns on.
#[derive(Debug, Clone)]
pub(super) struct PayloadFrame {
    /// Byte offset of the frame's CBOR item, for messages.
    pub(super) offset: usize,
    /// Index of the segment whose catalog this frame's transform id resolves in.
    pub(super) segment: usize,
    /// The frame's `pub.rep` wire label, when it carries public metadata.
    pub(super) rep: Option<String>,
    /// The frame's `pub.digest` — the byte identity the frame itself states. Absent
    /// exactly on the snapshot frame, which carries no public metadata at all.
    pub(super) digest: Option<String>,
    /// The single transform id the frame references.
    pub(super) codec: i128,
    /// The frame's `"d"` bytes, verbatim — still under the transform chain.
    pub(super) payload: Vec<u8>,
}

/// One segment header, as the audit reads it off the wire.
#[derive(Debug, Clone, Default)]
pub(super) struct SegmentHeader {
    /// Byte offset of the header item, for messages.
    pub(super) offset: usize,
    /// Codec id → the `"dct"` dictionary name that catalog entry binds, when it
    /// binds one. An entry ABSENT from this map is an unprimed entry.
    pub(super) dict_of_codec: BTreeMap<i128, String>,
    /// Codec id → the catalog entry's declared `(name, cls, level)` triple, so a
    /// consumer can rebuild the transform chain the frame was written through without
    /// re-parsing the header a second time.
    pub(super) codec_spec: BTreeMap<i128, CodecSpec>,
    /// Every codec id the catalog declares — so a frame naming an id the catalog
    /// never declared is distinguishable from one naming an unprimed entry.
    pub(super) declared_codecs: BTreeSet<i128>,
    /// The dictionary names this segment pins in band under `"dct"`, with their
    /// verbatim bytes — the ONE channel a consumer primes a decode from.
    pub(super) pinned_bytes: BTreeMap<String, Vec<u8>>,
    /// The dictionary names this segment pins in band under `"dct"`.
    pub(super) pinned: BTreeSet<String>,
}

/// One codec-catalog entry's declared coordinates (§5, §8.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodecSpec {
    /// The registered codec name (`"zstd-rsyncable"` for every mandated entry).
    pub(super) name: String,
    /// `"encode"` | `"compress"` | `"encrypt"`.
    pub(super) cls: String,
    /// The declared `level?` parameter, made observable on the wire so a profile can
    /// gate on it.
    pub(super) level: Option<i32>,
}

/// Audit `bytes` against the medium its producer DECLARED.
///
/// Strictly stronger than [`gmeow_gts_profile::validate_mandated_frames`]: that check
/// runs first, so every artifact this one accepts also satisfies the universal Rule 6
/// codec rule.
///
/// # Errors
/// `Transform` (the universal frame profile fails, or the wire is unreadable),
/// `InvalidDeclaration` (the declared medium is not a declared `gmeow:Medium`),
/// `MediumUnknownSchema` (a frame's rep is unregistered), `MediumUndeclaredDictionary`
/// (a frame rides an unprimed entry while its rep declares a dictionary, or a rep has
/// no declared medium), `MediumUnknownDictionary` (a frame cites a dictionary the
/// header does not pin, or one no registered `gmeow:dictionaryId` resolves), or
/// `MediumOpaqueFrame` (the fold degraded a frame to an opaque node, or the reader
/// raised a diagnostic).
pub fn validate_declared_media(
    bytes: &[u8],
    declared: &MediumDeclaration<'_>,
) -> Result<(), gmeow_errors::Diag> {
    let (headers, frames) = check_wire(bytes)?;

    let medium = declared
        .registry
        .media()
        .get(declared.medium)
        .ok_or_else(|| {
            invalid_declaration(format!(
                "the producer declares medium <{}>, which is not a declared gmeow:Medium — a \
             producer whose medium cannot be resolved has no audit to run, and running none is \
             the exemption the medium axis removes",
                declared.medium
            ))
        })?;

    match medium.source_kind {
        MediumSourceKind::PerRep => check_per_rep(bytes, declared, &headers, &frames)?,
        MediumSourceKind::HeaderDict => check_header_dict(declared, &headers, &frames)?,
        MediumSourceKind::WholeArtifact => {
            check_whole_artifact(declared, medium, &headers, &frames)?
        }
    }

    check_no_opaque_nodes(bytes)
}

/// Every clause that is decidable from the WIRE ALONE, in one place and run FIRST.
///
/// Order is load-bearing rather than incidental. Every one of these faults ALSO breaks
/// the fold — a header whose `"dct"` map went missing no longer matches its own
/// self-hash, so the reader drops the segment — and a fold-first audit would report
/// every such artifact as "could not be re-imported", which is true, useless, and
/// identical for a dozen different defects. Checking the wire first means the artifact
/// is reported for the claim that is actually wrong with it.
///
/// # Errors
/// `Transform` (the universal Rule 6 profile), `InvalidDeclaration` (a torn sequence, a
/// frame with no segment, a payload-free artifact) or `MediumUnknownDictionary` (a
/// catalog entry binding a dictionary its header does not pin).
pub(super) fn check_wire(
    bytes: &[u8],
) -> Result<(Vec<SegmentHeader>, Vec<PayloadFrame>), gmeow_errors::Diag> {
    // The universal rule first: a bundle that violates Rule 6 is not made acceptable
    // by having a tidy dictionary story, and running it here is what makes this audit
    // strictly stronger rather than merely different.
    crate::gts_profile::validate_mandated_frames(bytes)?;
    let (headers, frames) = read_wire(bytes)?;
    if frames.is_empty() {
        return Err(invalid_declaration(
            "the artifact carries no payload-bearing frames — there is nothing for a declared \
             medium to govern",
        ));
    }
    check_catalog_dicts_are_pinned(&headers, &frames)?;
    Ok((headers, frames))
}

/// Every payload frame's catalog entry, if it binds a dictionary at all, names one the
/// frame's OWN segment header pins in band.
///
/// Universal: it binds under every `gmeow:mediumSourceKind`, because it is not a claim
/// about resolution policy but about the file being self-contained.
///
/// Universal: it holds under every `gmeow:mediumSourceKind`, because it is not a claim
/// about resolution policy but about the file being self-contained. A frame citing a
/// dictionary its segment does not carry is permanently undecodable even with every
/// byte intact.
fn check_catalog_dicts_are_pinned(
    headers: &[SegmentHeader],
    frames: &[PayloadFrame],
) -> Result<(), gmeow_errors::Diag> {
    for frame in frames {
        let (header, in_band) = entry_for(headers, frame)?;
        if let Some(name) = in_band
            && !header.pinned.contains(name)
        {
            return Err(unknown_dictionary(format!(
                "the frame at byte offset {} is primed with {name:?}, which the segment header at \
                 byte offset {} does not pin in its \"dct\" map — the payload is permanently \
                 undecodable even with its bytes intact, and the reader would DROP it rather than \
                 refuse",
                frame.offset, header.offset
            )));
        }
    }
    Ok(())
}

/// The `gmeow:GtsProducer` individual that classifies the terminal carrier — the one
/// producer whose artifact is `generated/dist/gmeow.gts`.
///
/// This IRI is the ONLY thing about the dist bundle's medium that is spelled in Rust;
/// the medium itself is read out of the bundle's own ontology through
/// `gmeow:producerMedium`, so a change of medium is a change of DATA. `crates/validate`'s
/// Seal C keeps the row honest in the other direction: it hard-fails if the terminal's
/// source file is not the one this producer claims.
pub const DIST_BUNDLE_PRODUCER: &str = "https://blackcatinformatics.ca/gmeow/gtsProducerDistBundle";

/// Audit a distribution bundle against the medium ITS OWN ontology says its producer
/// writes through.
///
/// The convenience door the gates use: it resolves
/// [`DIST_BUNDLE_PRODUCER`]'s `gmeow:producerMedium` out of the folded bundle, builds
/// the registry from the same bytes, and runs [`validate_declared_media`]. Nothing in
/// the chain is a Rust-side assumption about which dictionaries the bundle should
/// carry.
///
/// # Errors
/// Everything [`validate_declared_media`] raises, plus `InvalidDeclaration` when the
/// bundle carries no (or more than one) `gmeow:producerMedium` for the terminal.
pub fn validate_dist_bundle_media(bytes: &[u8]) -> Result<(), gmeow_errors::Diag> {
    // The wire clauses run BEFORE the fold that resolves the declaration: a bundle whose
    // header was tampered with folds to nothing, and "this producer declares no medium"
    // would then be the diagnosis for a file whose real defect is a dictionary it cites
    // but does not carry. `validate_declared_media` re-runs them; they are pure and
    // cheap, and a gate that depends on its caller having done half the work is a gate
    // waiting to be called wrong.
    check_wire(bytes)?;
    let dataset = fold_leniently(bytes)?;
    let medium = declared_medium_of(&dataset, DIST_BUNDLE_PRODUCER)?;
    let registry = MediumRegistry::from_dataset(&dataset)?;
    validate_declared_media(
        bytes,
        &MediumDeclaration {
            medium: &medium,
            registry: &registry,
        },
    )
}

/// The `gmeow:Medium` a `gmeow:GtsProducer` declares it writes through, read out of a
/// dataset that carries the producer→medium map (a slice's `module.ttl`, or a bundle
/// that folded it).
///
/// This is the one lookup that turns "which audit branch governs this producer" into a
/// derivation. Exactly-one is enforced here rather than defaulted: a producer with no
/// row has no branch, and one with two has no answer.
///
/// # Errors
/// `InvalidDeclaration` when the producer declares other than exactly one
/// `gmeow:producerMedium`.
pub fn declared_medium_of(
    dataset: &purrdf::RdfDataset,
    producer: &str,
) -> Result<String, gmeow_errors::Diag> {
    let media: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(dataset)
        .into_iter()
        .filter(|quad| {
            quad.subject == purrdf::RdfTerm::iri(producer)
                && quad.predicate == format!("{}producerMedium", super::GMEOW)
        })
        .filter_map(|quad| match quad.object {
            purrdf::RdfTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect();
    if media.len() != 1 {
        return Err(invalid_declaration(format!(
            "<{producer}> declares {} gmeow:producerMedium value(s) {media:?} — a producer writes \
             through exactly one medium, so any other count leaves its audit branch underivable",
            media.len()
        )));
    }
    Ok(media.into_iter().next().expect("length checked"))
}

/// Fold an artifact through purrdf's TOTAL reader rather than the strict importer.
///
/// Deliberate, and safe only because [`check_no_opaque_nodes`] runs at the end of every
/// audit. The strict importer refuses on the first fold diagnostic, which would make a
/// wire-level defect surface as a generic import error — the artifact would be reported
/// for the fold failure its tampering also caused rather than for the dictionary claim
/// that is actually wrong. Folding leniently lets the PRECISE wire clauses speak first;
/// nothing is thereby tolerated, because any degradation the lenient read papered over
/// is a hard failure at the zero-opaque gate.
pub(super) fn fold_leniently(
    bytes: &[u8],
) -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let graph = purrdf::gts::reader::read(bytes, true, None);
    purrdf::gts::dataset_from_gts_graph(&graph)
        .map_err(|e| invalid_declaration(format!("fold the artifact back to its own graphs: {e}")))
}

/// The wire read: every segment header's catalog, and every payload-bearing frame
/// keyed to the segment whose catalog resolves its transform id.
///
/// Codec ids are SEGMENT-scoped (spec §5), so a frame is only ever resolved against
/// the header that precedes it — resolving against a file-level union would let a
/// later segment's entry silently vouch for an earlier segment's frame.
fn read_wire(bytes: &[u8]) -> Result<(Vec<SegmentHeader>, Vec<PayloadFrame>), gmeow_errors::Diag> {
    let (items, torn) = iter_items(bytes);
    if let Some(offset) = torn {
        return Err(invalid_declaration(format!(
            "the artifact's CBOR sequence is torn at byte offset {offset}"
        )));
    }
    let mut headers: Vec<SegmentHeader> = Vec::new();
    let mut frames: Vec<PayloadFrame> = Vec::new();
    for (offset, item) in &items {
        if is_segment_header(item) {
            let header = unwrap_header(item).map_err(|message| {
                invalid_declaration(format!(
                    "invalid segment header at byte offset {offset}: {message}"
                ))
            })?;
            headers.push(read_header(*offset, header)?);
            continue;
        }
        let Value::Map(entries) = item else {
            return Err(invalid_declaration(format!(
                "the item at byte offset {offset} is neither a segment header nor a frame map"
            )));
        };
        if map_get(entries, "d").is_none() {
            continue;
        }
        let segment = headers.len().checked_sub(1).ok_or_else(|| {
            invalid_declaration(format!(
                "the payload frame at byte offset {offset} precedes any segment header"
            ))
        })?;
        let codec = match map_get(entries, "x") {
            Some(Value::Array(chain)) if chain.len() == 1 => match &chain[0] {
                Value::Integer(id) => i128::from(*id),
                other => {
                    return Err(invalid_declaration(format!(
                        "the transform id at byte offset {offset} is not a CBOR integer ({other:?})"
                    )));
                }
            },
            // Unreachable after `validate_mandated_frames`, which already refuses a
            // payload frame without exactly one transform. Refuse rather than assume.
            other => {
                return Err(invalid_declaration(format!(
                    "the payload frame at byte offset {offset} carries {other:?} for its \
                     transform chain, not exactly one id"
                )));
            }
        };
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
        let payload = match map_get(entries, "d") {
            Some(Value::Bytes(bytes)) => bytes.clone(),
            other => {
                return Err(invalid_declaration(format!(
                    "the payload frame at byte offset {offset} carries {other:?} for its \"d\" \
                     field, not a CBOR byte string"
                )));
            }
        };
        frames.push(PayloadFrame {
            offset: *offset,
            segment,
            rep,
            digest,
            codec,
            payload,
        });
    }
    Ok((headers, frames))
}

/// True when `item` is a GTS segment header rather than a frame — the same
/// discriminator the profile crate uses, restated here because `unwrap_header`
/// unwraps ANY map and would happily read a frame as a catalog-less header.
fn is_segment_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == SELF_DESCRIBE_TAG,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

fn read_header(
    offset: usize,
    header: &[(Value, Value)],
) -> Result<SegmentHeader, gmeow_errors::Diag> {
    let Some(Value::Map(catalog)) = map_get(header, "cat") else {
        return Err(invalid_declaration(format!(
            "the segment header at byte offset {offset} carries no codec catalog"
        )));
    };
    let mut dict_of_codec = BTreeMap::new();
    let mut declared_codecs = BTreeSet::new();
    let mut codec_spec = BTreeMap::new();
    for (id, descriptor) in catalog {
        let Value::Integer(id) = id else {
            return Err(invalid_declaration(format!(
                "the segment header at byte offset {offset} has a non-integer codec id"
            )));
        };
        let id = i128::from(*id);
        declared_codecs.insert(id);
        if let Value::Map(fields) = descriptor {
            if let Some(Value::Text(dict)) = map_get(fields, "dct") {
                dict_of_codec.insert(id, dict.clone());
            }
            codec_spec.insert(
                id,
                CodecSpec {
                    name: match map_get(fields, "name") {
                        Some(Value::Text(name)) => name.clone(),
                        _ => String::new(),
                    },
                    cls: match map_get(fields, "cls") {
                        Some(Value::Text(cls)) => cls.clone(),
                        _ => "encode".to_string(),
                    },
                    level: match map_get(fields, "level") {
                        Some(Value::Integer(level)) => i32::try_from(i128::from(*level)).ok(),
                        _ => None,
                    },
                },
            );
        }
    }
    let pinned_bytes: BTreeMap<String, Vec<u8>> = match map_get(header, "dct") {
        Some(Value::Map(dicts)) => dicts
            .iter()
            .filter_map(|(name, bytes)| match (name, bytes) {
                (Value::Text(name), Value::Bytes(bytes)) => Some((name.clone(), bytes.clone())),
                _ => None,
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    let pinned = pinned_bytes.keys().cloned().collect();
    Ok(SegmentHeader {
        offset,
        dict_of_codec,
        codec_spec,
        declared_codecs,
        pinned_bytes,
        pinned,
    })
}

/// The catalog entry a frame references, resolved in its OWN segment's catalog.
pub(super) fn entry_for<'a>(
    headers: &'a [SegmentHeader],
    frame: &PayloadFrame,
) -> Result<(&'a SegmentHeader, Option<&'a String>), gmeow_errors::Diag> {
    let header = headers.get(frame.segment).ok_or_else(|| {
        invalid_declaration(format!(
            "the payload frame at byte offset {} names segment {}, which has no header",
            frame.offset, frame.segment
        ))
    })?;
    if !header.declared_codecs.contains(&frame.codec) {
        return Err(invalid_declaration(format!(
            "the payload frame at byte offset {} references codec id {}, which its segment's \
             catalog (at byte offset {}) never declares",
            frame.offset, frame.codec, header.offset
        )));
    }
    Ok((header, header.dict_of_codec.get(&frame.codec)))
}

/// The `gmeow:payloadSchemaId` a frame's payload is registered under. A frame with no
/// `pub` metadata is the snapshot frame — the one payload the pack writes without a
/// content-representation tag — and it is registered like any other rep, so it is
/// resolved rather than exempted.
pub(super) fn rep_of(frame: &PayloadFrame) -> &str {
    frame.rep.as_deref().unwrap_or(SNAPSHOT_WIRE_REP)
}

/// `gmeow:mediumSourcePerRep`: every frame's rep resolves to exactly one declared
/// medium, and the catalog entry it actually references is the one that medium names
/// for that rep.
fn check_per_rep(
    bytes: &[u8],
    declared: &MediumDeclaration<'_>,
    headers: &[SegmentHeader],
    frames: &[PayloadFrame],
) -> Result<(), gmeow_errors::Diag> {
    // A per-rep artifact is SELF-DESCRIBING by construction: the rep→medium map it is
    // audited against travels inside it. Reading the registry back out of the artifact
    // is what makes the audit a property of the shipped bytes rather than of whatever
    // the caller happened to hold.
    let folded = fold_leniently(bytes)?;
    let carried = MediumRegistry::from_dataset(&folded)?;
    if carried.media().is_empty() || carried.schemas().is_empty() {
        return Err(undeclared_dictionary(
            "a gmeow:mediumSourcePerRep artifact carries no medium registry — its frames would \
             name reps nothing resolves, so every dictionary claim in it would be uncheckable",
        ));
    }
    let _ = declared;

    for frame in frames {
        let rep = rep_of(frame);
        // TOTAL by contract: an unregistered rep and a registered-but-unassigned rep
        // are DIFFERENT defects, and `assignment_for` names each.
        let row = carried.assignment_for(rep)?;
        if !carried.media().contains_key(&row.medium) {
            return Err(invalid_declaration(format!(
                "rep {rep:?} (frame at byte offset {}) is assigned medium <{}>, which the \
                 artifact's graph/medium-registry does not declare",
                frame.offset, row.medium
            )));
        }
        let (header, in_band) = entry_for(headers, frame)?;
        match &row.dictionary {
            DictSelection::Named(iri) => {
                let def = carried.dictionaries().get(iri).ok_or_else(|| {
                    unknown_dictionary(format!(
                        "rep {rep:?} selects dictionary <{iri}>, which the artifact's registry \
                         does not declare"
                    ))
                })?;
                let in_band = in_band.ok_or_else(|| {
                    undeclared_dictionary(format!(
                        "the frame at byte offset {} (rep {rep:?}) references an UNPRIMED catalog \
                         entry (codec id {}), but its rep declares dictionary {:?} — a frame that \
                         quietly rides the unprimed entry discards the density the declaration \
                         promises while still shipping under a reader contract that demands \
                         dictionary priming",
                        frame.offset, frame.codec, def.id
                    ))
                })?;
                if in_band != &def.id {
                    return Err(unknown_dictionary(format!(
                        "the frame at byte offset {} (rep {rep:?}) is primed with {in_band:?}, but \
                         its rep declares {:?} — decoding a primed payload against a DIFFERENT \
                         dictionary produces plausible-looking garbage rather than an error",
                        frame.offset, def.id
                    )));
                }
                if !header.pinned.contains(in_band) {
                    return Err(unknown_dictionary(format!(
                        "the frame at byte offset {} is primed with {in_band:?}, which the \
                         segment header at byte offset {} does not pin in its \"dct\" map — the \
                         payload would be permanently undecodable even with its bytes intact",
                        frame.offset, header.offset
                    )));
                }
            }
            DictSelection::Baseline => {
                if let Some(in_band) = in_band {
                    return Err(unknown_dictionary(format!(
                        "the frame at byte offset {} (rep {rep:?}) is primed with {in_band:?}, but \
                         its assigned medium <{}> declares NO dictionary — the medium's declared \
                         set is the bound on what it may prime with",
                        frame.offset, row.medium
                    )));
                }
            }
        }
    }
    Ok(())
}

/// `gmeow:mediumSourceHeaderDict`: the artifact carries no registry of its own, so
/// every payload frame's catalog entry must name a dictionary THAT FILE's header pins,
/// and that name must be a registered `gmeow:dictionaryId`.
fn check_header_dict(
    declared: &MediumDeclaration<'_>,
    headers: &[SegmentHeader],
    frames: &[PayloadFrame],
) -> Result<(), gmeow_errors::Diag> {
    for frame in frames {
        let (header, in_band) = entry_for(headers, frame)?;
        let in_band = in_band.ok_or_else(|| {
            undeclared_dictionary(format!(
                "the frame at byte offset {} references an UNPRIMED catalog entry (codec id {}), \
                 but its file is written through a gmeow:mediumSourceHeaderDict medium — every \
                 frame of such a segment primes with the dictionary its header pins, so an \
                 unprimed frame is a silent density loss rather than a selection",
                frame.offset, frame.codec
            ))
        })?;
        if !header.pinned.contains(in_band) {
            return Err(unknown_dictionary(format!(
                "the frame at byte offset {} is primed with {in_band:?}, which the segment header \
                 at byte offset {} does not pin in its \"dct\" map — a store that cites a \
                 dictionary it does not carry is undecodable away from the bundle that has it",
                frame.offset, header.offset
            )));
        }
        // The id must ALSO be one the ontology declares: a store may only be primed
        // with a dictionary the bundle actually ships, or nothing can ever re-prime it.
        declared.registry.dictionary_by_id(in_band)?;
    }
    Ok(())
}

/// `gmeow:mediumSourceWholeArtifact`: one medium governs the finished artifact as a
/// unit, so every payload frame references the SAME catalog entry and that entry
/// matches the declared medium.
fn check_whole_artifact(
    declared: &MediumDeclaration<'_>,
    medium: &MediumDef,
    headers: &[SegmentHeader],
    frames: &[PayloadFrame],
) -> Result<(), gmeow_errors::Diag> {
    let mut entries: BTreeSet<(i128, Option<String>)> = BTreeSet::new();
    for frame in frames {
        let (_, in_band) = entry_for(headers, frame)?;
        entries.insert((frame.codec, in_band.cloned()));
    }
    if entries.len() != 1 {
        return Err(invalid_declaration(format!(
            "a gmeow:mediumSourceWholeArtifact artifact must write EVERY payload frame through \
             one catalog entry; found {} distinct (codec id, dictionary) pairs {entries:?} — a \
             whole-artifact medium that varies per frame is a per-frame decision under a \
             whole-artifact name",
            entries.len()
        )));
    }
    let (_, in_band) = entries.into_iter().next().expect("length checked");

    // The declared medium's dictionary SET is the obligation, in both directions.
    match (medium.dictionaries.is_empty(), in_band) {
        // An explicitly dictionary-less medium: no `dct`, and that IS the selection.
        (true, None) => Ok(()),
        (true, Some(in_band)) => Err(unknown_dictionary(format!(
            "every payload frame is primed with {in_band:?}, but the declared medium <{}> \
             declares NO dictionary — the medium's declared set is the bound on what it may \
             prime with",
            declared.medium
        ))),
        (false, None) => Err(undeclared_dictionary(format!(
            "the declared medium <{}> declares {} dictionary/ies, but every payload frame rides \
             an UNPRIMED catalog entry — a producer that names a primed medium must actually \
             prime its frames, or the declaration promises density the artifact never delivers",
            declared.medium,
            medium.dictionaries.len()
        ))),
        (false, Some(in_band)) => {
            let def = declared.registry.dictionary_by_id(&in_band)?;
            if !medium.dictionaries.contains(&def.iri) {
                return Err(unknown_dictionary(format!(
                    "every payload frame is primed with {in_band:?} (<{}>), which the declared \
                     medium <{}> does not list in its gmeow:mediumDictionary bound",
                    def.iri, declared.medium
                )));
            }
            Ok(())
        }
    }
}

/// Folding the artifact must produce ZERO opaque nodes and ZERO reader diagnostics.
///
/// This is the clause that makes an unresolvable dictionary LOUD. purrdf's reader is
/// total by design (spec §7.6): a frame whose codec entry cannot be resolved degrades
/// to an opaque node and the fold continues without its content. Every byte is intact,
/// the fold reports success, and an entire archive is simply absent from the result —
/// so "the bundle parsed" is not evidence that the bundle is whole.
pub(super) fn check_no_opaque_nodes(bytes: &[u8]) -> Result<(), gmeow_errors::Diag> {
    let graph = purrdf::gts::reader::read(bytes, true, None);
    if !graph.opaque.is_empty() {
        let reasons: Vec<String> = graph
            .opaque
            .iter()
            .map(|node| format!("{}/{}", node.frame_type, node.reason))
            .collect();
        return Err(opaque_frame(format!(
            "folding the artifact produced {} opaque node(s) [{}] — a frame whose medium the \
             reader cannot resolve is DROPPED from the fold, so the result is silently missing \
             that frame's content",
            graph.opaque.len(),
            reasons.join(", ")
        )));
    }
    if !graph.diagnostics.is_empty() {
        let codes: Vec<String> = graph
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code, d.detail))
            .collect();
        return Err(opaque_frame(format!(
            "folding the artifact raised {} reader diagnostic(s) [{}] — the reader is total by \
             design, so a diagnostic is the only signal that it recovered rather than refused",
            graph.diagnostics.len(),
            codes.join("; ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medium::registry::fixture;

    /// A `gmeow-` prefixed dictionary the store fixtures prime with, trained over a
    /// corpus big enough for zstd to accept.
    fn dict_bytes() -> Vec<u8> {
        let owned: Vec<Vec<u8>> = (0..512u32)
            .map(|i| {
                format!(
                    "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                    i % 41
                )
                .into_bytes()
            })
            .collect();
        let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        crate::medium::train::build(
            crate::medium::registry::DictionaryStrategy::Trained,
            &corpus,
            4096,
        )
        .expect("the fixture dictionary trains")
    }

    fn registry(extra: &str) -> MediumRegistry {
        MediumRegistry::from_dataset(&fixture::dataset(extra)).expect("fixture registry")
    }

    /// A whole-artifact bundle authored through the production baseline door.
    fn baseline_bundle() -> Vec<u8> {
        let dataset = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> <https://e/o> .\n",
            "application/n-triples",
            None,
        )
        .expect("fixture parses");
        let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
        builder.add_dataset(&dataset).expect("add fixture");
        crate::gts_profile::emit_gmeow_gts(
            &builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"a whole-artifact payload".repeat(32),
                media_type: "text/plain".to_string(),
                rep: "cells-archive".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("the baseline door emits")
    }

    #[test]
    fn a_whole_artifact_bundle_passes_under_its_declared_dictionary_less_medium() {
        let registry = registry("");
        let bytes = baseline_bundle();
        // The universal Rule 6 check still holds on the very same bytes: the split is
        // an ADDITION, never a replacement.
        crate::gts_profile::validate_mandated_frames(&bytes).expect("universal rule holds");
        validate_declared_media(
            &bytes,
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumBaseline"),
                registry: &registry,
            },
        )
        .expect("an unprimed artifact under a dictionary-less medium is exactly conformant");
    }

    #[test]
    fn a_whole_artifact_producer_declaring_a_primed_medium_must_prime_every_frame() {
        // The dist fixture medium DECLARES two dictionaries. An artifact authored
        // through the unprimed door and then claimed to be that medium is refused —
        // this is the branch's real obligation, not a formality.
        let registry = registry("");
        let diag = validate_declared_media(
            &baseline_bundle(),
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumDist"),
                registry: &registry,
            },
        )
        .expect_err("a primed medium with unprimed frames must be refused");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
    }

    #[test]
    fn a_producer_whose_declared_medium_is_undeclared_is_refused() {
        let registry = registry("");
        let diag = validate_declared_media(
            &baseline_bundle(),
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumInvented"),
                registry: &registry,
            },
        )
        .expect_err("an unresolvable declared medium has no audit to run");
        assert_eq!(diag.code(), crate::error::InvalidDeclaration::register());
    }

    /// A store-shaped file: one dict-primed segment authored through the production
    /// `store_writer` door, audited under a header-dict medium.
    fn primed_store(dictionary: &str) -> Vec<u8> {
        let medium = crate::gts_profile::StoreMedium {
            dictionary: dictionary.to_string(),
            bytes: dict_bytes(),
        };
        let mut writer = crate::gts_profile::store_writer("ai-package", &[], &medium)
            .expect("the store door opens");
        writer
            .add_terms(&[purrdf::gts::model::Term {
                kind: purrdf::gts::model::TermKind::Iri,
                value: Some("https://e/claim".to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            }])
            .expect("terms frame");
        writer.into_bytes()
    }

    #[test]
    fn a_header_dict_store_passes_when_its_pinned_dictionary_is_registered() {
        let registry = registry(HEADER_DICT_MEDIUM);
        let bytes = primed_store("gmeow-core-v1");
        crate::gts_profile::validate_mandated_frames(&bytes).expect("universal rule holds");
        validate_declared_media(
            &bytes,
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumStore"),
                registry: &registry,
            },
        )
        .expect("a primed store citing a registered dictionary conforms");
    }

    #[test]
    fn a_header_dict_store_citing_an_unregistered_dictionary_is_refused() {
        let registry = registry(HEADER_DICT_MEDIUM);
        let diag = validate_declared_media(
            &primed_store("not-a-registered-dictionary"),
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumStore"),
                registry: &registry,
            },
        )
        .expect_err("a store citing an unregistered dictionary id must be refused");
        assert_eq!(
            diag.code(),
            crate::error::MediumUnknownDictionary::register(),
            "{diag}"
        );
    }

    #[test]
    fn a_header_dict_store_with_unprimed_frames_is_refused() {
        // The counter-example the branch exists for: a store authored through the
        // deliberately unprimed writer, then claimed to be header-dict.
        let registry = registry(HEADER_DICT_MEDIUM);
        let mut writer = gmeow_gts_profile::GmeowGtsWriter::new("ai-package");
        writer
            .add_terms(&[purrdf::gts::model::Term {
                kind: purrdf::gts::model::TermKind::Iri,
                value: Some("https://e/claim".to_string()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            }])
            .expect("terms frame");
        let diag = validate_declared_media(
            &writer.into_bytes(),
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumStore"),
                registry: &registry,
            },
        )
        .expect_err("an unprimed store under a header-dict medium must be refused");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
    }

    /// A header-dict medium spliced into the registry fixture.
    const HEADER_DICT_MEDIUM: &str = "\
gmeow:mediumStore a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourceHeaderDict ;
    gmeow:requiresReaderCapability \"zstd-dictionary\" , \"zstd-rsyncable\" ;
    gmeow:mediumDictionary gmeow:dictCore , gmeow:dictTerms .
";

    /// A bundle whose catalog names a `dct` the header map does not carry: the reader
    /// drops the entry, the frame degrades to an opaque node, and the fold silently
    /// loses its content. Both halves of the refusal are proved — the wire check fires,
    /// and the fold really would have degraded.
    #[test]
    fn a_bundle_whose_catalog_names_an_unpinned_dictionary_is_rejected() {
        let mut bytes = primed_store("gmeow-core-v1");
        strip_header_dct(&mut bytes);
        // The degradation is REAL, not hypothetical: fold the tampered bytes and watch
        // the reader recover instead of refusing.
        let graph = purrdf::gts::reader::read(&bytes, true, None);
        assert!(
            !graph.opaque.is_empty() || !graph.diagnostics.is_empty(),
            "the tampered store must genuinely degrade, or the gate below is vacuous"
        );
        let registry = registry(HEADER_DICT_MEDIUM);
        let diag = validate_declared_media(
            &bytes,
            &MediumDeclaration {
                medium: &crate::medium::registry::gm("mediumStore"),
                registry: &registry,
            },
        )
        .expect_err("a catalog naming an unpinned dictionary must be refused");
        assert_eq!(
            diag.code(),
            crate::error::MediumUnknownDictionary::register(),
            "{diag}"
        );
    }

    /// STRICTLY STRONGER, proved rather than asserted: every artifact the UNIVERSAL
    /// Rule 6 check rejects is also rejected by the declared-media audit.
    ///
    /// The split would be a regression if any of these slipped through — a second
    /// check that accepted what the first refused would mean the gate that runs
    /// second had quietly widened the contract. The set replays the profile crate's
    /// own rejection fixtures (a bare writer, a level-declaring bare writer, a
    /// payload frame with its transform chain removed, a mixed multi-segment file)
    /// and adds the two the medium axis makes newly reachable: a frame naming a codec
    /// id its catalog never declared, and a catalog whose `zstd-rsyncable` entry omits
    /// the mandated level.
    #[test]
    fn every_universal_rejection_is_still_a_rejection_under_the_stronger_check() {
        let registry = registry(HEADER_DICT_MEDIUM);
        let declared = MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumBaseline"),
            registry: &registry,
        };

        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
        cases.push(("empty bytes", Vec::new()));
        cases.push(("not a header", b"\xa1\x61a\x01".to_vec()));
        cases.push(("a torn CBOR sequence", {
            let mut b = baseline_bundle();
            b.truncate(b.len() - 7);
            b
        }));
        // A bare purrdf writer: no declared level AND no transform chain.
        cases.push(("a bare purrdf writer", {
            let mut writer = purrdf::gts::writer::Writer::new("ai-package");
            writer.add_terms(&[iri_term("https://e/s")]);
            writer.into_bytes()
        }));
        // A level-declaring bare writer: the frame-level violation alone.
        cases.push(("a level-declaring bare writer", {
            let options = purrdf::gts::writer::WriterOptions {
                zstd_level: Some(12),
                ..Default::default()
            };
            let mut writer =
                purrdf::gts::writer::Writer::with_options("ai-package", options).expect("valid");
            writer.add_terms(&[iri_term("https://e/s")]);
            writer.into_bytes()
        }));
        // A conforming first segment followed by an unprofiled appended one.
        cases.push(("a mixed multi-segment file", {
            let mut mixed = primed_store("gmeow-core-v1");
            let mut bare = purrdf::gts::writer::Writer::new("ai-package");
            bare.add_terms(&[iri_term("https://e/b")]);
            mixed.extend_from_slice(&bare.into_bytes());
            mixed
        }));
        // A payload frame with its transform chain removed.
        cases.push(("a payload frame with no transform chain", {
            rewrite_items(&baseline_bundle(), |item| match item {
                Value::Map(mut entries) if map_get(&entries, "d").is_some() => {
                    entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "x"));
                    Value::Map(entries)
                }
                other => other,
            })
        }));
        // NEW under the medium axis: a frame naming a codec id nothing declares.
        cases.push(("a frame naming an undeclared codec id", {
            rewrite_items(&baseline_bundle(), |item| match item {
                Value::Map(mut entries) if map_get(&entries, "d").is_some() => {
                    for (key, value) in &mut entries {
                        if matches!(key, Value::Text(k) if k == "x") {
                            *value = Value::Array(vec![Value::Integer(9_999.into())]);
                        }
                    }
                    Value::Map(entries)
                }
                other => other,
            })
        }));
        // NEW: a catalog whose zstd-rsyncable entry omits the mandated level.
        cases.push(("a catalog entry with no declared level", {
            rewrite_items(&baseline_bundle(), strip_catalog_levels)
        }));

        for (label, bytes) in cases {
            let universal = crate::gts_profile::validate_mandated_frames(&bytes);
            assert!(
                universal.is_err(),
                "{label}: the universal rule must reject it, or this is not a replay"
            );
            assert!(
                validate_declared_media(&bytes, &declared).is_err(),
                "{label}: the declared-media audit accepted what the universal rule rejects — \
                 the split would then be a WIDENING rather than a strengthening"
            );
        }
    }

    fn iri_term(iri: &str) -> purrdf::gts::model::Term {
        purrdf::gts::model::Term {
            kind: purrdf::gts::model::TermKind::Iri,
            value: Some(iri.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    /// Re-serialize a CBOR sequence, applying `edit` to every item in order.
    fn rewrite_items(bytes: &[u8], mut edit: impl FnMut(Value) -> Value) -> Vec<u8> {
        let (items, torn) = iter_items(bytes);
        assert!(torn.is_none());
        let mut out = Vec::new();
        for (_, item) in items {
            ciborium::ser::into_writer(&edit(item), &mut out).expect("re-serialize");
        }
        out
    }

    /// Drop `level` from every catalog entry of a header item, leaving frames alone.
    fn strip_catalog_levels(item: Value) -> Value {
        fn strip(entries: &mut [(Value, Value)]) {
            for (key, value) in entries.iter_mut() {
                if !matches!(key, Value::Text(k) if k == "cat") {
                    continue;
                }
                if let Value::Map(catalog) = value {
                    for (_, descriptor) in catalog.iter_mut() {
                        if let Value::Map(fields) = descriptor {
                            fields
                                .retain(|(key, _)| !matches!(key, Value::Text(k) if k == "level"));
                        }
                    }
                }
            }
        }
        match item {
            Value::Tag(tag, inner) if tag == SELF_DESCRIBE_TAG => {
                let Value::Map(mut entries) = *inner else {
                    return Value::Tag(tag, inner);
                };
                strip(&mut entries);
                Value::Tag(tag, Box::new(Value::Map(entries)))
            }
            Value::Map(mut entries) if map_get(&entries, "gts").is_some() => {
                strip(&mut entries);
                Value::Map(entries)
            }
            other => other,
        }
    }

    /// Drop the header's `"dct"` map while leaving the catalog's `dct` binding in
    /// place — the exact shape a pack has when its dictionary went missing.
    fn strip_header_dct(bytes: &mut Vec<u8>) {
        let (items, torn) = iter_items(bytes);
        assert!(torn.is_none());
        let mut rewritten = Vec::new();
        for (_, item) in items {
            let item = match item {
                Value::Tag(tag, inner) if tag == SELF_DESCRIBE_TAG => {
                    let Value::Map(mut entries) = *inner else {
                        panic!("a tagged header wraps a map");
                    };
                    entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "dct"));
                    Value::Tag(tag, Box::new(Value::Map(entries)))
                }
                Value::Map(mut entries) if map_get(&entries, "gts").is_some() => {
                    entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "dct"));
                    Value::Map(entries)
                }
                other => other,
            };
            ciborium::ser::into_writer(&item, &mut rewritten).expect("re-serialize");
        }
        *bytes = rewritten;
    }
}
