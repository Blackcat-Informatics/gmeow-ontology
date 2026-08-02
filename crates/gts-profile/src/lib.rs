// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mandatory GMEOW GTS authorship profile.
//!
//! Every payload-bearing frame emitted by GMEOW production code uses one
//! transform, `zstd-rsyncable`, at compression level 12 — small blob frames,
//! large blob frames, the snapshot frame, transformed consumer output, appended
//! `ai-package` segments, and signed release bundles alike. No size threshold
//! may fall back to plain `zstd`, `gzip`, or `identity`.
//!
//! This crate is the SINGLE production gateway to purrdf's GTS-authorship
//! surface. Four doors exist and no fifth:
//!
//! * [`emit_gmeow_gts`] / [`emit_gmeow_gts_with_medium`] — snapshot bundles
//!   composed from a [`purrdf::gts_compose::SnapshotBuilder`]
//!   (the shipped `gmeow.gts`, release bundles, on-demand consumer bundles),
//!   unprimed or under an explicit [`MediumPlan`](purrdf::gts_compose::MediumPlan);
//! * [`dataset_to_gmeow_gts`] — a frozen carrier
//!   [`RdfDataset`](purrdf::RdfDataset) serialized straight to GTS bytes (the
//!   `gmeow convert --to gts` exit);
//! * [`GmeowGtsWriter`] (via [`store_writer`]) — incremental, append-only segment
//!   authorship (the MCP agent-memory, conjecture and candidate stores);
//! * [`compact_gmeow_gts`] — the streamable repack of an append-only store under a
//!   named dictionary.
//!
//! [`validate_mandated_frames`] is the wire-level audit that proves any of them.
//!
//! It is deliberately a LEAF crate: `gmeow-pipeline` depends on `gmeow-math` and
//! `gmeow-music`, so hosting the profile inside `gmeow-pipeline` would make it
//! unreachable from exactly the producers that need it. The `Transform`
//! diagnostic kind lives here for the same reason — leaving it in
//! `gmeow-pipeline` would merely relocate the cycle.

use ciborium::value::Value;
use gmeow_errors::{FindingCategory, Grade, Severity, Standpoint, define_diag_kind};
use purrdf::gts::model::{Quad, Term};
use purrdf::gts::wire::{SELF_DESCRIBE_TAG, iter_items, map_get, unwrap_header};
use purrdf::gts::writer::{FrameOptions, Writer, term_to_wire};
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};

define_diag_kind! {
    /// A hard defect raised inside the native MAXIMAL(G) transform (skolemization,
    /// saturation, projection, GTS emission): a malformed cell, an unparsable
    /// input graph, or a serialization failure. The RDF value is invalid or the
    /// codec refused — a HARD FAIL, never papered over.
    pub struct Transform { message: String }
    code = "pipeline.transform";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "transform error: {}", message;
}

/// Required transform on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_FRAME_TRANSFORM: &str = "zstd-rsyncable";

/// Required zstd compression level on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_ZSTD_LEVEL: i32 = 12;

// `emit_gts` applies its public dist-profile level to every blob and snapshot
// frame.  Turn an upstream default drift into a compile failure rather than a
// silently different committed bundle.
const _: () = assert!(purrdf::gts_compose::DIST_ZSTD_LEVEL == GMEOW_GTS_ZSTD_LEVEL);

fn profile_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(Transform {
        message: message.into(),
    })
}

fn map<'a>(value: &'a Value, context: &str) -> gmeow_errors::Result<&'a [(Value, Value)]> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => Err(profile_error(format!("{context} is not a CBOR map"))),
    }
}

fn integer(value: &Value, context: &str) -> gmeow_errors::Result<i128> {
    match value {
        Value::Integer(value) => Ok(i128::from(*value)),
        _ => Err(profile_error(format!("{context} is not a CBOR integer"))),
    }
}

/// A quad row's term-table index as the CBOR signed integer the GTS wire format
/// stores. Hard-fails rather than truncating: a silently wrapped index would
/// point at the wrong term, so an out-of-range index is a codec fault, not a
/// value to coerce.
fn term_index(index: usize) -> gmeow_errors::Result<i64> {
    i64::try_from(index).map_err(|_| {
        profile_error(format!(
            "GTS quad row term index {index} exceeds the signed 64-bit wire range"
        ))
    })
}

/// True when `item` is a GTS segment header rather than a frame.
///
/// A header is either the self-describe-tagged map the writer mints by default
/// (`Tag(55799, Map)`) or, when the magic tag is suppressed, a bare map carrying
/// the `"gts": "GTS1"` magic. A frame is always an untagged map keyed by
/// `t`/`d`/`x`/`prev`/`id` and never carries `"gts"`, so the two cannot collide.
fn is_segment_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == SELF_DESCRIBE_TAG,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

/// Every codec id a segment header binds to the mandated transform.
///
/// A catalog carries ONE entry per `(codec, dictionary)` pair (§5), so a
/// dictionary-primed pack legitimately declares several `zstd-rsyncable` ids: the
/// unprimed one plus one per pinned dictionary. The mandate is on the CHAIN, not on
/// the arity — every declared entry must be the mandated codec at the mandated level,
/// and every payload frame must reference one of them. Requiring exactly one would
/// have made priming unrepresentable, which is precisely the density the medium axis
/// exists to recover.
fn mandated_codec_ids(
    header: &[(Value, Value)],
    offset: usize,
) -> gmeow_errors::Result<std::collections::BTreeSet<i128>> {
    let catalog = map(
        map_get(header, "cat").ok_or_else(|| {
            profile_error(format!(
                "GTS header at byte offset {offset} has no codec catalog"
            ))
        })?,
        "GTS codec catalog",
    )?;

    let mut ids = std::collections::BTreeSet::new();
    for (id, descriptor) in catalog {
        let descriptor = map(descriptor, "GTS codec descriptor")?;
        if matches!(
            map_get(descriptor, "name"),
            Some(Value::Text(name)) if name == GMEOW_GTS_FRAME_TRANSFORM
        ) {
            // Rule 6 mandates a LEVEL, not just a codec name. The level is a
            // declared catalog parameter (§8.5 `level?`) rather than something
            // recoverable from the compressed bytes, so before it was emitted on
            // the wire this clause was unenforceable on an artifact and the gate
            // that claims to "enforce zstd-rsyncable level 12" could not see it.
            // Now it can: a catalog entry that omits the level, or declares a
            // different one, is a hard failure — and it is checked on EVERY entry,
            // so a dict-bound entry cannot smuggle a different level past the gate.
            let declared = map_get(descriptor, "level").ok_or_else(|| {
                profile_error(format!(
                    "GTS codec catalog at byte offset {offset} declares a {} entry with no level; \
                     the mandated profile is level {GMEOW_GTS_ZSTD_LEVEL}",
                    GMEOW_GTS_FRAME_TRANSFORM
                ))
            })?;
            let declared = integer(declared, "GTS codec level")?;
            if declared != i128::from(GMEOW_GTS_ZSTD_LEVEL) {
                return Err(profile_error(format!(
                    "GTS codec catalog at byte offset {offset} declares {} at level {declared}, \
                     not the mandated {GMEOW_GTS_ZSTD_LEVEL}",
                    GMEOW_GTS_FRAME_TRANSFORM
                )));
            }
            ids.insert(integer(id, "GTS codec id")?);
        }
    }
    if ids.is_empty() {
        return Err(profile_error(format!(
            "GTS codec catalog at byte offset {offset} declares no {} entry",
            GMEOW_GTS_FRAME_TRANSFORM
        )));
    }
    Ok(ids)
}

/// Validate the mandatory transform profile on materialized GMEOW GTS bytes.
///
/// Works on a single bundle (the shipped `gmeow.gts`, a feedback/music/math
/// bundle, a `convert --to gts` output) and equally on an append-only MULTI-
/// SEGMENT file (the MCP agent-memory and conjecture-library `.gts` files): each
/// appended segment mints its own header, and every one of them must bind the
/// mandated transform for the frames that follow it.
///
/// This is intentionally a cheap wire-level audit: it does not fold or parse the
/// RDF payload. Deep semantic validation remains the responsibility of the
/// validation gate. The compile-time assertion above separately pins the only
/// production emitter's zstd level to 12.
///
/// # Errors
/// A torn CBOR sequence, bytes that do not start with a header, a
/// missing/duplicated `zstd-rsyncable` codec-catalog entry in any segment header,
/// a payload-bearing frame with no (or a non-conforming) transform chain, a
/// payload-free frame that nonetheless carries one, or bytes with no payload
/// frames at all.
pub fn validate_mandated_frames(bytes: &[u8]) -> gmeow_errors::Result<()> {
    let (items, torn) = iter_items(bytes);
    if let Some(offset) = torn {
        return Err(profile_error(format!(
            "GTS CBOR sequence is torn at byte offset {offset}"
        )));
    }
    if items.is_empty() {
        return Err(profile_error("GTS bytes carry no header"));
    }
    if !is_segment_header(&items[0].1) {
        return Err(profile_error("GTS bytes do not begin with a header"));
    }

    let mut codec_ids: Option<std::collections::BTreeSet<i128>> = None;
    let mut payload_frames = 0usize;
    for (offset, item) in &items {
        if is_segment_header(item) {
            let header = unwrap_header(item).map_err(|message| {
                profile_error(format!("invalid header at byte offset {offset}: {message}"))
            })?;
            codec_ids = Some(mandated_codec_ids(header, *offset)?);
            continue;
        }

        let required = codec_ids.as_ref().ok_or_else(|| {
            profile_error(format!(
                "GTS frame at byte offset {offset} precedes any segment header"
            ))
        })?;
        let frame = map(item, &format!("GTS frame at byte offset {offset}"))?;
        if map_get(frame, "d").is_none() {
            // A signed release may carry a metadata-only transport-key frame.
            // It has no payload bytes to transform and is not a codec exception.
            if map_get(frame, "x").is_some() {
                return Err(profile_error(format!(
                    "payload-free GTS frame at byte offset {offset} carries a transform chain"
                )));
            }
            continue;
        }

        payload_frames += 1;
        let transforms = match map_get(frame, "x") {
            Some(Value::Array(values)) => values,
            _ => {
                return Err(profile_error(format!(
                    "payload-bearing GTS frame at byte offset {offset} has no transform chain"
                )));
            }
        };
        if transforms.len() != 1 {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} must carry exactly one transform; found {}",
                transforms.len()
            )));
        }
        let transform = integer(&transforms[0], "GTS frame transform id")?;
        if !required.contains(&transform) {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} uses codec id {transform}, \
                 which is not one of this segment's {} entries {required:?}",
                GMEOW_GTS_FRAME_TRANSFORM
            )));
        }
    }

    if payload_frames == 0 {
        return Err(profile_error("GTS bytes carry no payload-bearing frames"));
    }
    Ok(())
}

/// Emit a GMEOW snapshot using the one permitted production frame profile.
///
/// The single production call to [`purrdf::gts_compose::emit_gts`] in the whole
/// workspace; `gmeow_validate::repo_static`'s Seal A censuses that and hard-fails
/// a second one.
///
/// # Errors
/// A composition or codec failure inside `emit_gts` (including its
/// all-three-or-none signing precondition).
#[allow(clippy::too_many_arguments)]
pub fn emit_gmeow_gts(
    builder: &SnapshotBuilder,
    archive_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
) -> gmeow_errors::Result<Vec<u8>> {
    emit_gmeow_gts_with_medium(
        builder,
        archive_blobs,
        report_blobs,
        signer_secret,
        signer_kid,
        public_key_armor,
        &baseline_medium_plan(),
    )
}

/// The mandated no-dictionary medium: the one permitted transform chain with
/// [`GMEOW_GTS_ZSTD_LEVEL`] declared explicitly in the catalog.
///
/// The level is CHAIN-gated, never profile-gated. Upstream used to grant level 12
/// only under the literal header profile string `"dist"`, so a producer with an
/// honest profile of its own silently dropped to the encoder default — the
/// `zstd_level` dist/main regression class. Declaring it here means every GMEOW
/// authorship door emits at 12 regardless of what its profile string says, and
/// the level is readable back off the artifact rather than being an
/// emitter-side article of faith.
#[must_use]
pub fn baseline_medium_plan() -> purrdf::gts_compose::MediumPlan {
    purrdf::gts_compose::MediumPlan::undicted(Some(GMEOW_GTS_ZSTD_LEVEL))
}

/// Emit a GMEOW snapshot under the mandated frame profile with an explicit
/// medium plan — the dictionary-carrying door.
///
/// [`emit_gmeow_gts`] is this function at [`baseline_medium_plan`]; the pipeline's
/// medium registry supplies a dict-bearing plan instead.
///
/// # Errors
/// A composition or codec failure inside `emit_gts` (including its
/// all-three-or-none signing precondition), or a frame slot missing from a
/// non-empty plan's total assignment.
#[allow(clippy::too_many_arguments)]
pub fn emit_gmeow_gts_with_medium(
    builder: &SnapshotBuilder,
    archive_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
    medium: &purrdf::gts_compose::MediumPlan,
) -> gmeow_errors::Result<Vec<u8>> {
    purrdf::gts_compose::emit_gts(
        builder,
        "dist",
        Some(vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()]),
        archive_blobs,
        report_blobs,
        signer_secret,
        signer_kid,
        public_key_armor,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        medium,
    )
    .map_err(|message| gmeow_errors::Diag::of_kind(Transform { message }))
}

/// Serialize a frozen carrier [`RdfDataset`](purrdf::RdfDataset) to GMEOW GTS
/// bytes under the mandated frame profile.
///
/// This is the transformed-consumer-output door (`gmeow convert --to gts`). It
/// deliberately does NOT route through [`purrdf::gts_write::to_gts`]: that path
/// authors its `terms`/`quads`/`reifies`/`annot`/`blob` frames through
/// `Writer::deterministic`, which passes no transform chain at all, so its bytes
/// ship identity-framed. Composing the same dataset through a
/// [`SnapshotBuilder`] and [`emit_gmeow_gts`] yields the identical snapshot
/// shape the shipped `gmeow.gts` carries — one canonical GMEOW authorship form,
/// not two.
///
/// # Errors
/// A carrier term that is not directly representable in the snapshot frame (a
/// quoted-triple term outside the reifier/annotation tables), or a codec failure.
pub fn dataset_to_gmeow_gts(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<Vec<u8>> {
    let mut builder = SnapshotBuilder::new();
    builder
        .add_dataset(dataset)
        .map_err(|message| gmeow_errors::Diag::of_kind(Transform { message }))?;
    emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
}

/// An append-only GTS segment writer that stamps the mandated transform profile
/// on every frame it authors.
///
/// purrdf's `Writer::add_terms` / `add_quads` convenience methods hard-code
/// `transform: None`, so a bare `Writer` emits payload frames with no transform
/// chain at all — invisible to a seal that only watches `emit_gts`, and rejected
/// by [`validate_mandated_frames`]. This wrapper authors the same two frame types
/// through `add_frame_with_options`, carrying
/// [`GMEOW_GTS_FRAME_TRANSFORM`] at [`GMEOW_GTS_ZSTD_LEVEL`].
///
/// A writer built by [`GmeowGtsWriter::new`] carries no pack dictionary — the
/// dictionary-primed store lane goes through [`store_writer`] instead, which is the
/// one door that decides between minting a header and continuing an existing
/// segment's chain.
pub struct GmeowGtsWriter {
    inner: Writer,
    /// The in-band pack dictionary every authored frame primes with, when this
    /// writer's segment declares one. `None` is the explicit no-dictionary
    /// selection, never "the caller forgot".
    dict: Option<String>,
    /// How many term rows this writer's segment ALREADY carries.
    ///
    /// Term ids are SEGMENT-scoped and cumulative across a segment's `terms` frames
    /// (spec §7.5). A writer that MINTS a header starts at 0; one that CONTINUES an
    /// existing segment starts at that segment's current term count, and every quad
    /// row it authors must be shifted by the same base — otherwise the appended
    /// rows would silently point at the term table of whatever was appended before
    /// them. [`Self::add_quads`] applies the shift, so callers keep authoring
    /// frame-local indices and cannot get this wrong.
    term_base: usize,
}

impl GmeowGtsWriter {
    /// Mint a segment writer for `profile` and emit its header (the chain genesis).
    ///
    /// The header's catalog DECLARES [`GMEOW_GTS_ZSTD_LEVEL`], not merely encodes
    /// at it. `Writer::new`'s default catalog carries no level, so a segment
    /// authored through it would satisfy the codec-name half of Rule 6 while
    /// leaving the level unstated — and therefore unverifiable — on the artifact.
    #[must_use]
    pub fn new(profile: &str) -> Self {
        let options = purrdf::gts::writer::WriterOptions {
            zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
            ..Default::default()
        };
        Self {
            inner: Writer::with_options(profile, options)
                .expect("declaring the mandated level on the default catalog is always valid"),
            dict: None,
            term_base: 0,
        }
    }

    /// Append a `terms` frame under the mandated transform profile, returning the
    /// frame id.
    ///
    /// # Errors
    /// A codec failure encoding the frame payload.
    pub fn add_terms(&mut self, terms: &[Term]) -> gmeow_errors::Result<Vec<u8>> {
        // A term's `datatype` / `reifier` slots are TERM IDS, and they are
        // segment-scoped exactly as a quad row's are (§7.5). An appended frame
        // therefore has to shift them by the same base — a typed literal whose
        // datatype id was left frame-local would resolve to whatever term happens to
        // sit at that index in the earlier part of the segment, which is how a
        // `xsd:dateTime` literal ends up claiming another literal as its datatype.
        let shifted: Vec<Term>;
        let terms = if self.term_base == 0 {
            terms
        } else {
            let base = self.term_base;
            shifted = terms
                .iter()
                .map(|term| Term {
                    datatype: term.datatype.map(|id| id + base),
                    reifier: term.reifier.map(|id| id + base),
                    ..term.clone()
                })
                .collect();
            &shifted
        };
        let payload = Value::Array(terms.iter().map(term_to_wire).collect());
        self.add_mandated_frame("terms", payload)
    }

    /// Append a `quads` frame under the mandated transform profile, returning the
    /// frame id. The graph slot is dropped when `None`, exactly as purrdf's own
    /// `add_quads` row encoding does.
    ///
    /// # Errors
    /// A term index outside the CBOR signed-64-bit wire range, or a codec failure
    /// encoding the frame payload.
    pub fn add_quads(&mut self, quads: &[Quad]) -> gmeow_errors::Result<Vec<u8>> {
        let mut rows: Vec<Value> = Vec::with_capacity(quads.len());
        for &(s, p, o, g) in quads {
            let mut row = Vec::with_capacity(3 + usize::from(g.is_some()));
            for slot in [Some(s), Some(p), Some(o), g].into_iter().flatten() {
                // Shift by the segment's existing term count (§7.5): the caller
                // authored frame-local indices, and in an appended frame those are
                // not the segment-local ids the reader will resolve.
                row.push(Value::Integer(term_index(slot + self.term_base)?.into()));
            }
            rows.push(Value::Array(row));
        }
        self.add_mandated_frame("quads", Value::Array(rows))
    }

    /// How many term rows this writer's segment already carried before it was
    /// opened — 0 for a freshly minted header.
    #[must_use]
    pub fn term_base(&self) -> usize {
        self.term_base
    }

    fn add_mandated_frame(
        &mut self,
        frame_type: &str,
        payload: Value,
    ) -> gmeow_errors::Result<Vec<u8>> {
        let options = FrameOptions {
            payload: Some(payload),
            transform: vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()],
            zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
            dict: self.dict.clone(),
            ..FrameOptions::default()
        };
        self.inner
            .add_frame_with_options(frame_type, options)
            .map_err(|err| {
                gmeow_errors::Diag::of_kind(Transform {
                    message: format!("{frame_type} frame: {err}"),
                })
            })
    }

    /// Consume the writer and return the authored segment bytes.
    ///
    /// For a writer built by [`store_writer`] over an existing store this is ONLY
    /// the appended frames — the segment header is not repeated — so the caller
    /// appends them to the file as-is.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.into_bytes()
    }
}

/// The medium an append-only GMEOW store is written through: the in-band pack
/// dictionary its segments pin and prime with.
///
/// A plain `(name, bytes)` pair rather than an `Option`: a store lane that could
/// pass "no dictionary" by forgetting a field is exactly the silent density loss the
/// medium axis exists to remove. A deliberately unprimed store authors through
/// [`GmeowGtsWriter::new`] instead, which says so in its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreMedium {
    /// The `gmeow:dictionaryId` the segment header pins under `"dct"` (§5).
    pub dictionary: String,
    /// The trained dictionary bytes, stored uncompressed and in-band.
    pub bytes: Vec<u8>,
}

/// Author the next segment of an append-only, dictionary-primed GMEOW store.
///
/// `existing` is the store file's current bytes (empty for a store that does not
/// exist yet). Exactly one of two things happens, and BOTH produce a dict-primed
/// segment — the choice is only whether a new header is needed:
///
/// * the file's last segment already pins `medium.dictionary` → the writer
///   CONTINUES that segment (`Writer::appending`), so the store pays one header —
///   and one copy of the dictionary — per FILE rather than per record. At ~16 KiB
///   of dictionary against a few hundred bytes of claim, a header per record would
///   cost more than the priming saves;
/// * it does not (an empty file, or a tail written through a different medium) → a
///   NEW segment header is minted, pinning the dictionary. A segment declares ONE
///   codec catalog, so a medium change REQUIRES a segment boundary; the earlier
///   segments keep their own headers and decode under their own declared medium,
///   which is why a mixed file is not a degraded read but several honest ones.
///
/// # Errors
/// `existing` is non-empty but carries no readable segment header, ends in a torn
/// append, or has a chain that cannot be walked to a head id.
pub fn store_writer(
    profile: &str,
    existing: &[u8],
    medium: &StoreMedium,
) -> gmeow_errors::Result<GmeowGtsWriter> {
    if !existing.is_empty() {
        let state = purrdf::gts::reader::segment_append_state(existing).map_err(profile_error)?;
        if state.dicts.contains_key(&medium.dictionary) {
            let inner =
                Writer::appending(existing).map_err(|err| profile_error(err.to_string()))?;
            // The segment's CURRENT term count — the base every appended quad row is
            // shifted by. Read off the LAST segment's own fold, never the
            // cross-segment union, whose by-value merge would under-count the ids
            // actually in use.
            let term_base = purrdf::gts::reader::read_file_segments(existing)
                .segments
                .last()
                .map_or(0, |segment| segment.terms.len());
            return Ok(GmeowGtsWriter {
                inner,
                dict: Some(medium.dictionary.clone()),
                term_base,
            });
        }
    }
    let options = purrdf::gts::writer::WriterOptions {
        zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
        dicts: vec![(medium.dictionary.clone(), medium.bytes.clone())],
        ..Default::default()
    };
    let inner = Writer::with_options(profile, options).map_err(|err| {
        profile_error(format!(
            "pinning dictionary {:?} in a {profile} segment header: {err}",
            medium.dictionary
        ))
    })?;
    Ok(GmeowGtsWriter {
        inner,
        dict: Some(medium.dictionary.clone()),
        term_base: 0,
    })
}

/// The in-band pack dictionaries the LAST segment of `bytes` pins, by name (§5
/// header `"dct"`).
///
/// This is how a runtime reads a shipped dictionary out of the bundle it already
/// holds: the dictionaries travel IN BAND, so a consumer priming its own store never
/// needs a second artifact, a network fetch, or a repo checkout.
///
/// # Errors
/// `bytes` carries no readable segment header or ends in a torn append.
pub fn segment_dictionaries(
    bytes: &[u8],
) -> gmeow_errors::Result<std::collections::BTreeMap<String, Vec<u8>>> {
    Ok(purrdf::gts::reader::segment_append_state(bytes)
        .map_err(profile_error)?
        .dicts)
}

/// Whether the last segment of `existing` pins `dictionary` — i.e. whether the next
/// record appended to this store continues that segment or opens a new one.
///
/// Exposed so a caller can OPEN the new segment before handing the file to a writer
/// that only knows how to continue one (`purrdf`'s `agent_memory::Memory`), rather
/// than discovering the medium change as a missing-catalog-entry failure mid-write.
///
/// # Errors
/// `existing` is non-empty but carries no readable segment header.
pub fn store_tail_pins(existing: &[u8], dictionary: &str) -> gmeow_errors::Result<bool> {
    if existing.is_empty() {
        return Ok(false);
    }
    let state = purrdf::gts::reader::segment_append_state(existing).map_err(profile_error)?;
    Ok(state.dicts.contains_key(dictionary))
}

/// The bytes of a header-only segment pinning `medium` — the segment boundary a
/// MEDIUM CHANGE requires.
///
/// A GTS segment declares exactly one codec catalog (spec §5), so a store whose tail
/// was written through a different medium cannot simply keep appending: the new
/// frames would name a catalog id the tail's header never declared. Opening a fresh
/// header is the whole fix, and it costs one header — the dictionary is then paid
/// once for every record that follows, exactly as it is for a fresh store.
///
/// # Errors
/// The dictionary name is empty, or the catalog cannot bind it.
pub fn open_store_segment(profile: &str, medium: &StoreMedium) -> gmeow_errors::Result<Vec<u8>> {
    let options = purrdf::gts::writer::WriterOptions {
        zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
        dicts: vec![(medium.dictionary.clone(), medium.bytes.clone())],
        ..Default::default()
    };
    Ok(Writer::with_options(profile, options)
        .map_err(|err| {
            profile_error(format!(
                "opening a {profile} segment pinned to {:?}: {err}",
                medium.dictionary
            ))
        })?
        .into_bytes())
}

/// Compact a GMEOW-authored GTS file into ONE streamable segment primed by a named
/// dictionary — the fourth (and last) production door onto purrdf's authorship
/// surface.
///
/// Compaction re-authors ORDERING only: content claims are rewrite-invariant, so the
/// compacted file carries the same statements at a different layout and, here, under
/// a different medium. Either way the dictionary rides the new header in band, so the
/// compacted file stays self-decoding without the bundle.
///
/// Where the dictionary bytes come from is `strategy`'s choice, and the two cases have
/// different reproducibility footings:
///
/// * a DERIVED strategy (`DictStrategy::Trained` / `DictStrategy::RawContent`) builds
///   them from the pack's OWN content-blob corpus, so the result is byte-reproducible
///   from `data` alone — and requires `data` to HAVE content blobs;
/// * `DictStrategy::Pinned` uses the caller's bytes verbatim (no training, no corpus
///   derivation, no truncation), so the result is byte-reproducible from `data` PLUS
///   those bytes, and a blob-less pack — a `terms`/`quads`-only store log — compacts
///   exactly like any other. That is the strategy a GMEOW runtime store uses, because
///   a derived dictionary would bind pack-local bytes to the shipped
///   `gmeow:dictionaryId` and leave one id naming several byte sequences.
///
/// The packaging signature is MANDATORY upstream (a plain tuple, not an `Option`),
/// and it is deliberately not widened here: an unsigned repack would be a pack whose
/// ordering commitment nobody attests.
///
/// # Errors
/// The input is not safely compactable (refuse-don't-trust), a blob cannot be
/// decoded, the dictionary cannot be built, or the writer rejects the plan.
pub fn compact_gmeow_gts(
    data: &[u8],
    timestamp: &str,
    dictionary: &str,
    strategy: purrdf::gts::compact::DictStrategy,
    packaging_signer: (ed25519_dalek::SigningKey, String),
) -> gmeow_errors::Result<Vec<u8>> {
    purrdf::gts::compact::compact_streamable(
        data,
        purrdf::gts::compact::CompactionParams {
            timestamp,
            seal_original: false,
            plan: purrdf::gts::compact::DictPlan::rsyncable(
                dictionary,
                strategy,
                GMEOW_GTS_ZSTD_LEVEL,
            ),
            content_digest: None,
            packaging_signer,
        },
    )
    .map_err(|err| profile_error(format!("compact a GMEOW store under {dictionary:?}: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_builder() -> SnapshotBuilder {
        let dataset = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> <https://e/o> .\n",
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(&dataset).expect("add fixture");
        builder
    }

    #[test]
    fn production_profile_pins_transform_and_level() {
        assert_eq!(GMEOW_GTS_FRAME_TRANSFORM, "zstd-rsyncable");
        assert_eq!(GMEOW_GTS_ZSTD_LEVEL, 12);
        assert_eq!(purrdf::gts_compose::DIST_ZSTD_LEVEL, 12);

        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(
            &builder,
            vec![BlobRow {
                data: b"small payload must not fall back to plain zstd".to_vec(),
                media_type: "text/plain".to_string(),
                rep: "profile-test".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("emit fixture");
        validate_mandated_frames(&bytes).expect("fixture uses mandated frame profile");
    }

    #[test]
    fn transform_kind_keeps_its_registered_code() {
        assert_eq!(Transform::CODE, "pipeline.transform");
    }

    #[test]
    fn profile_validator_rejects_a_payload_without_a_transform_chain() {
        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
            .expect("emit fixture");
        let (mut items, torn) = iter_items(&bytes);
        assert!(torn.is_none());
        let payload = items
            .iter_mut()
            .skip(1)
            .find_map(|(_, item)| match item {
                Value::Map(entries) if map_get(entries, "d").is_some() => Some(entries),
                _ => None,
            })
            .expect("fixture has a payload frame");
        payload.retain(|(key, _)| !matches!(key, Value::Text(value) if value == "x"));

        let mut malformed = Vec::new();
        for (_, item) in items {
            ciborium::ser::into_writer(&item, &mut malformed).expect("serialize fixture item");
        }
        let error = validate_mandated_frames(&malformed).expect_err("missing transform must fail");
        assert!(
            error.to_string().contains("has no transform chain"),
            "{error}"
        );
    }

    #[test]
    fn dataset_exit_carries_the_mandated_profile_and_reads_back() {
        let dataset = purrdf::parse_dataset(
            concat!(
                "<https://e/s> <https://e/p> <https://e/o> .\n",
                "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
                "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            )
            .as_bytes(),
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let bytes = dataset_to_gmeow_gts(&dataset).expect("serialize the carrier exit");
        validate_mandated_frames(&bytes).expect("carrier exit uses the mandated frame profile");
        let graph = purrdf::gts::reader::read(&bytes, false, None);
        assert!(!graph.quads.is_empty(), "the exit bytes read back as quads");
    }

    /// A bare `purrdf` `Writer` — the shape every non-`emit_gts` authorship path
    /// used to take — emits payload frames with NO transform chain. Pin the
    /// counter-example so the wrapper below is demonstrably load-bearing.
    #[test]
    fn a_bare_purrdf_writer_fails_the_mandated_profile() {
        // A bare writer violates the profile twice: its catalog declares no level,
        // and its frames carry no transform chain. Pin BOTH independently, so
        // neither rejection can mask a regression in the other.
        let mut writer = Writer::new("ai-package");
        writer.add_terms(&[iri_term("https://e/s")]);
        let error = validate_mandated_frames(&writer.into_bytes())
            .expect_err("a bare writer must fail the profile");
        assert!(error.to_string().contains("no level"), "{error}");

        // Now grant it the declared level and nothing else: the frame-level
        // violation must still stand on its own.
        let options = purrdf::gts::writer::WriterOptions {
            zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
            ..Default::default()
        };
        let mut levelled =
            Writer::with_options("ai-package", options).expect("declared level is valid");
        levelled.add_terms(&[iri_term("https://e/s")]);
        let error = validate_mandated_frames(&levelled.into_bytes())
            .expect_err("a level-declaring bare writer still authors untransformed frames");
        assert!(
            error.to_string().contains("has no transform chain"),
            "{error}"
        );
    }

    /// An append-only file concatenates whole segments, each with its own header.
    /// The audit must walk every segment (not stop at the first) and must not
    /// mistake a later header for a malformed frame.
    #[test]
    fn multi_segment_append_is_audited_segment_by_segment() {
        let mut appended = mandated_segment("https://e/a");
        appended.extend_from_slice(&mandated_segment("https://e/b"));
        validate_mandated_frames(&appended).expect("every appended segment is audited");

        // A second segment authored WITHOUT the profile is caught, proving the walk
        // does not stop after the first header. A bare `Writer` violates the profile
        // twice over — its catalog declares no level and its frames carry no
        // transform chain — and either rejection is correct, so the assertion binds
        // to the invariant the test actually exists for: the failure is attributed
        // to the SECOND segment, i.e. the walk did not stop at the first header.
        let first = mandated_segment("https://e/a");
        let boundary = first.len();
        let mut mixed = first;
        let mut bare = Writer::new("ai-package");
        bare.add_terms(&[iri_term("https://e/b")]);
        mixed.extend_from_slice(&bare.into_bytes());
        let error =
            validate_mandated_frames(&mixed).expect_err("an unprofiled appended segment must fail");
        let offset: usize = error
            .to_string()
            .split("byte offset ")
            .nth(1)
            .and_then(|rest| {
                rest.split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|digits| digits.parse().ok())
            })
            .unwrap_or_else(|| panic!("the failure must name a byte offset: {error}"));
        assert!(
            offset >= boundary,
            "the failure must be attributed to the appended segment at or past byte {boundary}, \
             not the conforming first one: {error}"
        );
    }

    fn iri_term(iri: &str) -> Term {
        Term {
            kind: purrdf::gts::model::TermKind::Iri,
            value: Some(iri.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn mandated_segment(iri: &str) -> Vec<u8> {
        let mut writer = GmeowGtsWriter::new("ai-package");
        writer.add_terms(&[iri_term(iri)]).expect("terms frame");
        writer.into_bytes()
    }

    #[test]
    fn segment_writer_stamps_the_mandated_profile_on_every_frame() {
        let terms = vec![
            iri_term("https://e/s"),
            iri_term("https://e/p"),
            iri_term("https://e/o"),
        ];
        let mut writer = GmeowGtsWriter::new("ai-package");
        writer.add_terms(&terms).expect("terms frame");
        writer.add_quads(&[(0, 1, 2, None)]).expect("quads frame");
        let bytes = writer.into_bytes();
        validate_mandated_frames(&bytes).expect("segment writer uses the mandated frame profile");

        // The segment still reads back as the quad it encodes — the transform is
        // decoded transparently, not a write-only stamp.
        let graph = purrdf::gts::reader::read(&bytes, false, None);
        assert_eq!(graph.quads.len(), 1, "one quad round-trips");
    }
}
